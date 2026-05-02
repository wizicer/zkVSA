use ark_bls12_377::Fr;
use ark_ff::{BigInteger, PrimeField, Zero};
use num_bigint::BigUint;
use std::path::Path;

/// Convert f32 phase to Fr (BLS12-377 scalar field element)
/// Implements scaled phase propagation: tilde_Phi = 2^l * Phi / π
fn f32_to_fr(x: f32, l: usize) -> Fr {
    if x.is_nan() || x.is_infinite() {
        return Fr::zero();
    }

    // Normalize by π and scale by 2^l: tilde_Phi = 2^l * Phi / π
    let scale = (1u64 << l) as f32; // 2^l
    let normalized = x / std::f32::consts::PI; // Phi / π
    let scaled = (normalized * scale).round() as i64; // 2^l * Phi / π

    if scaled >= 0 {
        Fr::from(scaled as u64)
    } else {
        // Handle negative values by using field arithmetic
        let pos_val = Fr::from((-scaled) as u64);
        -pos_val
    }
}

/// Convert Fr back to f32 phase
/// Reverses scaled phase propagation: Phi = π * tilde_Phi / 2^l
fn fr_to_f32(fr: &Fr, l: usize) -> f32 {
    // Get the canonical representation of the field element
    let bigint: BigUint = fr.into_bigint().into();

    // Check if this is a "negative" value (> p/2)
    let modulus: BigUint = Fr::MODULUS.into();
    let half_modulus = {
        let half = modulus.clone() / 2u32;
        half
    };

    let scale = (1u64 << l) as f32; // 2^l

    if bigint > half_modulus {
        // This represents a negative value
        let diff = modulus - bigint;
        let value = diff.to_u64_digits()[0];
        let normalized = -(value as f32) / scale; // -tilde_Phi / 2^l
        normalized * std::f32::consts::PI // π * (-tilde_Phi / 2^l)
    } else {
        if bigint.to_u64_digits().len() == 0 {
            0.0
        } else {
            let value = bigint.to_u64_digits()[0];
            let normalized = (value as f32) / scale; // tilde_Phi / 2^l
            normalized * std::f32::consts::PI // π * (tilde_Phi / 2^l)
        }
    }
}

/// Convert Vec<f32> to Vec<Fr>
fn f32_vec_to_fr_vec(input: &[f32], l: usize) -> Vec<Fr> {
    input.iter().map(|&x| f32_to_fr(x, l)).collect()
}

/// Convert Vec<Fr> to Vec<f32>
fn fr_vec_to_f32_vec(input: &[Fr], l: usize) -> Vec<f32> {
    input.iter().map(|fr| fr_to_f32(fr, l)).collect()
}

/// Convert Vec<Vec<f32>> to Vec<Vec<Fr>>
fn f32_matrix_to_fr_matrix(input: &[Vec<f32>], l: usize) -> Vec<Vec<Fr>> {
    input.iter().map(|row| f32_vec_to_fr_vec(row, l)).collect()
}

/// Convert Vec<Vec<Fr>> to Vec<Vec<f32>>
fn fr_matrix_to_f32_matrix(input: &[Vec<Fr>], l: usize) -> Vec<Vec<f32>> {
    input.iter().map(|row| fr_vec_to_f32_vec(row, l)).collect()
}

/// Core implementation of PvAccumulateAllMirror in Rust using Fr field arithmetic
fn pv_accumulate_all_mirror(
    phases: &[Vec<Fr>],
    omega: &[Fr],
    rs: &Fr,
    u: usize,
    l: usize,
    m: usize,
    cbor_dir: Option<&Path>,
    use_half_up: bool,
) -> Result<Vec<Vec<Fr>>, String> {
    if m == 0 {
        return Err("m must be > 0".to_string());
    }
    if u < 2 {
        return Err("u must be >= 2".to_string());
    }
    if phases.len() != u {
        return Err(format!("len(phases)={} != U={}", phases.len(), u));
    }
    if phases.is_empty() || phases[0].is_empty() {
        return Err("phases has zero columns".to_string());
    }

    let n = 1usize << l; // 2^l
    if omega.len() != n {
        return Err(format!(
            "len(omega)={} != N={} (N must be 1<<l)",
            omega.len(),
            n
        ));
    }

    for (t, phase_row) in phases.iter().enumerate() {
        if phase_row.len() != n {
            return Err(format!(
                "phases[{}] length {} != N={}",
                t,
                phase_row.len(),
                n
            ));
        }
    }

    // Precompute constants as Fr elements
    let pow_m = 1u64 << m; // 2^m
    let min_dphi = 1u64 << (l + m + 2); // 2^(l+m+2)
    let l_m_big = 1u64 << (l - m); // 2^(l-m)

    // Allocate output
    let mut out = vec![vec![Fr::zero(); n]; u];

    // Frame 0: pass-through
    for k in 0..n {
        out[0][k] = phases[0][k];
    }

    // Main accumulation loop (frames 1..U-1)
    for u_idx in 1..u {
        for k in 0..n {
            // t1 = omega[k] * 2^m
            let t1 = omega[k] * Fr::from(pow_m);

            // dphi = phases[u,k] - phases[u-1,k] - t1 (all in field)
            let dphi = phases[u_idx][k] - phases[u_idx - 1][k] - t1;

            // Unwrap to [0, 2^(l+1)): dphi_u = (dphi + 2^(l+m+2)) mod 2^(l+1)
            let dphi_u = fr_mod(dphi + Fr::from(min_dphi), l + 1);

            let t2 = if use_half_up {
                // Rounding by 2^m (half-up): t2 = floor((dphi_u + 2^(m-1)) / 2^m)
                half_up_div_pow2(&dphi_u, m)
            } else {
                // Rounding by 2^m (floor): t2 = floor(dphi_u / 2^m)
                floor_div_pow2(&dphi_u, m)
            };

            // trueFreq = omega[k] + (t2 - 2^l)
            let true_freq = omega[k] + t2 - Fr::from(l_m_big);

            // step = trueFreq * rs
            let step = true_freq * rs;

            // out[u,k] = out[u-1,k] + step (field addition)
            out[u_idx][k] = out[u_idx - 1][k] + step;
        }
    }

    // Generate CBOR files if directory is provided
    if let Some(cbor_dir) = cbor_dir {
        // Capture input data for CBOR generation
        let input_values = fr_matrix_to_i64_vec(phases);
        let rs_u64 = fr_to_u64(rs);

        // Create input CBOR data
        let input_file = crate::cbor::InputFile::new(
            u as u64,
            l as u64,
            m as u64,
            phases[0].len() as u64, // s = number of frequency bins
            rs_u64,
            input_values,
        )
        .map_err(|e| format!("Failed to create input CBOR: {}", e))?;

        // Create output CBOR data
        let output_values = fr_matrix_to_i64_vec(&out);
        let output_file = crate::cbor::OutputFile::new(output_values);

        // Write CBOR files
        std::fs::create_dir_all(cbor_dir)
            .map_err(|e| format!("Failed to create CBOR directory: {}", e))?;

        let input_path = cbor_dir.join("input.cbor");
        input_file
            .write_file(&input_path)
            .map_err(|e| format!("Failed to write input CBOR: {}", e))?;

        let output_path = cbor_dir.join("output.cbor");
        output_file
            .write_file(&output_path)
            .map_err(|e| format!("Failed to write output CBOR: {}", e))?;
    }

    Ok(out)
}

/// Compute (a mod m) where a, m are Fr elements.
/// Semantics: treat `a` and `m` as their canonical integers in [0, r),
/// compute integer remainder `a % m`, then map back to Fr.
///
/// Panics if m == 0 (as an Fr element).
pub fn fr_mod(a: Fr, m: usize) -> Fr {
    if m == 0 {
        panic!("fr_mod: modulus is zero");
    }
    // 1) Fr -> canonical big integer (bytes, little-endian)
    let a_le = a.into_bigint().to_bytes_le();

    // 2) bytes -> BigUint, do integer `%`
    let a_big = BigUint::from_bytes_le(&a_le);
    let m_big = BigUint::from((1 << m) as u64);
    let r_big = a_big % m_big; // integer remainder, 0 <= r < m < r(field)

    // 3) remainder back to Fr (canonical mod-order mapping)
    Fr::from_le_bytes_mod_order(&r_big.to_bytes_le())
}

/// Convert Fr to u64 for CBOR serialization
fn fr_to_u64(fr: &Fr) -> u64 {
    let bigint: BigUint = fr.into_bigint().into();
    if bigint.to_u64_digits().is_empty() {
        0
    } else {
        bigint.to_u64_digits()[0]
    }
}

/// Convert Fr to i64 for CBOR serialization
fn fr_to_i64(fr: &Fr) -> i64 {
    // Get the canonical representation of the field element
    let bigint: BigUint = fr.into_bigint().into();

    // Check if this is a "negative" value (> p/2)
    let modulus: BigUint = Fr::MODULUS.into();
    let half_modulus = {
        let half = modulus.clone() / 2u32;
        half
    };

    if bigint > half_modulus {
        // This represents a negative value
        let diff = modulus - bigint;
        -(diff.to_u32_digits()[0] as i64)
    } else {
        if bigint.to_u64_digits().len() == 0 {
            0
        } else {
            bigint.to_u64_digits()[0] as i64
        }
    }
}

/// Convert Vec<Vec<Fr>> to flattened Vec<u64> for CBOR
fn fr_matrix_to_i64_vec(matrix: &[Vec<Fr>]) -> Vec<i64> {
    matrix
        .iter()
        .flat_map(|row| row.iter().map(fr_to_i64))
        .collect()
}

/// Helper function for half-up division by power of 2
/// Equivalent to floor((z + 2^(m-1)) / 2^m)
#[allow(dead_code)]
fn half_up_div_pow2(z: &Fr, m: usize) -> Fr {
    let z_big: BigUint = z.into_bigint().into();
    let m_big = BigUint::from((1 << m) as u64);
    let half_m_big = m_big.clone() / 2u32;
    let z_prime = z_big + half_m_big;
    let z_big = z_prime / m_big;
    Fr::from(z_big)
}

/// Helper function for floor division by power of 2
/// Equivalent to floor(z / 2^m)
#[allow(dead_code)]
fn floor_div_pow2(z: &Fr, m: usize) -> Fr {
    let z_big: BigUint = z.into_bigint().into();
    let m_big = BigUint::from((1 << m) as u64);
    let z_big = z_big / m_big;
    Fr::from(z_big)
}

/// Main entry point with optional CBOR output
pub fn pv_accumulate_all(
    phases: &[Vec<f32>],
    win_size: usize,
    ha: usize,
    hs: usize,
    cbor_dir: Option<&Path>,
    use_half_up: bool,
) -> Vec<Vec<f32>> {
    if phases.is_empty() {
        return Vec::new();
    }

    // Calculate parameters
    let u = phases.len(); // Number of frames
    let n = win_size;

    // Calculate l such that 2^l = N (win_size)
    let l = (n as f32).log2() as usize;
    if (1usize << l) != n {
        panic!("l is not power of 2");
    }
    let m = (ha as f32).log2() as usize;
    if (1usize << m) != ha {
        panic!("m is not power of 2");
    }

    // Convert input f32 phases to Fr using the calculated l
    let fr_phases = f32_matrix_to_fr_matrix(phases, l);

    // Create omega array (frequency bins)
    let omega: Vec<Fr> = (0..n).map(|k| Fr::from((k as u64) * 2)).collect();

    // Calculate rs (synthesis hop ratio)
    let rs = f32_to_fr(hs as f32 / ha as f32, l);

    // Call the core function
    match pv_accumulate_all_mirror(&fr_phases, &omega, &rs, u, l, m, cbor_dir, use_half_up) {
        Ok(result_fr) => fr_matrix_to_f32_matrix(&result_fr, l),
        Err(e) => {
            eprintln!("Failed to accumulate phases: {}", e);
            panic!("Failed to accumulate phases");
        }
    }
}
