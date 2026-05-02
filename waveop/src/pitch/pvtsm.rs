use std::iter;

use rustfft::{num_complex::Complex, FftPlanner};
use super::utils::{f32_to_i16, i16_to_f32, resample_linear, semitones_to_factor, hann_window};

mod m32;
mod f32;
pub mod f377;

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum PvAccum {
    M32,
    F32,
    F377,
}

/// Pass 1: Analyze all frames using FFT and convert to polar (magnitude, phase).
fn pv_analyze_all(
    input: &[f32],
    win: &[f32],
    win_size: usize,
    frames: usize,
    ha: usize,
) -> (Vec<Vec<f32>>, Vec<Vec<f32>>) {
    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(win_size);
    let mut magnitudes: Vec<Vec<f32>> = vec![vec![0.0f32; win_size]; frames];
    let mut phases: Vec<Vec<f32>> = vec![vec![0.0f32; win_size]; frames];
    for f in 0..frames {
        let in_pos = f * ha;
        let mut spec: Vec<Complex<f32>> = (0..win_size)
            .map(|i| Complex { re: input[in_pos + i] * win[i], im: 0.0 })
            .collect();
        fft.process(&mut spec);
        for k in 0..win_size {
            let re = spec[k].re;
            let im = spec[k].im;
            magnitudes[f][k] = (re * re + im * im).sqrt();
            phases[f][k] = im.atan2(re);
        }
    }
    (magnitudes, phases)
}

/// Pass 2: Accumulate phases across frames in the polar domain with optional CBOR output.
fn pv_accumulate_all(
    phases: &[Vec<f32>],
    win_size: usize,
    ha: usize,
    hs: usize,
    impl_kind: PvAccum,
    cbor_dir: Option<&std::path::Path>,
    use_half_up: bool,
) -> Vec<Vec<f32>> {
    if phases.is_empty() {
        return Vec::new();
    }

    if impl_kind == PvAccum::F377 {
        return f377::pv_accumulate_all(phases, win_size, ha, hs, cbor_dir, use_half_up);
    }

    let two_pi = 2.0 * std::f32::consts::PI;
    let omega: Vec<f32> = (0..win_size)
        .map(|k| two_pi * (k as f32) / (win_size as f32))
        .collect();
    let first = phases[0].clone();

    let tail = phases
        .windows(2)
        .scan(first.clone(), |phase_acc, win| {
            let new_acc = match impl_kind {
                PvAccum::M32 => m32::accumulate_phase_pv(&win[1], &win[0], phase_acc, &omega, ha, hs),
                PvAccum::F32 => self::f32::accumulate_phase_pv(&win[1], &win[0], phase_acc, &omega, ha, hs),
                PvAccum::F377 => panic!("should not here")
            };
            *phase_acc = new_acc.clone();
            Some(new_acc)
        });

    iter::once(first).chain(tail).collect()
}

/// Pass 3: Resynthesize all frames via IFFT and overlap-add.
fn pv_synthesize_all(
    magnitudes: &[Vec<f32>],
    syn_phases: &[Vec<f32>],
    win: &[f32],
    win_size: usize,
    hs: usize,
    frames: usize,
) -> Vec<f32> {
    let out_len = frames * hs + win_size;
    assert_eq!(magnitudes.len(), syn_phases.len());
    let frames = magnitudes.len();
    let mut planner = FftPlanner::<f32>::new();
    let ifft = planner.plan_fft_inverse(win_size);
    let mut out = vec![0.0f32; out_len];
    let mut norm = vec![0.0f32; out_len];
    for f in 0..frames {
        let out_pos = f * hs;
        let mut syn_spec = Vec::with_capacity(win_size);
        for k in 0..win_size {
            syn_spec.push(Complex {
                re: magnitudes[f][k] * syn_phases[f][k].cos(),
                im: magnitudes[f][k] * syn_phases[f][k].sin(),
            });
        }
        let mut time_frame = syn_spec;
        ifft.process(&mut time_frame);
        // rustfft inverse is unnormalized
        let scale = 1.0 / (win_size as f32);
        for i in 0..win_size {
            let v = time_frame[i].re * scale;
            let w = win[i];
            let o = out_pos + i;
            if o < out_len {
                out[o] += v * w;
                norm[o] += w * w;
            }
        }
    }
    for i in 0..out_len {
        if norm[i] > 1e-6 {
            out[i] /= norm[i];
        }
    }
    out
}

fn time_stretch_pv(input: &[f32], stretch: f32, win_size: usize, hop_a: usize, impl_kind: PvAccum, cbor_dir: Option<&std::path::Path>, use_half_up: bool) -> Vec<f32> {
    assert!(stretch > 0.0);
    if input.is_empty() || win_size == 0 {
        return Vec::new();
    }
    let ha = hop_a.max(1);
    let hs = ((ha as f32) * stretch).round().max(1.0) as usize;

    let win = hann_window(win_size);

    // Estimate number of frames and output length
    if input.len() < win_size {
        return input.to_vec();
    }
    let frames = 1 + (input.len() - win_size) / ha;
    // Pass 1: analysis FFT and polar conversion for all frames
    let (magnitudes, phases) = pv_analyze_all(input, &win, win_size, frames, ha);
    // Pass 2: accumulate phases across frames (polar domain)
    let syn_phases = pv_accumulate_all(&phases, win_size, ha, hs, impl_kind, cbor_dir, use_half_up);
    // Pass 3: resynthesis IFFT and overlap-add
    pv_synthesize_all(&magnitudes, &syn_phases, &win, win_size, hs, frames)
}

pub(crate) fn pitch_shift_pv_tsm(samples: &[i16], semitones: f32, impl_kind: PvAccum, cbor_dir: Option<&std::path::Path>, use_half_up: bool) -> Vec<i16> {
    if samples.is_empty() || semitones.abs() < 1e-6 {
        return samples.to_vec();
    }
    let x = i16_to_f32(samples);
    let factor = semitones_to_factor(semitones);
    let stretch = 1.0 / factor;

    let win_size = 512usize; // ~64ms at 8kHz
    let hop_a = win_size / 4; // 75% overlap

    let stretched = time_stretch_pv(&x, stretch, win_size, hop_a, impl_kind, cbor_dir, use_half_up);
    let resampled = resample_linear(&stretched, samples.len());
    f32_to_i16(&resampled)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gen_sine(len: usize, freq_hz: f32, sr: f32) -> Vec<f32> {
        let two_pi = 2.0 * std::f32::consts::PI;
        (0..len)
            .map(|n| (two_pi * freq_hz * (n as f32) / sr).sin())
            .collect()
    }

    fn gen_noise_i16(len: usize, seed: u32) -> Vec<i16> {
        let mut s = seed;
        let mut out = Vec::with_capacity(len);
        for _ in 0..len {
            // LCG
            s = s.wrapping_mul(1664525).wrapping_add(1013904223);
            let v = ((s >> 16) & 0xFFFF) as i32;
            let centered = v - 32768; // [-32768, 32767]
            out.push(centered as i16);
        }
        out
    }

    #[test]
    fn time_stretch_pv_length_formula() {
        let sr = 8000.0;
        let n = 8000usize; // 1s
        let x = gen_sine(n, 220.0, sr);
        let win_size = 512usize;
        let ha = 128usize;
        let stretch = 1.5f32;
        let hs = ((ha as f32) * stretch).round().max(1.0) as usize;
        let frames = 1 + (n - win_size) / ha;
        let expected_len = frames * hs + win_size;
        let y = time_stretch_pv(&x, stretch, win_size, ha, PvAccum::M32, None, true);
        assert_eq!(y.len(), expected_len, "PV-TSM output length mismatch");
    }

    #[test]
    fn pv_tsm_zero_semitones_identity() {
        let input = gen_noise_i16(2048, 12345);
        let out = pitch_shift_pv_tsm(&input, 0.0, PvAccum::F32, None, true);
        assert_eq!(out, input, "Zero-semitone shift must be identity");
    }

    #[test]
    fn pv_tsm_deterministic_and_nontrivial() {
        let input = gen_noise_i16(4096, 42);
        let semitones = 4.0;
        let out1 = pitch_shift_pv_tsm(&input, semitones, PvAccum::F32, None, true);
        let out2 = pitch_shift_pv_tsm(&input, semitones, PvAccum::F32, None, true);
        assert_eq!(out1, out2, "PV-TSM must be deterministic for same input");

        // Basic invariants: not identical to input, not all zeros
        assert_ne!(out1, input, "Shifted output should differ from input");
        let all_zero = out1.iter().all(|&v| v == 0);
        assert!(!all_zero, "Shifted output should not be all zeros");

        // No NaN can exist in i16, but ensure intermediate path doesn't produce extreme clipping
        let max_abs = out1.iter().map(|&v| v.abs() as i32).max().unwrap_or(0);
        assert!(max_abs > 0, "Output must have some energy");
    }

    fn fnv1a64_i16_le(data: &[i16]) -> u64 {
        let mut h: u64 = 0xcbf29ce484222325;
        for &s in data {
            let b0 = (s as u16 & 0x00FF) as u8;
            let b1 = ((s as u16 >> 8) & 0x00FF) as u8;
            for &b in &[b0, b1] {
                h ^= b as u64;
                h = h.wrapping_mul(0x100000001b3);
            }
        }
        h
    }

    #[test]
    fn pv_tsm_golden_checksum_noise42_plus4() {
        let input = gen_noise_i16(4096, 42);
        let out = pitch_shift_pv_tsm(&input, 4.0, PvAccum::F32, None, true);
        let checksum = fnv1a64_i16_le(&out);
        let expected = 3382771720382167256;

        assert_eq!(checksum, expected, "Golden checksum mismatch: got {:#x}", checksum);
    }
}
