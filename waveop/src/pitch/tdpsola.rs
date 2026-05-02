use super::utils::{f32_to_i16, i16_to_f32, semitones_to_factor, hann_window};

fn estimate_pitch_period_acf(x: &[f32], min_p: usize, max_p: usize) -> usize {
    let n = x.len();
    if n == 0 || min_p >= max_p {
        return 80;
    }
    let seg_len = n.min(40000);
    let mut best_lag = min_p;
    let mut best_val = f32::MIN;
    for lag in min_p..=max_p {
        let mut acc = 0.0f32;
        for i in lag..seg_len {
            acc += x[i] * x[i - lag];
        }
        if acc > best_val {
            best_val = acc;
            best_lag = lag;
        }
    }
    best_lag.clamp(min_p, max_p)
}

pub(crate) fn pitch_shift_td_psola(samples: &[i16], semitones: f32) -> Vec<i16> {
    if samples.is_empty() || semitones.abs() < 1e-6 {
        return samples.to_vec();
    }
    let x = i16_to_f32(samples);
    // Assuming 8kHz: F0 in ~50..400 Hz => periods ~160..20 samples
    let min_p = 20usize;
    let max_p = 160usize;
    let p = estimate_pitch_period_acf(&x, min_p, max_p);

    let factor = semitones_to_factor(semitones);
    let p_prime = ((p as f32) / factor).round().max(1.0) as usize;

    // Window roughly two periods for good overlap
    let win_len = (2 * p).max(64);
    let win = hann_window(win_len);
    let half = win_len / 2;

    let n = x.len();
    let mut out = vec![0.0f32; n];
    let mut norm = vec![0.0f32; n];

    // Synthesis epochs spaced by p', analysis epochs nearest multiples of p
    let mut s_epoch: isize = p as isize; // start one period in
    while (s_epoch as usize) < n {
        let s = s_epoch as usize;
        if s >= n { break; }
        // Nearest analysis epoch index (multiple of p)
        let k = ((s_epoch as f32) / (p as f32)).round() as isize;
        let a_epoch = (k * (p as isize)).clamp(0, (n as isize) - 1) as usize;

        let start_out = s.saturating_sub(half);
        let start_in = a_epoch.saturating_sub(half);

        for i in 0..win_len {
            let o_idx = start_out + i;
            let in_idx = start_in + i;
            if o_idx >= n || in_idx >= n { break; }
            let wv = win[i];
            out[o_idx] += x[in_idx] * wv;
            norm[o_idx] += wv;
        }

        s_epoch += p_prime as isize;
        if p_prime == 0 { break; }
        if s_epoch as usize >= n + half { break; }
    }

    for i in 0..n {
        if norm[i] > 1e-6 {
            out[i] /= norm[i];
        }
    }
    f32_to_i16(&out)
}
