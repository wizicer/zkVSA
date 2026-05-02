/// Accumulate synthesis phases for PV-TSM in the polar domain.
///
/// This pure function computes the updated synthesis phase accumulator `phase_acc`
/// based on the current analysis `phase` and the previous analysis phase `prev_phase_in`.
/// If `prev_phase_in` is `None`, it treats the frame as the first one and initializes the
/// accumulator to the current `phase`.
///
/// The caller is responsible for updating `prev_phase` externally (e.g., `prev_phase.copy_from_slice(phase)`).
/// No Cartesian conversion is done here; caller can reconstruct Complex spectra from the
/// returned `phase_acc` and magnitudes afterwards.
pub(crate) fn accumulate_phase_pv(
    phase: &[f32],
    prev_phase: &[f32],
    phase_acc_in: &[f32],
    omega: &[f32],
    ha: usize,
    hs: usize,
) -> Vec<f32> {
    debug_assert_eq!(phase.len(), omega.len());
    debug_assert_eq!(phase.len(), prev_phase.len());
    debug_assert_eq!(phase.len(), phase_acc_in.len());

    let two_pi = 2.0 * std::f32::consts::PI;
    let ha_f = ha as f32;
    let hs_f = hs as f32;

    let mut acc_out = vec![0.0f32; phase.len()];
    for k in 0..phase.len() {
        let dphi = phase[k] - prev_phase[k] - omega[k] * ha_f;
        // Wrap to [-pi, pi]
        let dphi_wrapped = dphi - two_pi * (dphi / two_pi).round();
        let true_freq = omega[k] + dphi_wrapped / ha_f;
        acc_out[k] = phase_acc_in[k] + true_freq * hs_f;
    }
    acc_out
}

