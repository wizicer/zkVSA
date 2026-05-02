use super::utils::{f32_to_i16, i16_to_f32, resample_linear, semitones_to_factor, hann_window};

fn time_stretch_ola(input: &[f32], stretch: f32, win_size: usize, hop_out: usize) -> Vec<f32> {
    assert!(stretch > 0.0);
    let hop_out = hop_out.max(1);
    let hop_in = ((hop_out as f32) / stretch).round().max(1.0) as usize;

    let win = hann_window(win_size);
    let mut out_len = ((input.len() as f32) * stretch).ceil() as usize + win_size + hop_out;
    out_len = out_len.max(win_size);

    let mut out = vec![0.0f32; out_len];
    let mut norm = vec![0.0f32; out_len];

    let mut in_pos = 0usize;
    let mut out_pos = 0usize;

    while in_pos + win_size <= input.len() {
        for i in 0..win_size {
            let v = input[in_pos + i] * win[i];
            if out_pos + i < out.len() {
                out[out_pos + i] += v;
                norm[out_pos + i] += win[i];
            }
        }
        in_pos = in_pos.saturating_add(hop_in);
        out_pos = out_pos.saturating_add(hop_out);
        if out_pos + win_size >= out.len() {
            break;
        }
    }

    for i in 0..out.len() {
        if norm[i] > 1e-6 {
            out[i] /= norm[i];
        }
    }

    out
}

pub(crate) fn pitch_shift_ola(samples: &[i16], semitones: f32) -> Vec<i16> {
    if semitones.abs() < 1e-6 {
        return samples.to_vec();
    }
    let factor = semitones_to_factor(semitones);
    let stretch = 1.0 / factor;
    let x = i16_to_f32(samples);

    let win_size = 256usize; // ~32ms at 8kHz
    let hop_out = win_size / 2; // 50% overlap

    let stretched = time_stretch_ola(&x, stretch, win_size, hop_out);

    let resampled = resample_linear(&stretched, samples.len());

    f32_to_i16(&resampled)
}
