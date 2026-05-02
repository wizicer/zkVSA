# wav_pitchshift

A minimal Rust utility that reads a WAV file in binary (no audio libraries), applies a simple OLA-based pitch shift, and writes a new WAV file. Designed for 8 kHz, 16-bit PCM, mono WAV files.

## Build

Requires Rust toolchain (cargo).

```bash
cargo build --release
```

## Usage

```bash
cargo run -- <input.wav> <output.wav> [semitones] [algo] [--json <path>]
```

- `input.wav`: Expected 8 kHz, 16-bit PCM, mono.
- `semitones` (optional): Float semitone shift (e.g., `3.0`, `-5.5`). Default `0.0`.
- `algo` (optional): Pitch algorithm. Supported: `ola` (default), `resample` (alias: `simple`).
- `--json <path>` (optional): Export a JSON file with fixed-length arrays (40,000 samples) for `input` and `output`. If longer, arrays are truncated (warning printed). If shorter, arrays are zero-padded (warning printed). Each sample is encoded as a string.

Example with the provided sample in `data/`:

```bash
cargo run --bin wav_pitchshift -- data/common_voice_en_42706185_8k.wav target/wav/out_shifted_ola.wav 3.0 ola
```

Example using the simple resampling algorithm and exporting JSON arrays:

```bash
cargo run -- data/common_voice_en_42706185_8k.wav target/wav/out_shifted_resample.wav 3.0 resample --json target/wav/out_shifted_resample.json
```

## Notes

- Reader/writer only supports PCM (format=1), mono channel, 16-bit. Other formats will error.
- Pitch shifting is implemented as:
  - Time-stretch via OLA (Hann window, 50% overlap) by factor `1/pitch_factor`.
  - Linear resampling to restore original duration.
- This is intentionally simple for clarity and does not target artifact-free quality.

## Project Structure

- `src/main.rs` — CLI and orchestration; calls into modules.
- `src/wav.rs` — Binary WAV parser/writer for 8 kHz, 16-bit PCM mono.
- `src/pitch.rs` — Pitch shifting algorithms (currently OLA-based implementation).

## Add/try different pitch algorithms

- Implement new functions in `src/pitch.rs` (e.g., `pitch_shift_psola`, `pitch_shift_phase_vocoder`).
- Update `src/main.rs` to call your new function if you want to change the default.
