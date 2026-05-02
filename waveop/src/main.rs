use clap::Parser;
use serde::Serialize;
use std::fs::{create_dir_all, File};
use std::path::{Path, PathBuf};

mod wav;
mod pitch;
mod spectrogram;
mod cbor;

// WAV parsing/writing and pitch processing are in separate modules:
// - wav.rs: parse_wav_mono_16le, write_wav_mono_16le, WavFmt, WavData
// - pitch.rs: pitch_shift and helpers

const JSON_FIXED_LEN: usize = 40_000;

#[derive(Parser, Debug)]
#[command(name = "wav_pitchshift", version, about = "Pitch-shift WAV (8kHz mono 16-bit) and optional JSON export", long_about = None)]
struct Cli {
    /// Input WAV path (8kHz, mono, 16-bit PCM)
    input: PathBuf,

    /// Output WAV path
    output: PathBuf,

    /// Semitone shift (default 0.0)
    #[arg(default_value_t = 0.0, allow_negative_numbers = true)]
    semitones: f32,

    /// Pitch algorithm (default: ola)
    #[arg(value_enum, default_value_t = pitch::PitchAlgo::OLA)]
    algo: pitch::PitchAlgo,

    /// Optional JSON export path (fixed-length arrays)
    #[arg(long)]
    json: Option<PathBuf>,

    /// Optional spectrogram PNG of output signal
    #[arg(long, name = "spectrogram")]
    spectrogram: Option<PathBuf>,

    /// Optional CBOR output directory for F377 algorithm data
    #[arg(long)]
    cbor_dir: Option<PathBuf>,

    /// Use half-up rounding for F377 algorithm (default: true)
    #[arg(long, default_value_t = true)]
    use_half_up: bool,
}

fn fixed_len_i16_strings(data: &[i16], len: usize) -> (Vec<String>, bool, bool) {
    let mut out: Vec<String> = Vec::with_capacity(len);
    let mut truncated = false;
    let mut padded = false;
    if data.len() > len {
        truncated = true;
    }
    let take_n = data.len().min(len);
    for &s in &data[..take_n] {
        out.push(s.to_string());
    }
    if len > take_n {
        padded = true;
        out.resize(len, "0".to_string());
    }
    (out, truncated, padded)
}

#[derive(Serialize)]
struct ExportArrays {
    input: Vec<String>,
    output: Vec<String>,
}

fn write_json_arrays(json_path: &Path, input: &[i16], output: &[i16]) -> Result<(), String> {
    let (in_arr, in_trunc, in_pad) = fixed_len_i16_strings(input, JSON_FIXED_LEN);
    let (out_arr, out_trunc, out_pad) = fixed_len_i16_strings(output, JSON_FIXED_LEN);

    if let Some(parent) = json_path.parent() { if !parent.as_os_str().is_empty() { create_dir_all(parent).map_err(|e| format!("Failed to create JSON parent dirs: {e}"))?; } }

    let f = File::create(json_path).map_err(|e| format!("Failed to create JSON file: {e}"))?;
    let payload = ExportArrays { input: in_arr, output: out_arr };
    serde_json::to_writer_pretty(f, &payload).map_err(|e| format!("Failed to write JSON: {e}"))?;

    if in_trunc {
        eprintln!("JSON export: input longer than {} samples; truncated.", JSON_FIXED_LEN);
    }
    if in_pad {
        eprintln!("JSON export: input shorter than {} samples; zero-padded.", JSON_FIXED_LEN);
    }
    if out_trunc {
        eprintln!("JSON export: output longer than {} samples; truncated.", JSON_FIXED_LEN);
    }
    if out_pad {
        eprintln!("JSON export: output shorter than {} samples; zero-padded.", JSON_FIXED_LEN);
    }

    Ok(())
}

fn main() {
    let cli = Cli::parse();
    let input_path = cli.input.as_path();
    let output_path = cli.output.as_path();
    let semitones = cli.semitones;
    let algo = cli.algo;

    match wav::parse_wav_mono_16le(input_path) {
        Ok(wav) => {
            if wav.fmt.sample_rate != 8000 {
                eprintln!(
                    "Warning: input sample rate is {} Hz, expected 8000 Hz. Proceeding anyway.",
                    wav.fmt.sample_rate
                );
            }

            let out_samples = pitch::pitch_shift_with_algo(&wav.samples, semitones, algo, cli.cbor_dir.as_deref(), cli.use_half_up);
            if let Some(parent) = output_path.parent() { if !parent.as_os_str().is_empty() { if let Err(e) = create_dir_all(parent) { eprintln!("Failed to create output directory: {e}"); std::process::exit(1); } } }
            if let Err(e) = wav::write_wav_mono_16le(output_path, wav.fmt.sample_rate, &out_samples) {
                eprintln!("Failed to write WAV: {e}");
                std::process::exit(1);
            }
            // Optional: write spectrogram for output
            if let Some(spec_out_path) = cli.spectrogram.as_deref() {
                if let Err(e) = spectrogram::write_spectrogram_png(spec_out_path, &out_samples, wav.fmt.sample_rate) {
                    eprintln!("Failed to write output spectrogram: {e}");
                }
            }
            if let Some(json_path) = cli.json.as_deref() {
                if let Err(e) = write_json_arrays(json_path, &wav.samples, &out_samples) {
                    eprintln!("Failed to export JSON: {e}");
                }
            }
        }
        Err(e) => {
            eprintln!("Failed to parse WAV: {e}");
            std::process::exit(1);
        }
    }
}
