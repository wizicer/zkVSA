pub fn hann_window(n: usize) -> Vec<f32> {
    let mut w = vec![0.0f32; n];
    if n == 0 {
        return w;
    }
    for i in 0..n {
        w[i] = 0.5 - 0.5 * (2.0 * std::f32::consts::PI * (i as f32) / (n as f32)).cos();
    }
    w
}

pub fn resample_linear(input: &[f32], out_len: usize) -> Vec<f32> {
    if input.is_empty() || out_len == 0 {
        return Vec::new();
    }
    if input.len() == 1 {
        return vec![input[0]; out_len];
    }
    let in_len = input.len() as f32;
    let out_len_f = out_len as f32;
    let mut out = vec![0.0f32; out_len];
    for i in 0..out_len {
        let t = (i as f32) * ((in_len - 1.0) / (out_len_f - 1.0));
        let idx = t.floor() as usize;
        let frac = t - (idx as f32);
        let a = input[idx];
        let b = input[(idx + 1).min(input.len() - 1)];
        out[i] = a + (b - a) * frac;
    }
    out
}

pub fn i16_to_f32(input: &[i16]) -> Vec<f32> {
    input.iter().map(|&s| (s as f32) / 32768.0).collect()
}

pub fn f32_to_i16(input: &[f32]) -> Vec<i16> {
    input
        .iter()
        .map(|&x| {
            let y = x.clamp(-1.0, 1.0);
            (y * 32767.0).round() as i16
        })
        .collect()
}

pub fn semitones_to_factor(semitones: f32) -> f32 {
    (2.0f32).powf(semitones / 12.0)
}
