use ark_ff_optimized::fp31::Fp;
use ark_ff::{Field, PrimeField};

const P_U32: u32 = 2_147_483_647; // same modulus in u32
const SCALE_U32: u32 = 1000;

// ---------- Public entry: keeps the original f32 signature ----------
pub fn accumulate_phase_pv(
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

    // 1) Scale-and-quantize: f32 -> Fp (fixed-point with SCALE=1000)
    let to_fp = |x: f32| -> Fp {
        if x >= 0.0 {
            // Truncate toward zero for non-negatives (matches previous behavior)
            let u = (x * SCALE_U32 as f32) as u32;
            Fp::from(u as u64)
        } else {
            // Floor for negatives, then wrap to canonical [0, p)
            let q = (x * SCALE_U32 as f32).floor() as i64;
            let u = (q.rem_euclid(P_U32 as i64)) as u32;
            Fp::from(u as u64)
        }
    };

    let phase_f: Vec<Fp>       = phase.iter().map(|&v| to_fp(v)).collect();
    let prev_phase_f: Vec<Fp>  = prev_phase.iter().map(|&v| to_fp(v)).collect();
    let acc_in_f: Vec<Fp>      = phase_acc_in.iter().map(|&v| to_fp(v)).collect();
    let omega_f: Vec<Fp>       = omega.iter().map(|&v| to_fp(v)).collect();

    // 2) Core over the finite field; returns Fp elements
    let acc_out_f = accumulate_phase_pv_core_fp(
        &phase_f,
        &prev_phase_f,
        &acc_in_f,
        &omega_f,
        Fp::from(ha as u64),
        Fp::from(hs as u64),
        Fp::from(SCALE_U32 as u64),
    );

    // 3) Convert field residues to signed real numbers, then scale back to f32
    //    Interpret residues in symmetrical range [-p/2, p/2] to allow negatives.
    acc_out_f
        .into_iter()
        .map(|z| {
            let limb = z.into_bigint().0[0] as u32; // canonical [0,p)
            let half = P_U32 / 2;
            let signed: i64 = if limb <= half { limb as i64 } else { limb as i64 - P_U32 as i64 };
            signed as f32 / SCALE_U32 as f32
        })
        .collect()
}

// ---------- Core over the field (all numbers live in Fp31) ----------
// Inputs/outputs are Fp elements to make the boundary pure-field.
fn accumulate_phase_pv_core_fp(
    phase_f: &[Fp],
    prev_phase_f: &[Fp],
    acc_in_f: &[Fp],
    omega_f: &[Fp],
    ha_f: Fp,
    hs_f: Fp,
    scale_f: Fp,
) -> Vec<Fp> {
    assert_eq!(phase_f.len(), omega_f.len());
    assert_eq!(phase_f.len(), prev_phase_f.len());
    assert_eq!(phase_f.len(), acc_in_f.len());

    // Constants in field
    let inv_scale = scale_f.inverse().expect("scale and p are coprime");
    let inv_ha = ha_f.inverse().expect("ha must be non-zero modulo p");

    // Compute out_f in field, fixed-point semantics:
    //
    // Original reals:
    //   dphi       = phase - prev_phase - omega * ha_f
    //   true_freq  = omega + dphi / ha_f
    //   acc_out    = acc_in + true_freq * hs_f
    //
    // Fixed-point with scale=S:
    //   All arrays are stored as value*S in field.
    //   omega*ha_f   -> (omega*S) * ha / S = omega*S*ha * inv(S)
    //   dphi/ha_f    -> (dphi*S) / ha = dphi * inv(ha)
    //   *hs_f        -> (true_freq*S) * hs / S = true_freq * hs * inv(S)
    //
    // NOTE: No explicit wrap-to-[-π,π] here; we operate modulo p only.
    //       If you need 2π wrap, add a secondary reduction outside the field.
    let mut out_f = Vec::with_capacity(phase_f.len());
    for k in 0..phase_f.len() {
        let omega_ha = omega_f[k] * ha_f * inv_scale;     // (omega*ha)/S
        let dphi = phase_f[k] - prev_phase_f[k] - omega_ha; // mod p

        // "true_freq" in fixed-point
        let true_freq = omega_f[k] + dphi * inv_ha;       // still scaled by S

        // acc_out = acc_in + (true_freq * hs)/S  (fixed-point)
        let acc = acc_in_f[k] + true_freq * hs_f * inv_scale;
        out_f.push(acc);
    }

    // Return field elements
    out_f
}
