use clap::Parser;

use wav_pitchshift::pitch::utils::semitones_to_factor;

/// Estimate output hop size (hs) from semitone shift(s)
///
/// Formula: hs = ha / factor, where factor = 2^(semitones/12)
#[derive(Parser, Debug)]
#[command(name = "hs_estimator", about = "Estimate hs from semitone shift(s)", version)]
struct Cli {
    /// Base hop size (ha). Default is 128.
    #[arg(long, short = 'a', default_value_t = 128.0)]
    ha: f32,

    /// If provided, compute only for this semitone value (e.g., -2.5). Otherwise prints a table.
    #[arg(long, short = 's')]
    semitone: Option<f32>,

    /// Minimum semitone (inclusive) for the table when --semitone is not provided.
    #[arg(long, default_value_t = -4.0)]
    min: f32,

    /// Maximum semitone (inclusive) for the table when --semitone is not provided.
    #[arg(long, default_value_t = 4.0)]
    max: f32,

    /// Step between semitone values for the table when --semitone is not provided.
    #[arg(long, default_value_t = 1.0)]
    step: f32,
}

fn main() {
    let cli = Cli::parse();

    if cli.ha <= 0.0 {
        eprintln!("Error: ha (base hop size) must be > 0");
        std::process::exit(1);
    }

    if let Some(s) = cli.semitone {
        compute_and_print(cli.ha, s);
    } else {
        let mut min = cli.min;
        let mut max = cli.max;
        if min > max {
            std::mem::swap(&mut min, &mut max);
        }
        if cli.step <= 0.0 {
            eprintln!("Error: step must be > 0");
            std::process::exit(1);
        }

        println!("ha = {:.3}", cli.ha);
        println!("{:<10} {:<14} {:<14} {:<10}", "semitone", "factor", "hs (float)", "hs~int");
        let mut s = min;
        // Include upper bound with a small epsilon to account for float accumulation
        let eps = cli.step.abs() * 1e-6;
        while s <= max + eps {
            compute_and_print(cli.ha, s);
            s += cli.step;
        }
    }
}

fn compute_and_print(ha: f32, semitone: f32) {
    let factor = semitones_to_factor(semitone);
    let hs = ha / factor;
    let hs_int = hs.round() as i32;
    println!(
        "{:<10.3} {:<14.6} {:<14.6} {:<10}",
        semitone, factor, hs, hs_int
    );
}
