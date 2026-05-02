use super::utils::{f32_to_i16, i16_to_f32, resample_linear, semitones_to_factor};

pub(crate) fn pitch_shift_resample(samples: &[i16], semitones: f32) -> Vec<i16> {
    if semitones.abs() < 1e-6 {
        return samples.to_vec();
    }
    let factor = semitones_to_factor(semitones);
    let x = i16_to_f32(samples);
    let n_mid = ((samples.len() as f32) / factor).round().max(1.0) as usize;
    let mid = resample_linear(&x, n_mid);
    let out = resample_linear(&mid, samples.len());
    f32_to_i16(&out)
}
