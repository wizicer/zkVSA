use image::{ImageBuffer, Rgb};
use rustfft::{num_complex::Complex, FftPlanner};
use std::path::Path;

// Local helpers to avoid depending on private modules
fn hann_window(n: usize) -> Vec<f32> {
    let mut w = vec![0.0f32; n];
    if n == 0 { return w; }
    for i in 0..n {
        w[i] = 0.5 - 0.5 * (2.0 * std::f32::consts::PI * (i as f32) / (n as f32)).cos();
    }
    w
}

fn i16_to_f32(input: &[i16]) -> Vec<f32> {
    input.iter().map(|&s| (s as f32) / 32768.0).collect()
}

pub fn write_spectrogram_png(path: &Path, samples_i16: &[i16], _sample_rate: u32) -> Result<(), String> {
    if samples_i16.is_empty() {
        return Err("No samples for spectrogram".to_string());
    }

    let x = i16_to_f32(samples_i16);

    // Parameters tuned for 8kHz speech; still OK for nearby rates
    let win_size: usize = 512;
    let hop: usize = win_size / 4; // 75% overlap
    if x.len() < win_size {
        // Pad to one window for a single-frame spectrogram
        let mut padded = x.clone();
        padded.resize(win_size, 0.0);
        return render_spectrogram(&padded, win_size, hop, path);
    }
    render_spectrogram(&x, win_size, hop, path)
}

fn render_spectrogram(x: &[f32], win_size: usize, hop: usize, path: &Path) -> Result<(), String> {
    let frames = 1 + (x.len() - win_size) / hop;
    let n_freq = win_size / 2 + 1;

    // STFT
    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(win_size);
    let win = hann_window(win_size);

    let mut mags_db: Vec<Vec<f32>> = vec![vec![0.0; n_freq]; frames];
    let eps = 1e-9f32;

    for f in 0..frames {
        let pos = f * hop;
        let mut spec: Vec<Complex<f32>> = (0..win_size)
            .map(|i| Complex { re: x[pos + i] * win[i], im: 0.0 })
            .collect();
        fft.process(&mut spec);
        for k in 0..n_freq {
            let re = spec[k].re;
            let im = spec[k].im;
            let mag = (re * re + im * im).sqrt();
            let db = 20.0 * (mag.max(eps)).log10();
            mags_db[f][k] = db;
        }
    }

    // Normalize: map to [0,1] using max over spectrogram and an 80 dB dynamic range
    let mut max_db = f32::NEG_INFINITY;
    for row in &mags_db { for &v in row { if v > max_db { max_db = v; } } }
    let floor_db = max_db - 80.0;

    let width = frames as u32;
    let height = n_freq as u32;
    let mut img: ImageBuffer<Rgb<u8>, Vec<u8>> = ImageBuffer::new(width, height);

    // y=0 is top; map highest freq to top by flipping frequency index
    for f in 0..frames {
        for k in 0..n_freq {
            let v = mags_db[f][k];
            let norm = ((v - floor_db) / (max_db - floor_db).max(1e-3)).clamp(0.0, 1.0);
            let [r, g, b] = viridis_rgb(norm);
            let y = (height - 1 - k as u32) as u32;
            img.put_pixel(f as u32, y, Rgb([r, g, b]));
        }
    }

    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create spectrogram parent dirs: {e}"))?;
        }
    }

    img.save(path).map_err(|e| format!("Failed to save spectrogram PNG: {e}"))
}

// Approximate viridis colormap using 5 control points and linear interpolation.
// Control points from Matplotlib viridis samples.
fn viridis_rgb(t: f32) -> [u8; 3] {
    let t = t.clamp(0.0, 1.0);
    const STOPS: [(f32, [u8; 3]); 5] = [
        (0.0,  [68, 1, 84]),    // dark purple
        (0.25, [59, 82, 139]),  // blue
        (0.50, [33, 145, 140]), // teal
        (0.75, [94, 201, 98]),  // green
        (1.0,  [253, 231, 37]), // yellow
    ];
    // find segment
    let mut i = 0;
    while i + 1 < STOPS.len() && t > STOPS[i + 1].0 { i += 1; }
    if i + 1 == STOPS.len() { return STOPS[i].1; }
    let (t0, c0) = STOPS[i];
    let (t1, c1) = STOPS[i + 1];
    let u = if t1 > t0 { (t - t0) / (t1 - t0) } else { 0.0 };
    [
        ((c0[0] as f32) + (c1[0] as f32 - c0[0] as f32) * u).round() as u8,
        ((c0[1] as f32) + (c1[1] as f32 - c0[1] as f32) * u).round() as u8,
        ((c0[2] as f32) + (c1[2] as f32 - c0[2] as f32) * u).round() as u8,
    ]
}
