import os
import sys
import argparse
import json
import time
from pathlib import Path
from typing import Dict, List, Tuple

import numpy as np
import soundfile as sf

import torch
import torchaudio
from speechbrain.inference import SpeakerRecognition

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt

import warnings
warnings.filterwarnings("ignore", category=FutureWarning)

def set_hf_endpoint(endpoint: str):
    if endpoint:
        os.environ["HF_ENDPOINT"] = endpoint.rstrip("/")
        # Compatibility with huggingface_hub >=0.24 also honors this:
        os.environ["HUGGINGFACE_HUB_BASE_URL"] = endpoint.rstrip("/")
        # Speed-up binary large files if available (not required)
        os.environ.setdefault("HF_HUB_ENABLE_HF_TRANSFER", "1")


def find_audio_files(root: Path) -> List[Path]:
    exts = {".wav", ".flac", ".mp3", ".ogg", ".m4a"}
    return [p for p in root.rglob("*") if p.suffix.lower() in exts]


def load_wave(path: Path, target_sr: int = 16000) -> torch.Tensor:
    # Returns mono float32 tensor [1, T]
    wav, sr = sf.read(str(path), dtype="float32", always_2d=False)
    if wav.ndim == 2:
        wav = wav.mean(axis=1)
    wav_t = torch.from_numpy(wav).float().unsqueeze(0)  # [1, T]
    if sr != target_sr:
        wav_t = torchaudio.functional.resample(wav_t, sr, target_sr)
    return wav_t


def compute_embedding(recognizer: SpeakerRecognition, wav_t: torch.Tensor) -> np.ndarray:
    # recognizer.encode_batch expects [B, T] waveform between -1..1
    with torch.no_grad():
        emb = recognizer.encode_batch(wav_t).squeeze(0).squeeze(0)  # [C]
    return emb.cpu().numpy()


def average_embeddings(emb_list: List[np.ndarray]) -> np.ndarray:
    if len(emb_list) == 1:
        return emb_list[0]
    M = np.stack(emb_list, axis=0)
    m = M.mean(axis=0)
    # L2 normalize
    m = m / (np.linalg.norm(m) + 1e-12)
    return m


def cosine_score(a: np.ndarray, b: np.ndarray) -> float:
    denom = (np.linalg.norm(a) + 1e-12) * (np.linalg.norm(b) + 1e-12)
    return float(np.dot(a, b) / denom)


def parse_trials(trials_path: Path) -> List[Tuple[str, str, int]]:
    trials = []
    with trials_path.open("r", encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if not line or line.startswith("#"):
                continue
            toks = line.split()
            if len(toks) < 3:
                raise ValueError(f"Bad trials line: {line}")
            spk, rel_test, lab = toks[0], toks[1], toks[2]
            if lab.lower() in ("t", "target", "1", "true", "yes"):
                y = 1
            elif lab.lower() in ("n", "nontarget", "0", "false", "no"):
                y = 0
            else:
                # Try parse as int
                y = int(lab)
                if y not in (0, 1):
                    raise ValueError(f"Bad label in trials: {lab}")
            trials.append((spk, rel_test, y))
    return trials


def build_auto_trials(enroll_dir: Path, test_dir: Path) -> List[Tuple[str, str, int]]:
    # Label positive if the immediate parent dir under test equals spk_id.
    trials = []
    for test_wav in find_audio_files(test_dir):
        # relative path to data_root
        rel = test_wav.relative_to(test_dir.parent).as_posix()
        parent = test_wav.parent.name
        # we don't know all speakers yet; we'll score against every enroll spk later
        trials.append(("__AUTO__", rel, parent))  # placeholder label=parent name
    return trials


def compute_eer(scores: np.ndarray, labels: np.ndarray) -> Tuple[float, float]:
    # Returns (EER, threshold). labels: 1 target, 0 non-target
    # Sort by score descending
    idx = np.argsort(-scores)
    scores_sorted = scores[idx]
    labels_sorted = labels[idx]

    P = labels.sum()
    N = len(labels) - P
    if P == 0 or N == 0:
        return float("nan"), float("nan")

    # For k-th threshold (scores_sorted[k] as boundary included),
    # accepted = 0..k. Among them, target_accept = sum(labels_sorted[:k+1])
    # FAR = non_target_accept / N
    # FRR = 1 - target_accept / P = (P - target_accept)/P
    tar_accept = np.cumsum(labels_sorted)
    non_labels_sorted = 1 - labels_sorted
    non_accept = np.cumsum(non_labels_sorted)

    FAR = non_accept / max(N, 1)
    FRR = (P - tar_accept) / max(P, 1)

    # EER is point where FAR and FRR cross; find closest
    diff = np.abs(FAR - FRR)
    k = int(np.argmin(diff))
    eer = 0.5 * (FAR[k] + FRR[k])
    thr = scores_sorted[k]
    return float(eer), float(thr)


def min_dcf(scores: np.ndarray, labels: np.ndarray, p_target=0.01, c_miss=1.0, c_fa=1.0) -> Tuple[float, float]:
    # Sweep thresholds and compute normalized minDCF following NIST SRE style
    # Cost = c_miss * P_target * P_miss + c_fa * (1 - P_target) * P_fa
    # Normalize by min(c_miss*P_target, c_fa*(1-P_target))
    idx = np.argsort(-scores)
    scores_sorted = scores[idx]
    labels_sorted = labels[idx]

    P = labels.sum()
    N = len(labels) - P
    if P == 0 or N == 0:
        return float("nan"), float("nan")

    tar_accept = np.cumsum(labels_sorted)
    non_accept = np.cumsum(1 - labels_sorted)

    P_miss = (P - tar_accept) / max(P, 1)
    P_fa = non_accept / max(N, 1)

    costs = c_miss * p_target * P_miss + c_fa * (1 - p_target) * P_fa
    denom = min(c_miss * p_target, c_fa * (1 - p_target))
    dcf = costs / max(denom, 1e-12)
    k = int(np.argmin(dcf))
    return float(dcf[k]), float(scores_sorted[k])


def plot_det(scores: np.ndarray, labels: np.ndarray, out_png: Path):
    # Simple DET-like curve using ROC points -> convert to miss/FA
    # We'll sample thresholds at unique scores.
    uniq = np.unique(scores)
    miss = []
    fa = []
    P = labels.sum()
    N = len(labels) - P
    for thr in uniq:
        # accept >= thr
        accept = scores >= thr
        tar_acc = (accept & (labels == 1)).sum()
        non_acc = (accept & (labels == 0)).sum()
        miss.append(1 - tar_acc / max(P, 1))
        fa.append(non_acc / max(N, 1))
    miss = np.array(miss)
    fa = np.array(fa)

    plt.figure()
    plt.plot(fa, miss, marker=".")
    plt.xlabel("False Alarm Rate")
    plt.ylabel("Miss Rate")
    plt.title("DET Curve (approx)")
    plt.grid(True, which="both")
    plt.savefig(out_png, dpi=200, bbox_inches="tight")
    plt.close()


def format_elapsed_time(seconds: float) -> str:
    """Format elapsed time in human-readable format."""
    if seconds < 60:
        return f"{seconds:.1f}s"
    else:
        minutes = int(seconds // 60)
        secs = seconds % 60
        return f"{minutes:02d}:{secs:04.1f}"


def build_enrollment_embeddings(recognizer: SpeakerRecognition, enroll_dir: Path, sample_rate: int) -> Dict[str, np.ndarray]:
    """Build speaker enrollment embeddings (centroids) from enrollment directory."""
    start_time = time.time()
    print(f"[INFO] Scanning enroll from: {enroll_dir}")
    enroll_spk_dirs = [p for p in enroll_dir.iterdir() if p.is_dir()]
    if not enroll_spk_dirs:
        print(f"[ERROR] No speaker subfolders under {enroll_dir}")
        sys.exit(1)

    enroll_embs: Dict[str, np.ndarray] = {}
    total_files = 0
    for spk_path in sorted(enroll_spk_dirs):
        spk_id = spk_path.name
        wavs = find_audio_files(spk_path)
        if not wavs:
            print(f"[WARN] No audio for speaker {spk_id}, skipping.")
            continue
        emb_list = []
        for w in wavs:
            try:
                wav_t = load_wave(w, sample_rate)
                emb = compute_embedding(recognizer, wav_t)
                emb_list.append(emb)
                total_files += 1
            except Exception as e:
                print(f"[WARN] Failed to process {w}: {e}")
        if not emb_list:
            print(f"[WARN] No valid embeddings for {spk_id}, skipping.")
            continue
        centroid = average_embeddings(emb_list)
        # L2 normalize centroid
        centroid = centroid / (np.linalg.norm(centroid) + 1e-12)
        enroll_embs[spk_id] = centroid

    if not enroll_embs:
        print("[ERROR] No enroll embeddings built.")
        sys.exit(1)
    
    elapsed = time.time() - start_time
    print(f"[INFO] Enrollment completed: {len(enroll_embs)} speakers, {total_files} files, elapsed {format_elapsed_time(elapsed)}")
    return enroll_embs


def process_test_directory(recognizer: SpeakerRecognition, test_dir: Path, data_root: Path, 
                          sample_rate: int) -> Tuple[Dict[str, np.ndarray], Dict[str, str]]:
    """Process a single test directory and return embeddings and label guesses."""
    start_time = time.time()
    print(f"[INFO] Scanning test from: {test_dir}")
    test_wavs = find_audio_files(test_dir)
    if not test_wavs:
        print(f"[ERROR] No audio under {test_dir}")
        return {}, {}

    test_embs: Dict[str, np.ndarray] = {}
    test_labels_guess: Dict[str, str] = {}  # parent dir name as guess label
    processed_files = 0
    for w in sorted(test_wavs):
        rel = w.relative_to(data_root).as_posix()
        try:
            wav_t = load_wave(w, sample_rate)
            emb = compute_embedding(recognizer, wav_t)
            emb = emb / (np.linalg.norm(emb) + 1e-12)
            test_embs[rel] = emb
            test_labels_guess[rel] = w.parent.name
            processed_files += 1
        except Exception as e:
            print(f"[WARN] Failed to process {w}: {e}")

    elapsed = time.time() - start_time
    print(f"[INFO] Test processing completed: {processed_files} files, elapsed {format_elapsed_time(elapsed)}")
    return test_embs, test_labels_guess


def evaluate_test_set(enroll_embs: Dict[str, np.ndarray], test_embs: Dict[str, np.ndarray],
                     test_labels_guess: Dict[str, str], data_root: Path, 
                     trials_file: str = None) -> Tuple[np.ndarray, np.ndarray, List[Tuple]]:
    """Evaluate a test set against enrollment embeddings."""
    # Load or build trials
    trials_path = data_root / trials_file if trials_file else None
    trials: List[Tuple[str, str, int]] = []
    if trials_path and trials_path.exists():
        print(f"[INFO] Using provided trials: {trials_path}")
        trials = parse_trials(trials_path)
    else:
        print("[INFO] trials.txt not found; auto-generating exhaustive trials (label by test parent folder name match).")
        # for every test utterance, compare against all enroll speakers
        for rel, parent in test_labels_guess.items():
            for spk_id in enroll_embs.keys():
                lab = 1 if parent == spk_id else 0
                trials.append((spk_id, rel, lab))

    # Score
    scores = []
    labels = []
    rows = []
    for spk_id, rel_test, y in trials:
        if rel_test not in test_embs:
            print(f"[WARN] test file not embedded (skip): {rel_test}")
            continue
        if spk_id not in enroll_embs:
            print(f"[WARN] speaker not in enroll (skip): {spk_id}")
            continue
        s = cosine_score(enroll_embs[spk_id], test_embs[rel_test])
        scores.append(s)
        labels.append(int(y))
        rows.append((spk_id, rel_test, s, int(y)))

    scores = np.asarray(scores, dtype=np.float32)
    labels = np.asarray(labels, dtype=np.int64)

    return scores, labels, rows


def main():
    parser = argparse.ArgumentParser(description="ASV baseline with SpeechBrain ECAPA-TDNN")
    parser.add_argument("--data_root", type=str, required=True, help="Path to dataset root containing enroll/ and test directories")
    parser.add_argument("--enroll_dir", type=str, default="enroll", help="Relative enroll dir under data_root")
    parser.add_argument("--test_dirs", type=str, nargs="+", help="One or more test directories (relative to data_root)")
    parser.add_argument("--test_dir", type=str, help="Single test directory (for backward compatibility)")
    parser.add_argument("--trials", type=str, default=None, help="Optional trials file under data_root (applied to all test sets)")
    parser.add_argument("--out", type=str, required=True, help="Output directory")
    parser.add_argument("--hf_endpoint", type=str, default="https://hf-mirror.com", help="Hugging Face mirror endpoint")
    parser.add_argument("--model_id", type=str, default="speechbrain/spkrec-ecapa-voxceleb", help="Pretrained model repo id")
    parser.add_argument("--device", type=str, default="cuda" if torch.cuda.is_available() else "cpu")
    parser.add_argument("--gpu_id", type=int, default=None, help="Specific GPU ID to use (e.g., 0, 1, 2). If not specified, uses default CUDA device.")
    parser.add_argument("--sample_rate", type=int, default=16000)
    parser.add_argument("--save_plots", action="store_true", help="Save DET plot")
    args = parser.parse_args()

    # Handle GPU ID specification
    if args.gpu_id is not None:
        if torch.cuda.is_available():
            if args.gpu_id >= torch.cuda.device_count():
                print(f"[ERROR] GPU ID {args.gpu_id} not available. Available GPUs: 0-{torch.cuda.device_count()-1}")
                exit(1)
            args.device = f"cuda:{args.gpu_id}"
            print(f"[INFO] Using GPU {args.gpu_id}: {torch.cuda.get_device_name(args.gpu_id)}")
        else:
            print(f"[WARNING] GPU ID specified but CUDA not available. Using CPU.")
            args.device = "cpu"

    # Handle backward compatibility and determine test directories
    test_dirs = []
    if args.test_dirs:
        test_dirs = args.test_dirs
    elif args.test_dir:
        test_dirs = [args.test_dir]
    else:
        test_dirs = ["test"]  # default
    
    print(f"[INFO] Will process {len(test_dirs)} test directories: {test_dirs}")
    print(f"[INFO] ASV evaluation started at {time.strftime('%Y-%m-%d %H:%M:%S')}")

    data_root = Path(args.data_root).resolve()
    enroll_dir = data_root / args.enroll_dir
    out_dir = Path(args.out).resolve()
    out_dir.mkdir(parents=True, exist_ok=True)

    set_hf_endpoint(args.hf_endpoint)

    # Save config
    config = vars(args).copy()
    config["test_dirs"] = test_dirs  # Add resolved test_dirs to config
    with (out_dir / "run_args.json").open("w", encoding="utf-8") as f:
        json.dump(config, f, indent=2)

    overall_start_time = time.time()
    print(f"[INFO] Starting ASV evaluation...")
    
    print(f"[INFO] Loading pretrained model: {args.model_id} (device={args.device})")
    model_start = time.time()
    recognizer = SpeakerRecognition.from_hparams(
        source=args.model_id,
        savedir=str(out_dir / "pretrained_model"),
        run_opts={"device": args.device},
    )
    model_elapsed = time.time() - model_start
    print(f"[INFO] Model loaded in {format_elapsed_time(model_elapsed)}")

    # Build enroll embeddings (speaker centroids) - shared across all test sets
    enroll_embs = build_enrollment_embeddings(recognizer, enroll_dir, args.sample_rate)
    
    print(f"[INFO] Built enrollment embeddings for {len(enroll_embs)} speakers")
    np.save(out_dir / "enroll_centroids.npy", enroll_embs, allow_pickle=True)

    # Process each test directory
    all_results = {}
    
    for test_dir_name in test_dirs:
        test_start_time = time.time()
        print(f"\n[INFO] ===== Processing test directory: {test_dir_name} =====")
        test_dir = data_root / test_dir_name
        
        if not test_dir.exists():
            print(f"[ERROR] Test directory does not exist: {test_dir}")
            continue
            
        # Create output subdirectory for this test set
        test_out_dir = out_dir / test_dir_name
        test_out_dir.mkdir(parents=True, exist_ok=True)
        
        # Process test directory
        test_embs, test_labels_guess = process_test_directory(
            recognizer, test_dir, data_root, args.sample_rate
        )
        
        if not test_embs:
            print(f"[ERROR] No test embeddings built for {test_dir_name}")
            continue
            
        np.save(test_out_dir / "test_embeddings.npy", test_embs, allow_pickle=True)
        
        # Evaluate against enrollment
        eval_start = time.time()
        scores, labels, rows = evaluate_test_set(
            enroll_embs, test_embs, test_labels_guess, data_root, args.trials
        )
        eval_elapsed = time.time() - eval_start
        
        if scores.size == 0:
            print(f"[ERROR] No scores computed for {test_dir_name}")
            continue
            
        eer, thr_eer = compute_eer(scores, labels)
        dcf, thr_dcf = min_dcf(scores, labels, p_target=0.01, c_miss=1.0, c_fa=1.0)
        
        # Save outputs for this test set
        out_scores = test_out_dir / "scores.csv"
        with out_scores.open("w", encoding="utf-8") as f:
            f.write("spk_id,test_relpath,score,label\n")
            for r in rows:
                f.write(f"{r[0]},{r[1]},{r[2]:.6f},{r[3]}\n")
                
        test_total_elapsed = time.time() - test_start_time
        
        metrics = {
            "test_directory": test_dir_name,
            "EER": float(eer),
            "EER_threshold": float(thr_eer),
            "minDCF@pt=0.01": float(dcf),
            "minDCF_threshold": float(thr_dcf),
            "num_trials": int(len(rows)),
            "num_test_files": int(len(test_embs)),
            "evaluation_time_seconds": float(eval_elapsed),
            "total_time_seconds": float(test_total_elapsed),
            "timestamp": int(time.time())
        }
        
        with (test_out_dir / "metrics.json").open("w", encoding="utf-8") as f:
            json.dump(metrics, f, indent=2)
            
        all_results[test_dir_name] = metrics
        
        print(f"[RESULT] {test_dir_name}: EER = {eer:.4%} at thr={thr_eer:.4f}")
        print(f"[RESULT] {test_dir_name}: minDCF(PT=0.01) = {dcf:.4f} at thr={thr_dcf:.4f}")
        print(f"[INFO] {test_dir_name} completed in {format_elapsed_time(test_total_elapsed)} (eval: {format_elapsed_time(eval_elapsed)})")
        print(f"[INFO] Saved: {out_scores}")
        print(f"[INFO] Saved: {test_out_dir / 'metrics.json'}")
        
        if args.save_plots:
            plot_det(scores, labels, test_out_dir / "det.png")
            print(f"[INFO] Saved: {test_out_dir / 'det.png'}")
    
    overall_elapsed = time.time() - overall_start_time
    
    # Save summary of all results
    summary = {
        "model_id": args.model_id,
        "enroll_dir": args.enroll_dir,
        "enrollment_speakers": len(enroll_embs),
        "test_directories": list(all_results.keys()),
        "results": all_results,
        "total_time_seconds": float(overall_elapsed),
        "timestamp": int(time.time())
    }
    
    with (out_dir / "summary.json").open("w", encoding="utf-8") as f:
        json.dump(summary, f, indent=2)
        
    print(f"\n[INFO] ===== SUMMARY =====")
    print(f"[INFO] Overall elapsed time: {format_elapsed_time(overall_elapsed)}")
    print(f"[INFO] Processed {len(all_results)} test directories with {len(enroll_embs)} enrollment speakers")
    for test_name, result in all_results.items():
        test_time = format_elapsed_time(result['total_time_seconds'])
        print(f"[INFO] {test_name}: EER={result['EER']:.4%}, minDCF={result['minDCF@pt=0.01']:.4f}, time={test_time}")
    print(f"[INFO] Summary saved: {out_dir / 'summary.json'}")


if __name__ == "__main__":
    main()
