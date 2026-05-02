# ZK-VSA

This is the official implementation of the paper:

> **ZK-VSA: Zero-Knowledge Verifiable Speaker Anonymization Leveraging Phase Vocoder with Time-Scale Modification**
>
> Shuang Liang, Yang Hua, Peishen Yan, Linshan Jiang, Tao Song, Bin Yao, Haibing Guan
>
> *ICASSP 2026 — IEEE International Conference on Acoustics, Speech and Signal Processing*

We propose Verifiable Speaker Anonymization (VSA), a paradigm that enables public verification that a predefined anonymization has been applied while the original speech remains hidden. We instantiate this paradigm as ZK-VSA using ZK-SNARKs: we encode phase vocoder with time-scale modification (PV-TSM) as arithmetic constraints over BLS12-377 finite field, complemented by SNARK-friendly phase handling, and integrate cryptographic commitments with digital signatures for authentication.

## Repository Structure

```
zkAudio/
├── waveop/           # Rust: pitch shifting with ZK-compatible field arithmetic
├── gnark/            # Go: Groth16 ZK circuit (gnark framework, BLS12-377)
├── asr/              # Python: ASR evaluation (Whisper, Wav2Vec2, XLSR-53)
├── asv/              # Python: ASV evaluation (SpeechBrain)
├── corpus/           # LibriSpeech test data download and preparation
- docs
- go-wasm-verifier
```

## Components

### waveop (Rust)

Core audio processing engine. Reads WAV/FLAC files, applies phase-vocoder-based pitch shifting in both standard floating-point (f32) and BLS12-377 finite field (f377) representations, and writes output audio.

**Prerequisites:** Rust toolchain

```bash
cd waveop
cargo build --release
# Single file pitch shift
cargo run --release -- data/common_voice_en_42706185_8k.wav output.wav 3.0 pvtsm
# Batch processing on LibriSpeech corpus
bash generate_test_utterance.sh
```

### gnark (Go)

Groth16 ZK proof circuit built with [gnark](https://github.com/Consensys/gnark) on the BLS12-377 curve. Takes CBOR-encoded input/output pairs from `waveop` and generates/verifies proofs.

**Prerequisites:** Go 1.24+

```bash
cd gnark
go run ./cmd/circuit/ --input <input.cbor> --output <output.cbor>
# Benchmark mode
go run ./cmd/circuit/ --input <input.cbor> --output <output.cbor> --benchmark
```

### ASR (Automatic Speech Recognition Evaluation)

Evaluates speech intelligibility of pitch-shifted audio using HuggingFace ASR models (Whisper, Wav2Vec2, XLSR-53). Reports WER/CER metrics.

**Prerequisites:** Python 3, `pip install transformers torchaudio jiwer soundfile accelerate`

```bash
cd asr
# Run all evaluations
bash run_asr_auto.sh
# Or use Makefile targets
make all
```

### ASV (Automatic Speaker Verification Evaluation)

Evaluates speaker identity preservation using SpeechBrain speaker recognition. Reports EER and similarity scores.

**Prerequisites:** Python 3, `pip install speechbrain soundfile torchaudio matplotlib`

```bash
cd asv
bash run_asv_auto.sh
```

### corpus

Scripts to download and prepare the LibriSpeech test corpus.

```bash
cd corpus
bash download_librispeech.sh
cd librispeech && bash flatten_librispeech.sh
```

## Workflow

1. **Prepare corpus** — Download LibriSpeech test sets and flatten the directory structure.
2. **Generate pitch-shifted variants** — Use `waveop` to produce f32 and f377 variants at various semitone shifts.
3. **Generate ZK proofs** — Use `gnark` to prove correctness of the f377 transformations.
4. **Evaluate** — Run ASR and ASV benchmarks on all variants.

## Citation

If you find this work useful, please cite our paper:

```bibtex
@INPROCEEDINGS{11462079,
  author={Liang, Shuang and Hua, Yang and Yan, Peishen and Jiang, Linshan and Song, Tao and Yao, Bin and Guan, Haibing},
  booktitle={ICASSP 2026 - 2026 IEEE International Conference on Acoustics, Speech and Signal Processing (ICASSP)},
  title={ZK-VSA: Zero-Knowledge Verifiable Speaker Anonymization Leveraging Phase Vocoder with Time-Scale Modification},
  year={2026},
  volume={},
  number={},
  pages={13712-13716},
  keywords={Circuits;Central Processing Unit;Circuit synthesis;Circuits and systems;Logic circuits;Electronic circuits;Vocoders;Protocols;Radio access networks;Regional area networks;Verifiable Speaker Anonymization;Zero-knowledge proof;Acoustical signal processing},
  doi={10.1109/ICASSP55912.2026.11462079}}
```

## License

MIT License