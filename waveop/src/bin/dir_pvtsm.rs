use clap::{ArgAction, Parser, ValueEnum};
use std::fs::{create_dir_all, read_dir};
use std::path::{Path, PathBuf};
use std::sync::{Arc, atomic::{AtomicBool, AtomicUsize, Ordering}};
use std::thread;
use std::time::{Duration, Instant};
use rayon::prelude::*;

use wav_pitchshift::pitch;
use wav_pitchshift::wav;
use flacenc::component::BitRepr;
use flacenc::error::Verify;
use flacenc::bitsink::ByteSink;
use flacenc::config::Encoder as FlacEncoderConfig;
use flacenc::source::MemSource;

#[derive(Copy, Clone, Debug, ValueEnum)]
enum AudioFormat {
    Flac,
    Wav,
}

#[derive(Parser, Debug)]
#[command(name = "dir_pvtsm", about = "Scan directory for audio files and apply PV-TSM pitch shifting")]
struct Cli {
    /// Input directory to scan recursively
    #[arg(required = true)]
    input_dir: PathBuf,

    /// Output directory (preserves input structure)
    #[arg(required = true)]
    output_dir: PathBuf,

    /// Output name prefix (e.g., 'test' for test_f32_4 directories)
    #[arg(long, default_value = "test")]
    name: String,

    /// Input file format to process
    #[arg(long, value_enum, default_value_t = AudioFormat::Flac)]
    input_format: AudioFormat,

    /// Output file format
    #[arg(long, value_enum, default_value_t = AudioFormat::Wav)]
    output_format: AudioFormat,

    /// Semitone shift amount
    #[arg(long, default_value_t = 0.0, allow_negative_numbers = true)]
    semitones: f32,

    /// Number of worker threads (default: available CPUs)
    #[arg(long)]
    threads: Option<usize>,

    /// Generate only specific variants (comma-separated: m32,f32,f377). If empty, generates all variants.
    #[arg(long)]
    variants: Option<String>,

    /// Use half-up rounding for F377 algorithm (default: true)
    #[arg(long, default_value_t = true, action = ArgAction::Set)]
    use_half_up: bool,
}

#[derive(Clone)]
struct AudioTask {
    input_path: PathBuf,
    relative_path: PathBuf,
    txt_path: Option<PathBuf>,
}

fn get_file_extension(format: AudioFormat) -> &'static str {
    match format {
        AudioFormat::Flac => "flac",
        AudioFormat::Wav => "wav",
    }
}

fn scan_audio_files(dir: &Path, format: AudioFormat) -> Result<Vec<AudioTask>, String> {
    let ext = get_file_extension(format);
    let mut tasks = Vec::new();
    
    fn scan_recursive(
        current_dir: &Path,
        root_dir: &Path,
        ext: &str,
        tasks: &mut Vec<AudioTask>,
    ) -> Result<(), String> {
        let entries = read_dir(current_dir)
            .map_err(|e| format!("Failed to read directory {:?}: {}", current_dir, e))?;

        for entry in entries {
            let entry = entry.map_err(|e| format!("Directory entry error: {}", e))?;
            let path = entry.path();
            
            if path.is_dir() {
                scan_recursive(&path, root_dir, ext, tasks)?;
            } else if path.extension().and_then(|s| s.to_str()) == Some(ext) {
                let relative_path = path.strip_prefix(root_dir)
                    .map_err(|_| format!("Path {:?} not under root {:?}", path, root_dir))?
                    .to_path_buf();
                
                // Look for corresponding .txt file
                let txt_path = path.with_extension("txt");
                let txt_path = if txt_path.exists() { Some(txt_path) } else { None };
                
                tasks.push(AudioTask {
                    input_path: path,
                    relative_path,
                    txt_path,
                });
            }
        }
        Ok(())
    }

    scan_recursive(dir, dir, ext, &mut tasks)?;
    Ok(tasks)
}

fn decode_audio_file(path: &Path, format: AudioFormat) -> Result<(Vec<i16>, u32), String> {
    match format {
        AudioFormat::Flac => decode_flac_to_mono_i16(path),
        AudioFormat::Wav => decode_wav_to_mono_i16(path),
    }
}

fn encode_audio_file(
    path: &Path,
    format: AudioFormat,
    sample_rate: u32,
    samples: &[i16],
) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        create_dir_all(parent)
            .map_err(|e| format!("Failed to create directory {:?}: {}", parent, e))?;
    }

    match format {
        AudioFormat::Flac => write_flac_mono_i16(path, sample_rate, samples),
        AudioFormat::Wav => wav::write_wav_mono_16le(path, sample_rate, samples)
            .map_err(|e| format!("WAV write error: {}", e)),
    }
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

fn decode_wav_to_mono_i16(path: &Path) -> Result<(Vec<i16>, u32), String> {
    let wav_data = wav::parse_wav_mono_16le(path)?;
    Ok((wav_data.samples, wav_data.fmt.sample_rate))
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

fn clamp_to_i16(value: i32) -> i16 {
    if value > i16::MAX as i32 {
        i16::MAX
    } else if value < i16::MIN as i32 {
        i16::MIN
    } else {
        value as i16
    }
}

fn process_audio_task(
    task: &AudioTask,
    cli: &Cli,
) -> Result<(), String> {
    // Decode input audio
    let (samples, sample_rate) = decode_audio_file(&task.input_path, cli.input_format)?;
    
    if samples.is_empty() {
        return Err("Empty audio file".to_string());
    }

    // Build output paths
    let output_ext = get_file_extension(cli.output_format);
    let stem = task.relative_path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("audio");
    
    let rel_dir = task.relative_path.parent().unwrap_or(Path::new(""));
    
    // Format semitones for directory name (remove decimal if it's a whole number)
    let semitone_str = if cli.semitones.fract() == 0.0 {
        format!("{}", cli.semitones as i32)
    } else {
        format!("{}", cli.semitones)
    };

    // Determine which variants to generate
    let variants_to_generate = if let Some(ref variants_str) = cli.variants {
        if variants_str.trim().is_empty() {
            vec!["m32", "f32", "f377"] // Empty string means all variants
        } else {
            variants_str.split(',').map(|s| s.trim()).collect()
        }
    } else {
        vec!["m32", "f32", "f377"] // No variants specified means all variants
    };

    // Generate each requested variant
    for variant in variants_to_generate {
        let (algo, cbor_dir): (pitch::PitchAlgo, Option<PathBuf>) = match variant {
            "m32" => (pitch::PitchAlgo::PvtsmM32, None),
            "f32" => (pitch::PitchAlgo::PvtsmF32, None),
            "f377" => {
                // // Create CBOR output directory for f377
                // let cbor_dir = cli.output_dir.join(format!("{}_{}_f377_cbor", cli.name, semitone_str));
                (pitch::PitchAlgo::PvtsmF377, None)
            },
            _ => {
                eprintln!("Warning: Unknown variant '{}', skipping", variant);
                continue;
            }
        };

        // Apply pitch shifting
        let output_samples = pitch::pitch_shift_with_algo(&samples, cli.semitones, algo, cbor_dir.as_deref(), cli.use_half_up);
        
        // Write variant
        let variant_dir_name = format!("{}_{}_{}",  cli.name, semitone_str, variant);
        let variant_dir = cli.output_dir.join(&variant_dir_name).join(rel_dir);
        let variant_path = variant_dir.join(format!("{}.{}", stem, output_ext));
        encode_audio_file(&variant_path, cli.output_format, sample_rate, &output_samples)?;
        
        // Copy corresponding .txt file if it exists
        if let Some(ref txt_input_path) = task.txt_path {
            let txt_output_path = variant_dir.join(format!("{}.txt", stem));
            if let Err(e) = std::fs::copy(txt_input_path, &txt_output_path) {
                eprintln!("Warning: Failed to copy txt file {:?} to {:?}: {}", txt_input_path, txt_output_path, e);
            }
        }
    }

    Ok(())
}

fn format_duration(d: Duration) -> String {
    let secs = d.as_secs();
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    if h > 0 {
        format!("{:02}:{:02}:{:02}", h, m, s)
    } else {
        format!("{:02}:{:02}", m, s)
    }
}

fn setup_thread_pool(threads: Option<usize>) -> Result<(), String> {
    let default_threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    
    let threads_to_use = threads.unwrap_or(default_threads);
    
    rayon::ThreadPoolBuilder::new()
        .num_threads(threads_to_use)
        .stack_size(2 * 1024 * 1024)
        .build_global()
        .map_err(|e| format!("Failed to set thread pool: {}", e))
}

fn main() {
    let cli = Cli::parse();

    // Validate input directory
    if !cli.input_dir.exists() {
        eprintln!("Error: Input directory {:?} does not exist", cli.input_dir);
        std::process::exit(1);
    }

    // Setup thread pool
    if let Err(e) = setup_thread_pool(cli.threads) {
        eprintln!("Warning: {}", e);
    }

    // Create output directory
    if let Err(e) = create_dir_all(&cli.output_dir) {
        eprintln!("Error: Failed to create output directory {:?}: {}", cli.output_dir, e);
        std::process::exit(1);
    }

    println!("Directory PV-TSM Processor");
    println!("  Input dir    : {:?}", cli.input_dir);
    println!("  Output dir   : {:?}", cli.output_dir);
    println!("  Output name  : {}", cli.name);
    println!("  Input format : {:?}", cli.input_format);
    println!("  Output format: {:?}", cli.output_format);
    println!("  Semitones    : {}", cli.semitones);
    if let Some(ref variants) = cli.variants {
        if variants.trim().is_empty() {
            println!("  Variants     : all (m32, f32, f377)");
        } else {
            println!("  Variants     : {}", variants);
        }
    } else {
        println!("  Variants     : all (m32, f32, f377)");
    }
    println!("  Threads      : {:?}", cli.threads);
    println!("  Use half-up  : {}", cli.use_half_up);

    // Scan for audio files
    let tasks = match scan_audio_files(&cli.input_dir, cli.input_format) {
        Ok(tasks) => tasks,
        Err(e) => {
            eprintln!("Error scanning directory: {}", e);
            std::process::exit(1);
        }
    };

    let total_tasks = tasks.len();
    if total_tasks == 0 {
        println!("No audio files found with format {:?}", cli.input_format);
        return;
    }

    println!("Found {} audio files to process\n", total_tasks);

    // Progress tracking
    let start_time = Instant::now();
    let processed = Arc::new(AtomicUsize::new(0));
    let succeeded = Arc::new(AtomicUsize::new(0));
    let failed = Arc::new(AtomicUsize::new(0));
    let running = Arc::new(AtomicBool::new(true));

    // Progress reporter thread
    let reporter_handle = {
        let processed = processed.clone();
        let succeeded = succeeded.clone();
        let failed = failed.clone();
        let running = running.clone();
        
        thread::spawn(move || {
            while running.load(Ordering::Relaxed) {
                let p = processed.load(Ordering::Relaxed);
                let ok = succeeded.load(Ordering::Relaxed);
                let fl = failed.load(Ordering::Relaxed);
                let elapsed = start_time.elapsed();
                let pct = (p as f64 / total_tasks as f64) * 100.0;
                
                let eta = if p > 0 {
                    let rate = p as f64 / elapsed.as_secs_f64().max(1e-6);
                    let remaining = (total_tasks.saturating_sub(p)) as f64 / rate;
                    Duration::from_secs_f64(remaining.max(0.0))
                } else {
                    Duration::from_secs(0)
                };

                print!("\r{}/{} ({:.1}%) | elapsed {} | eta {} | ok {} | fail {}",
                    p, total_tasks, pct, format_duration(elapsed), format_duration(eta), ok, fl);
                std::io::Write::flush(&mut std::io::stdout()).unwrap();
                
                thread::sleep(Duration::from_millis(500));
            }
        })
    };

    // Process tasks in parallel
    tasks.par_iter().for_each(|task| {
        match process_audio_task(task, &cli) {
            Ok(()) => {
                succeeded.fetch_add(1, Ordering::Relaxed);
            }
            Err(e) => {
                eprintln!("\nError processing {:?}: {}", task.input_path, e);
                failed.fetch_add(1, Ordering::Relaxed);
            }
        }
        processed.fetch_add(1, Ordering::Relaxed);
    });

    // Stop progress reporter
    running.store(false, Ordering::Relaxed);
    let _ = reporter_handle.join();
    println!();

    // Final summary
    let final_processed = processed.load(Ordering::Relaxed);
    let final_succeeded = succeeded.load(Ordering::Relaxed);
    let final_failed = failed.load(Ordering::Relaxed);
    let total_elapsed = start_time.elapsed();

    println!("\n== Summary ==");
    println!("  Total files  : {}", total_tasks);
    println!("  Processed    : {}", final_processed);
    println!("  Succeeded    : {}", final_succeeded);
    println!("  Failed       : {}", final_failed);
    println!("  Elapsed time : {}", format_duration(total_elapsed));

    if final_failed > 0 {
        std::process::exit(1);
    }
}
