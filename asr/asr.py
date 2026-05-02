# Create an ASR baseline script analogous to the user's ASV script.
# - Uses Hugging Face models via mirror (HF_ENDPOINT / HUGGINGFACE_HUB_BASE_URL)
# - Supports multiple common models (Whisper, Wav2Vec2 CTC)
# - Walks a folder of audio files; expects reference text alongside as .txt OR a manifest CSV
# - Produces transcripts.csv, metrics.json (WER/CER), optional per-file JSONL, and timing
# - Simple normalization toggle for scoring
#
# Usage:
#   pip install transformers torchaudio jiwer soundfile accelerate
#   python asr_baseline.py \
#     --data_root /path/to/asr_data/test \
#     --out out_asr \
#     --model_id openai/whisper-small \
#     --hf_endpoint https://hf-mirror.com \
#     --language en \
#     --task transcribe \
#     --save_word_timestamps
#
# Directory layout options:
# A) Paired files (recommended, simplest):
#   /path/to/asr_data/test/
#     utt1.wav
#     utt1.txt        # reference transcript (UTF-8)
#     spk1/utt2.flac  # nested ok
#     spk1/utt2.txt
#
# B) Manifest CSV (alternative):
#   manifest.csv with headers: path,text
#   (paths absolute or relative to --data_root)
#
# Notes:
# - For Whisper models, set --language and --task if needed (transcribe/translate).
# - For CTC models (wav2vec2), language/task flags are ignored.
# - Script resamples to the model's preferred rate when reasonable (16k). Whisper models accept 16k input.

from pathlib import Path
import argparse
import os
import sys
import time
import json
import csv
from typing import List, Tuple, Dict, Optional

import numpy as np
import torch
import torchaudio
import soundfile as sf
from jiwer import wer, cer

def set_hf_endpoint(endpoint: str):
    if endpoint:
        os.environ["HF_ENDPOINT"] = endpoint.rstrip("/")
        os.environ["HUGGINGFACE_HUB_BASE_URL"] = endpoint.rstrip("/")
        os.environ.setdefault("HF_HUB_ENABLE_HF_TRANSFER", "1")

# Set default HF endpoint before importing transformers
# set_hf_endpoint("https://hf-mirror.com")

from transformers import pipeline

# ---- Model presets (you can extend as needed) ----
# Whisper (multilingual, seq2seq) – strong baseline, needs decoder
# CTC (wav2vec2) – lighter, faster baseline
MODEL_PRESETS = {
    # Whisper family (good multilingual; set --language/--task)
    "whisper-small": "openai/whisper-small",
    "whisper-medium": "openai/whisper-medium",
    "whisper-large-v3": "openai/whisper-large-v3",
    # English CTC
    "wav2vec2-960h": "facebook/wav2vec2-large-960h-lv60-self",
    # Multilingual CTC (strong on many languages)
    "xlsr-53": "jonatasgrosman/wav2vec2-large-xlsr-53-english",  # swap to language-specific variants as needed
}

AUDIO_EXTS = {".wav", ".flac", ".mp3", ".ogg", ".m4a"}

def find_audio_with_refs(root: Path) -> List[Tuple[Path, Optional[Path]]]:
    """Find audio files and optional paired .txt references with same stem."""
    items = []
    for p in root.rglob("*"):
        if p.suffix.lower() in AUDIO_EXTS:
            ref = p.with_suffix(".txt")
            items.append((p, ref if ref.exists() else None))
    return items

def load_manifest(manifest_path: Path, data_root: Path) -> List[Tuple[Path, str]]:
    pairs = []
    with manifest_path.open("r", encoding="utf-8") as f:
        reader = csv.DictReader(f)
        if not {"path", "text"}.issubset(reader.fieldnames or {}):
            raise ValueError("manifest.csv must have headers: path,text")
        for row in reader:
            rp = row["path"].strip()
            txt = row["text"]
            p = Path(rp)
            if not p.is_absolute():
                p = (data_root / rp).resolve()
            pairs.append((p, txt))
    return pairs

def load_audio(path: Path, target_sr: int = 16000) -> Tuple[torch.Tensor, int]:
    wav, sr = sf.read(str(path), dtype="float32", always_2d=False)
    if wav.ndim == 2:
        wav = wav.mean(axis=1)
    wav_t = torch.from_numpy(wav).float().unsqueeze(0)  # [1, T]
    if sr != target_sr:
        wav_t = torchaudio.functional.resample(wav_t, sr, target_sr)
        sr = target_sr
    return wav_t.squeeze(0), sr  # [T]

def norm_text(s: str, lower=True, rm_punct=True, collapse_ws=True) -> str:
    import re
    if s is None:
        return ""
    t = s
    if lower:
        t = t.lower()
    if rm_punct:
        t = re.sub(r"[^\w\s]", " ", t, flags=re.UNICODE)
    if collapse_ws:
        t = re.sub(r"\s+", " ", t).strip()
    return t

def main():
    ap = argparse.ArgumentParser(description="ASR baseline with Hugging Face models (Whisper / Wav2Vec2)")
    ap.add_argument("--data_root", required=True, type=str, help="Folder of audios (+ optional .txt refs), OR the folder containing manifest.csv")
    ap.add_argument("--manifest", type=str, default=None, help="Optional CSV manifest (path,text). If absent, will pair *.wav with *.txt of same stem.")
    ap.add_argument("--out", required=True, type=str, help="Output directory")
    ap.add_argument("--model_id", type=str, default="openai/whisper-small", help="HF model id or a preset key (see comments)")
    ap.add_argument("--preset", type=str, default=None, help=f"One of: {', '.join(MODEL_PRESETS.keys())}")
    ap.add_argument("--hf_endpoint", type=str, default="", help="Hugging Face mirror endpoint (e.g. https://hf-mirror.com)")
    ap.add_argument("--device", type=str, default="cuda" if torch.cuda.is_available() else "cpu")
    ap.add_argument("--gpu_id", type=int, default=0, help="GPU device ID to use (default: 0)")
    ap.add_argument("--batch_size", type=int, default=8, help="Batch size for pipeline (where applicable)")
    # Whisper-specific options
    ap.add_argument("--language", type=str, default="en", help="Language code for Whisper (e.g., en, zh, ja). If None, auto-detect.")
    ap.add_argument("--task", type=str, default="transcribe", choices=["transcribe", "translate"], help="Whisper task")
    # Scoring normalization
    ap.add_argument("--no_lower", action="store_true", help="Disable lowercase normalization in scoring")
    ap.add_argument("--keep_punct", action="store_true", help="Keep punctuation in scoring")
    ap.add_argument("--no_ws_collapse", action="store_true", help="Disable whitespace collapse in scoring")
    # Output verbosity
    ap.add_argument("--save_word_timestamps", action="store_true", help="Save word-level timestamps if model supports it")
    args = ap.parse_args()

    # Override HF endpoint if different from default
    if args.hf_endpoint:
        set_hf_endpoint(args.hf_endpoint)

    data_root = Path(args.data_root).resolve()
    out_dir = Path(args.out).resolve()
    out_dir.mkdir(parents=True, exist_ok=True)

    # Resolve model id
    model_id = args.model_id
    if args.preset:
        if args.preset not in MODEL_PRESETS:
            print(f"[ERROR] Unknown preset: {args.preset}. Available: {', '.join(MODEL_PRESETS.keys())}")
            sys.exit(1)
        model_id = MODEL_PRESETS[args.preset]

    # Collect items
    items_with_ref = []
    if args.manifest:
        manifest_path = (data_root / args.manifest) if not Path(args.manifest).is_absolute() else Path(args.manifest)
        pairs = load_manifest(manifest_path, data_root)
        for p, txt in pairs:
            if not p.exists():
                print(f"[WARN] Missing audio in manifest: {p}")
                continue
            items_with_ref.append((p, txt))
    else:
        # auto-pair by .txt with same stem; missing refs allowed (will be excluded from WER/CER)
        pairs = find_audio_with_refs(data_root)
        for audio_path, ref_path in pairs:
            ref_txt = None
            if ref_path is not None:
                try:
                    ref_txt = ref_path.read_text(encoding="utf-8").strip()
                except Exception as e:
                    print(f"[WARN] Failed reading {ref_path}: {e}")
            items_with_ref.append((audio_path, ref_txt))

    if not items_with_ref:
        print(f"[ERROR] No audio found under {data_root}")
        sys.exit(1)

    # Build ASR pipeline
    print(f"[INFO] Loading ASR model: {model_id} on {args.device}")
    # Whisper models benefit from FP16 on GPU
    dtype = "float16" if (args.device.startswith("cuda") and "whisper" in model_id.lower()) else "float32"
    asr = pipeline(
        "automatic-speech-recognition",
        model=model_id,
        device=args.gpu_id if args.device.startswith("cuda") else -1,
        dtype=torch.float16 if dtype == "float16" else torch.float32,
    )

    # Configure whisper-specific params
    gen_kwargs = {}
    if "whisper" in model_id.lower():
        if args.language:
            gen_kwargs["language"] = args.language
        gen_kwargs["task"] = args.task
        if args.save_word_timestamps:
            gen_kwargs["return_timestamps"] = "word"

    # Process
    rows = []
    ref_texts = []
    hyp_texts = []

    t0 = time.time()
    for idx, (audio_path, ref_txt) in enumerate(items_with_ref, 1):
        # Load + resample to 16k for consistency
        wav, sr = load_audio(audio_path, target_sr=16000)
        # Run pipeline (accepts ndarray or path). We pass array to ensure consistent resampling.
        inp = wav.numpy()
        try:
            out = asr(inp, batch_size=args.batch_size, generate_kwargs=gen_kwargs)
            if isinstance(out, list):
                out = out[0]
            hyp = out.get("text", "").strip()
            timestamps = out.get("chunks", None)  # for whisper timestamps
        except Exception as e:
            print(f"[WARN] ASR failed on {audio_path}: {e}")
            hyp = ""
            timestamps = None

        rows.append({
            "path": str(audio_path),
            "ref": ref_txt if ref_txt is not None else "",
            "hyp": hyp,
        })

        if ref_txt is not None:
            ref_texts.append(ref_txt)
            hyp_texts.append(hyp)

        if idx % 20 == 0:
            print(f"[INFO] Decoded {idx}/{len(items_with_ref)}")

        # Optionally save per-utterance word timestamps
        if args.save_word_timestamps and timestamps is not None:
            # write alongside outputs
            ts_path = out_dir / f"timestamps_{audio_path.stem}.json"
            try:
                with ts_path.open("w", encoding="utf-8") as f:
                    json.dump(timestamps, f, ensure_ascii=False, indent=2)
            except Exception as e:
                print(f"[WARN] Failed to save timestamps for {audio_path}: {e}")

    elapsed = time.time() - t0

    # Save transcripts CSV
    csv_path = out_dir / "transcripts.csv"
    with csv_path.open("w", newline="", encoding="utf-8") as f:
        w = csv.writer(f)
        w.writerow(["path", "ref", "hyp"])
        for r in rows:
            w.writerow([r["path"], r["ref"], r["hyp"]])

    # Compute metrics where references exist
    # Normalization for scoring
    lower = not args.no_lower
    rm_punct = not args.keep_punct
    ws_collapse = not args.no_ws_collapse

    if ref_texts:
        refs_n = [norm_text(s, lower, rm_punct, ws_collapse) for s in ref_texts]
        hyps_n = [norm_text(s, lower, rm_punct, ws_collapse) for s in hyp_texts]
        WER = float(wer(refs_n, hyps_n))
        CER = float(cer(refs_n, hyps_n))
    else:
        WER = float("nan")
        CER = float("nan")

    metrics = {
        "model_id": model_id,
        "num_files": len(items_with_ref),
        "num_scored": len(ref_texts),
        "WER": WER,
        "CER": CER,
        "elapsed_sec": elapsed,
        "avg_sec_per_utt": elapsed / max(len(items_with_ref), 1),
        "timestamp": int(time.time()),
        "normalization": {
            "lower": lower, "remove_punctuation": rm_punct, "collapse_whitespace": ws_collapse
        }
    }
    with (out_dir / "metrics.json").open("w", encoding="utf-8") as f:
        json.dump(metrics, f, ensure_ascii=False, indent=2)

    print(f"[RESULT] WER={WER:.4f}, CER={CER:.4f} over {len(ref_texts)}/{len(items_with_ref)} scored files")
    print(f"[INFO] Saved: {csv_path}")
    print(f"[INFO] Saved: {out_dir / 'metrics.json'}")

if __name__ == "__main__":
    main()
