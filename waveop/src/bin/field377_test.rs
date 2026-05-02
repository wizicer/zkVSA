use ark_bls12_377::Fr;
use ark_ff::{Field, PrimeField, Zero};
use core::str::FromStr; 

fn show(label: &str, x: Fr) {
    // Print canonical big-integer (decimal) representation of the field element
    println!("{:<10} = {}", label, x.into_bigint());
}

fn main() {
    // --- Basic constants ---
    let zero = Fr::ZERO;
    let one  = Fr::ONE;

    // Check field identity laws
    assert!(zero.is_zero());
    assert_eq!(one + one - one, one);

    // --- Construct some elements ---
    // from small integers:
    let a = Fr::from(123456u64);
    let b = Fr::from(789012u64);

    // from a (larger) decimal string:
    let c = Fr::from_str(
        "1234567890123456789012345678901234567890"
    ).expect("valid Fr element");

    // modulus sanity: (r - 1) + 1 == 0 (mod r)
    // r is the prime 8444461749...9239041 for BLS12-377 Fr
    let r_minus_1 = Fr::from_str(
        "8444461749428370424248824938781546531375899335154063827935233455917409239040"
    ).unwrap();
    assert_eq!(r_minus_1 + one, zero);

    // --- Arithmetic ---
    let add  = a + b;              // addition
    let sub  = b - a;              // subtraction
    let mul  = a * b;              // multiplication
    let neg  = -a;                 // additive inverse

    // division = multiply by inverse (explicit)
    let inv_b = b.inverse().expect("b != 0 has inverse");
    let div   = a * inv_b;         // a / b

    // quick round-trip: (a / b) * b == a
    assert_eq!(div * b, a);

    // --- Batch some outputs ---
    println!("-- Field arithmetic over BLS12-377 Fr --");
    show("a", a);
    show("b", b);
    show("c", c);
    show("a+b", add);
    show("b-a", sub);
    show("a*b", mul);
    show("-a",  neg);
    show("a/b", div);

    // A couple more: powers and linear combos
    let a_sq   = a.square();       // a^2
    let a_pow5 = a.pow([5]);       // a^5
    let lin    = a + c.double() - b; // a + 2c - b

    show("a^2",   a_sq);
    show("a^5",   a_pow5);
    show("a+2c-b", lin);

    // Confirm tiny identities
    assert_eq!(a_sq, a * a);
    assert_eq!(a_pow5, a * a * a * a * a);

    println!("OK.");
}
