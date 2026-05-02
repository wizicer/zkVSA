use clap::Parser;
use serde::{Deserialize, Serialize};
use std::fs::create_dir_all;
use std::path::{Path, PathBuf};

use wav_pitchshift::pitch;
use flacenc::component::BitRepr;
use flacenc::error::Verify;
use flacenc::bitsink::ByteSink;
use flacenc::config::Encoder as FlacEncoderConfig;
use flacenc::source::MemSource;

#[derive(Parser, Debug)]
#[command(name = "generate_site_data", about = "Generate sample.json and corresponding FLAC/CBOR files for site data")]
struct Cli {
    /// Input FLAC file path
    #[arg(required = true)]
    input_flac: PathBuf,

    /// Output directory for generated files
    #[arg(required = true)]
    output_dir: PathBuf,

    /// Use half-up rounding for F377 algorithm (default: false)
    #[arg(long, default_value_t = false)]
    use_half_up: bool,
}

#[derive(Serialize, Deserialize)]
struct SampleEntry {
    transcript: String,
    name: String,
}

fn decode_flac_to_mono_i16(path: &Path) -> Result<(Vec<i16>, u32), String> {
    let mut reader = claxon::FlacReader::open(path)
        .map_err(|e| format!("Failed to open FLAC {:?}: {}", path, e))?;
    
    let si = reader.streaminfo();
    let sample_rate = si.sample_rate;
    let channels = si.channels as usize;
    let bits_per_sample = si.bits_per_sample as u32;

    let mut samples_i16 = Vec::new();

    if channels == 1 {
        // Mono: direct conversion
        for sample_result in reader.samples() {
            let sample: i32 = sample_result
                .map_err(|e| format!("FLAC decode error: {}", e))?;
            
            let converted = if bits_per_sample <= 16 {
                clamp_to_i16(sample)
            } else {
                clamp_to_i16(sample >> (bits_per_sample - 16))
            };
            samples_i16.push(converted);
        }
    } else {
        // Multi-channel: downmix by averaging
        let mut channel_buffer = Vec::with_capacity(channels);
        for sample_result in reader.samples() {
            let sample: i32 = sample_result
                .map_err(|e| format!("FLAC decode error: {}", e))?;
            
            let converted = if bits_per_sample <= 16 {
                sample
            } else {
                sample >> (bits_per_sample - 16)
            };
            
            channel_buffer.push(converted);
            
            if channel_buffer.len() == channels {
                let avg: i64 = channel_buffer.iter().map(|&x| x as i64).sum::<i64>() / channels as i64;
                samples_i16.push(clamp_to_i16(avg as i32));
                channel_buffer.clear();
            }
        }
    }

    Ok((samples_i16, sample_rate))
}

fn clamp_to_i16(value: i32) -> i16 {
    if value > i16::MAX as i32 {
        i16::MAX
    } else if value < i16::MIN as i32 {
        i16::MIN
    } else {
        value as i16
    }
}

fn write_flac_mono_i16(path: &Path, sample_rate: u32, samples: &[i16]) -> Result<(), String> {
    let buf_i32: Vec<i32> = samples.iter().map(|&s| s as i32).collect();
    let channels = 1u32;
    let bits_per_sample = 16u32;
    
    let cfg = FlacEncoderConfig::default()
        .into_verified()
        .map_err(|_| "FLAC encoder config verification failed")?;
    
    let source = MemSource::from_samples(
        &buf_i32,
        channels as usize,
        bits_per_sample as usize,
        sample_rate as usize,
    );
    
    let stream = flacenc::encode_with_fixed_block_size(&cfg, source, cfg.block_size)
        .map_err(|e| format!("FLAC encode failed: {:?}", e))?;
    
    let mut sink = ByteSink::new();
    stream
        .write(&mut sink)
        .map_err(|e| format!("FLAC stream write failed: {:?}", e))?;
    
    std::fs::write(path, sink.as_slice())
        .map_err(|e| format!("Write FLAC {:?} failed: {}", path, e))?;
    
    Ok(())
}

fn move_f377_cbor_files(output_dir: &Path, name: &str, semitone: i32) -> Result<(), String> {
    let input_cbor_src = output_dir.join("input.cbor");
    let output_cbor_src = output_dir.join("output.cbor");
    
    let input_cbor_dst = output_dir.join(format!("{}_f377_{}_input.cbor", name, semitone));
    let output_cbor_dst = output_dir.join(format!("{}_f377_{}_output.cbor", name, semitone));
    
    if input_cbor_src.exists() {
        std::fs::rename(&input_cbor_src, &input_cbor_dst)
            .map_err(|e| format!("Failed to move input.cbor to {:?}: {}", input_cbor_dst, e))?;
    }
    
    if output_cbor_src.exists() {
        std::fs::rename(&output_cbor_src, &output_cbor_dst)
            .map_err(|e| format!("Failed to move output.cbor to {:?}: {}", output_cbor_dst, e))?;
    }
    
    Ok(())
}

fn read_transcript_file(flac_path: &Path) -> Result<String, String> {
    let txt_path = flac_path.with_extension("txt");
    
    if !txt_path.exists() {
        return Err(format!("Transcript file {:?} not found", txt_path));
    }
    
    std::fs::read_to_string(&txt_path)
        .map_err(|e| format!("Failed to read transcript {:?}: {}", txt_path, e))
        .map(|s| s.trim().to_string())
}

fn get_file_stem(path: &Path) -> Result<String, String> {
    path.file_stem()
        .and_then(|s| s.to_str())
        .map(|s| s.to_string())
        .ok_or_else(|| format!("Invalid file name: {:?}", path))
}

fn main() {
    let cli = Cli::parse();

    // Validate input file
    if !cli.input_flac.exists() {
        eprintln!("Error: Input FLAC file {:?} does not exist", cli.input_flac);
        std::process::exit(1);
    }

    // Create output directory
    if let Err(e) = create_dir_all(&cli.output_dir) {
        eprintln!("Error: Failed to create output directory {:?}: {}", cli.output_dir, e);
        std::process::exit(1);
    }

    println!("Generate Site Data");
    println!("  Input FLAC   : {:?}", cli.input_flac);
    println!("  Output dir   : {:?}", cli.output_dir);
    println!("  Use half-up  : {}", cli.use_half_up);

    // Get file name stem
    let name = match get_file_stem(&cli.input_flac) {
        Ok(name) => name,
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    };

    // Read transcript
    let transcript = match read_transcript_file(&cli.input_flac) {
        Ok(transcript) => transcript,
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    };

    // Decode input FLAC
    let (original_samples, sample_rate) = match decode_flac_to_mono_i16(&cli.input_flac) {
        Ok(data) => data,
        Err(e) => {
            eprintln!("Error decoding FLAC: {}", e);
            std::process::exit(1);
        }
    };

    if original_samples.is_empty() {
        eprintln!("Error: Empty audio file");
        std::process::exit(1);
    }

    println!("  Sample rate  : {} Hz", sample_rate);
    println!("  Samples      : {}", original_samples.len());
    println!("  Duration     : {:.2}s", original_samples.len() as f32 / sample_rate as f32);
    println!("  Transcript   : {}", transcript);

    // Copy original FLAC file to output directory
    let original_flac_path = cli.output_dir.join(format!("{}.flac", name));
    if let Err(e) = std::fs::copy(&cli.input_flac, &original_flac_path) {
        eprintln!("Error copying original FLAC: {}", e);
        std::process::exit(1);
    }

    // Generate variants for each algorithm and semitone
    let algorithms = [("f32", pitch::PitchAlgo::PvtsmF32), ("f377", pitch::PitchAlgo::PvtsmF377)];
    let semitones = [-3, -2, -1, 1, 2, 3]; // Exclude 0 as requested

    let mut sample_entries = Vec::new();
    
    // Add single entry for this input file
    sample_entries.push(SampleEntry {
        transcript: transcript.clone(),
        name: name.clone(),
    });

    for (algo_name, algo) in &algorithms {
        for &semitone in &semitones {
            println!("Processing {} with {} semitones...", algo_name, semitone);
            
            // Create CBOR directory for f377 algorithm
            let cbor_dir = match algo {
                pitch::PitchAlgo::PvtsmF377 => Some(cli.output_dir.clone()),
                _ => None,
            };

            // Apply pitch shifting
            let shifted_samples = pitch::pitch_shift_with_algo(
                &original_samples,
                semitone as f32,
                *algo,
                cbor_dir.as_deref(),
                cli.use_half_up
            );

            // Generate output file names
            let variant_name = format!("{}_{}_{}",  name, algo_name, semitone);
            let flac_path = cli.output_dir.join(format!("{}.flac", variant_name));

            // Write shifted FLAC
            if let Err(e) = write_flac_mono_i16(&flac_path, sample_rate, &shifted_samples) {
                eprintln!("Error writing FLAC {:?}: {}", flac_path, e);
                std::process::exit(1);
            }

            // Move f377 CBOR files if this is f377 algorithm
            if matches!(algo, pitch::PitchAlgo::PvtsmF377) {
                if let Err(e) = move_f377_cbor_files(&cli.output_dir, &name, semitone) {
                    eprintln!("Error moving f377 CBOR files: {}", e);
                    std::process::exit(1);
                }
            }

            println!("  Generated: {}.flac", variant_name);
        }
    }

    // Generate samples.json (append to existing if it exists)
    let samples_json_path = cli.output_dir.join("samples.json");
    
    // Read existing samples if file exists
    let mut all_samples = if samples_json_path.exists() {
        match std::fs::read_to_string(&samples_json_path) {
            Ok(content) => {
                match serde_json::from_str::<Vec<SampleEntry>>(&content) {
                    Ok(existing) => existing,
                    Err(e) => {
                        eprintln!("Warning: Could not parse existing samples.json: {}", e);
                        Vec::new()
                    }
                }
            }
            Err(e) => {
                eprintln!("Warning: Could not read existing samples.json: {}", e);
                Vec::new()
            }
        }
    } else {
        Vec::new()
    };
    
    // Append new samples
    all_samples.extend(sample_entries);
    
    let json_content = match serde_json::to_string_pretty(&all_samples) {
        Ok(content) => content,
        Err(e) => {
            eprintln!("Error serializing samples.json: {}", e);
            std::process::exit(1);
        }
    };

    if let Err(e) = std::fs::write(&samples_json_path, json_content) {
        eprintln!("Error writing samples.json: {}", e);
        std::process::exit(1);
    }

    let total_flac_files = 1 + (algorithms.len() * semitones.len()); // original + variants
    let total_cbor_files = semitones.len() * 2; // f377 generates input/output pair for each semitone
    println!("\n== Summary ==");
    println!("  Generated {} FLAC files", total_flac_files);
    println!("  Generated {} CBOR files (f377 input/output pairs for each semitone)", total_cbor_files);
    println!("  Updated samples.json with {} total entries", all_samples.len());
    println!("  All files written to: {:?}", cli.output_dir);
}
