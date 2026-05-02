package main

import (
	"crypto/rand"
	"fmt"
	"math/big"

	cborio "gkr_test/pkg/cbor"

	fr377 "github.com/consensys/gnark-crypto/ecc/bls12-377/fr"
	mimc377 "github.com/consensys/gnark-crypto/ecc/bls12-377/fr/mimc"
	poseidon2Bls12377 "github.com/consensys/gnark-crypto/ecc/bls12-377/fr/poseidon2"
	eddsa377 "github.com/consensys/gnark-crypto/ecc/bls12-377/twistededwards/eddsa"
	// "math/rand"
)

// PvAccumulateAllMirrorNew generates the witness outputs consistent with the updated circuit.
//
// Inputs:
//   - phases: T x N matrix of phase values as field elements (row-major: frame t then bin k).
//     We interpret each element as a non-negative integer in Z (its canonical repr mod p).
//   - omega : length-N array of per-bin angular frequencies as big.Int (Z, not field elements).
//   - rs    : step factor, matching the circuit's `rs` variable.
//   - U     : number of frames (should equal len(phases)).
//   - l, m  : integers with m > 0, N = 2^l.
//
// Output:
//   - out   : T x N matrix of accumulated phases as field elements, following:
//     out[0,k] = phases[0,k]
//     dphi     = phases[t,k] - phases[t-1,k] - omega[k]*2^m
//     // unwrap to [0, 2^{l+1})
//     dphi_u   = (dphi + 2^{l+m+2}) mod 2^{l+1}
//     // rounding by 2^m (half-up)
//     t2       = floor( (dphi_u + 2^{m-1}) / 2^m )
//     trueFreq = omega[k] + (t2 - 2^l)
//     step     = trueFreq * rs
//     out[t,k] = out[t-1,k] + step
//
// Notes:
//   - All integer math for unwrap/rounding is done in Z (big.Int).
//   - Conversion between fr.Element <-> big.Int uses canonical representatives.
//   - This witness code must mirror the circuit exactly to avoid constraint failures.
func PvAccumulateAllMirror(
	phases [][]fr377.Element,
	omega []*big.Int,
	rs fr377.Element,
	U, l, m int,
	isHalfUp bool,
) ([][]fr377.Element, error) {

	if m <= 0 {
		return nil, fmt.Errorf("m must be > 0")
	}
	if U < 2 {
		return nil, fmt.Errorf("U must be >= 2")
	}
	if len(phases) != U {
		return nil, fmt.Errorf("len(phases)=%d != U=%d", len(phases), U)
	}
	if len(phases[0]) == 0 {
		return nil, fmt.Errorf("phases has zero columns")
	}

	N := 1 << uint(l)
	if len(omega) != N {
		return nil, fmt.Errorf("len(omega)=%d != N=%d (N must be 1<<l)", len(omega), N)
	}
	for t := range phases {
		if len(phases[t]) != N {
			return nil, fmt.Errorf("phases[%d] length %d != N=%d", t, len(phases[t]), N)
		}
	}

	// Precompute integer constants.
	powM := new(big.Int).Lsh(big.NewInt(1), uint(m))        // 2^m
	powL1 := new(big.Int).Lsh(big.NewInt(1), uint(l+1))     // 2^(l+1)
	minDphi := new(big.Int).Lsh(big.NewInt(1), uint(l+m+2)) // 2^(l+m+2)
	lmBig := new(big.Int).Lsh(big.NewInt(1), uint(l-m))     // 2^(l-m)

	// Allocate output.
	out := make([][]fr377.Element, U)
	for u := 0; u < U; u++ {
		out[u] = make([]fr377.Element, N)
	}

	// Frame 0: pass-through
	for k := 0; k < N; k++ {
		out[0][k] = phases[0][k]
	}

	// Helper to read an fr element as a big.Int (canonical representative mod p).
	elemToBig := func(e *fr377.Element) *big.Int {
		var z big.Int
		e.BigInt(&z) // gnark-crypto returns canonical representative in [0, p)
		return &z
	}

	// Main accumulation loop (frames 1..U-1).
	for u := 1; u < U; u++ {
		for k := 0; k < N; k++ {
			// t1 = omega[k] * 2^m
			t1 := new(big.Int).Mul(omega[k], powM)

			// dphi = phases[t,k] - phases[t-1,k] - t1      (all in Z)
			ptk := elemToBig(&phases[u][k])
			ptkm := elemToBig(&phases[u-1][k])
			dphi := new(big.Int).Sub(ptk, ptkm)
			dphi.Sub(dphi, t1)

			// Unwrap to [0, 2^(l+1)):
			// dphi_u = (dphi + 2^(l+m+2)) mod 2^(l+1)
			tt := new(big.Int).Add(dphi, minDphi)
			tt.Mod(tt, fr377.Modulus())
			dphiU := new(big.Int).Mod(tt, powL1) // 0 <= dphiU < 2^(l+1)

			// Rounding by 2^m (half-up):
			// t2 = floor( (dphi_u + 2^(m-1)) / 2^m )
			var t2 *big.Int
			if isHalfUp {
				t2, _ = halfUpDivPow2Core(dphiU, m)
			} else {
				t2, _ = floorDivPow2Core(dphiU, m)
			}

			// trueFreq = omega[k] + (t2 - 2^(l-m))
			tf := new(big.Int).Sub(t2, lmBig)
			tf.Add(tf, omega[k])

			// step = trueFreq * rs
			t3 := new(big.Int).Mul(tf, elemToBig(&rs))

			// out[t,k] = out[t-1,k] + step    (field addition)
			var stepElt fr377.Element
			stepElt.SetBigInt(t3)
			out[u][k].Add(&out[u-1][k], &stepElt)
		}
	}

	return out, nil
}

// GKR Poseidon2 compression (consistent with circuit)
func foldPoseidon2Native(vals []fr377.Element) fr377.Element {
	params := poseidon2Bls12377.GetDefaultParameters()
	perm := poseidon2Bls12377.NewPermutation(2, params.NbFullRounds, params.NbPartialRounds)

	var acc fr377.Element // 0
	for i := 0; i < len(vals); i++ {
		x := [2]fr377.Element{acc, vals[i]}
		y0 := x[1]
		if err := perm.Permutation(x[:]); err != nil {
			panic(err)
		}
		x[1].Add(&x[1], &y0) // feed-forward
		acc = x[1]
	}
	return acc
}

func flatten2D(a [][]fr377.Element) []fr377.Element {
	if len(a) == 0 {
		return nil
	}
	N := len(a[0])
	out := make([]fr377.Element, 0, len(a)*N)
	for t := 0; t < len(a); t++ {
		out = append(out, a[t]...)
	}
	return out
}

// generateWitness creates and returns the assigned witness for the circuit
func generateWitness(U, l, m int, isHalfUp bool) (*PvAudioSignedCircuit, error) {
	N := 1 << l

	// Construct input: random phases, omega, rs
	var rs fr377.Element
	rs.SetInt64(128)

	omega := make([]*big.Int, N)
	for k := 0; k < N; k++ {
		// Example: directly set to k (in practice, can use rounding of 2πk/W * S, then mod p)
		omega[k] = big.NewInt(int64(k) * 2)
	}

	phases := make([][]fr377.Element, U)
	for u := 0; u < U; u++ {
		phases[u] = make([]fr377.Element, N)
		for k := 0; k < N; k++ {
			// phases[u][k].SetRandom() // Example: random phase (already includes *S fixed point)
			phases[u][k].SetBigInt(big.NewInt(int64(k)))
		}
	}

	// Offline accumulation
	out, err := PvAccumulateAllMirror(phases, omega, rs, U, l, m, isHalfUp)
	if err != nil {
		return nil, fmt.Errorf("PvAccumulateAllMirror failed: %v", err)
	}

	// Use shared witness assembly function
	return assembleWitness(phases, out, omega, rs, U, l, m, isHalfUp)
}

// readCBORParameters reads U, l, m parameters from CBOR input file
func readCBORParameters(inputPath string) (int, int, int, error) {
	inputData, err := cborio.ReadInputFile(inputPath)
	if err != nil {
		return 0, 0, 0, fmt.Errorf("failed to read input CBOR file: %v", err)
	}

	U := int(inputData.U)
	l := int(inputData.L)
	m := int(inputData.M)

	if m <= 0 {
		return 0, 0, 0, fmt.Errorf("m must be > 0, got %d", m)
	}
	if U < 2 {
		return 0, 0, 0, fmt.Errorf("U must be >= 2, got %d", U)
	}

	return U, l, m, nil
}

// generateWitnessFromCBOR creates witness from CBOR input file and validates against expected output
func generateWitnessFromCBOR(inputPath, outputPath string, isHalfUp bool) (*PvAudioSignedCircuit, error) {
	// Load input CBOR file
	inputData, err := cborio.ReadInputFile(inputPath)
	if err != nil {
		return nil, fmt.Errorf("failed to read input CBOR file: %v", err)
	}

	// Extract parameters from CBOR input
	U := int(inputData.U)
	l := int(inputData.L)
	m := int(inputData.M)
	_ = int(inputData.S) // s parameter not used in current implementation
	rs := int(inputData.Rs)

	if m <= 0 {
		return nil, fmt.Errorf("m must be > 0, got %d", m)
	}
	if U < 2 {
		return nil, fmt.Errorf("U must be >= 2, got %d", U)
	}

	N := 1 << l
	expectedValuesLen := U * N

	if len(inputData.Values) != expectedValuesLen {
		return nil, fmt.Errorf("input values length (%d) doesn't match expected U*N (%d)", len(inputData.Values), expectedValuesLen)
	}

	// Convert CBOR values to phases matrix
	phases := make([][]fr377.Element, U)
	for u := 0; u < U; u++ {
		phases[u] = make([]fr377.Element, N)
		for k := 0; k < N; k++ {
			idx := u*N + k
			// Convert u64 to field element using the scaling factor from memories
			scaledValue := new(big.Int).SetInt64(inputData.Values[idx])
			phases[u][k].SetBigInt(scaledValue)
		}
	}

	// Create omega array (frequency bins)
	omega := make([]*big.Int, N)
	for k := 0; k < N; k++ {
		// Use frequency bin index as base frequency
		omega[k] = big.NewInt(int64(k * 2))
	}

	// Create rs field element
	var rsElement fr377.Element
	rsElement.SetInt64(int64(rs))

	// Generate witness output using PvAccumulateAllMirror
	out, err := PvAccumulateAllMirror(phases, omega, rsElement, U, l, m, isHalfUp)
	if err != nil {
		return nil, fmt.Errorf("PvAccumulateAllMirror failed: %v", err)
	}

	// Validate against expected output if provided
	if outputPath != "" {
		expectedOutput, err := cborio.ReadOutputFile(outputPath)
		if err != nil {
			return nil, fmt.Errorf("failed to read expected output CBOR file: %v", err)
		}

		if len(expectedOutput.Values) != expectedValuesLen {
			return nil, fmt.Errorf("expected output values length (%d) doesn't match U*N (%d)", len(expectedOutput.Values), expectedValuesLen)
		}

		// Compare generated output with expected output
		for u := 0; u < U; u++ {
			for k := 0; k < N; k++ {
				idx := u*N + k
				// Convert field element to signed representation
				generatedSigned := fieldElementToSigned(&out[u][k])
				expectedSigned := big.NewInt(expectedOutput.Values[idx])

				if generatedSigned.Cmp(expectedSigned) != 0 {
					return nil, fmt.Errorf("output mismatch at [%d][%d]: generated %s, expected %s",
						u, k, generatedSigned.String(), expectedSigned.String())
				}
			}
		}
		fmt.Printf("✅ Output validation passed: generated output matches expected CBOR output\n")
	}

	// Use shared witness assembly function
	assign, err := assembleWitness(phases, out, omega, rsElement, U, l, m, isHalfUp)
	if err != nil {
		return nil, err
	}

	fmt.Printf("✅ Witness generated from CBOR input: U=%d, l=%d, m=%d, N=%d\n", U, l, m, N)
	return assign, nil
}

// fieldElementToSigned converts a field element to its signed representation
// If the element > p/2, it represents a negative number and should be converted to -(p - element)
func fieldElementToSigned(elem *fr377.Element) *big.Int {
	elemBI := new(big.Int)
	elem.BigInt(elemBI)

	// Get the field modulus p
	p := fr377.Modulus()

	// Calculate p/2
	pHalf := new(big.Int).Div(p, big.NewInt(2))

	// If elemBI > p/2, convert to negative: -(p - elemBI)
	if elemBI.Cmp(pHalf) > 0 {
		result := new(big.Int).Sub(p, elemBI)
		result.Neg(result)
		return result
	}

	return elemBI
}

// assembleWitness creates the complete witness from phases, output, and parameters
func assembleWitness(phases, out [][]fr377.Element, omega []*big.Int, rs fr377.Element, U, l, m int, isHalfUp bool) (*PvAudioSignedCircuit, error) {
	N := 1 << l

	// Generate salt for commitments
	var salt fr377.Element
	salt.SetRandom()

	// Create commitments using Poseidon2
	flatIn := append([]fr377.Element{salt}, flatten2D(phases)...)
	inRoot := foldPoseidon2Native(flatIn)

	flatOut := append([]fr377.Element{salt}, flatten2D(out)...)
	outRoot := foldPoseidon2Native(flatOut)

	// Generate EdDSA key and signature
	priv, err := eddsa377.GenerateKey(rand.Reader)
	if err != nil {
		return nil, fmt.Errorf("failed to generate EdDSA key: %v", err)
	}
	pub := priv.PublicKey

	// Sign the input root
	inRootBI := new(big.Int)
	inRoot.BigInt(inRootBI)
	msg := inRootBI.Bytes()

	sigBin, err := priv.Sign(msg, mimc377.NewMiMC())
	if err != nil {
		return nil, fmt.Errorf("failed to sign message: %v", err)
	}

	// Verify signature
	ok, err := pub.Verify(sigBin, msg, mimc377.NewMiMC())
	if err != nil {
		return nil, fmt.Errorf("signature verification error: %v", err)
	}
	if !ok {
		return nil, fmt.Errorf("signature verification failed")
	}

	var sig eddsa377.Signature
	_, err = sig.SetBytes(sigBin)
	if err != nil {
		return nil, fmt.Errorf("failed to set signature bytes: %v", err)
	}

	// Create public key hash
	var pkX, pkY fr377.Element
	pkX, pkY = pub.A.X, pub.A.Y
	pkHash := foldPoseidon2Native([]fr377.Element{pkX, pkY})

	// Assemble witness
	assign := NewPvAudioSignedCircuit(U, l, m, isHalfUp)
	for u := 0; u < U; u++ {
		for k := 0; k < N; k++ {
			assign.PhasesIn[idx(u, k, N)] = phases[u][k].BigInt(new(big.Int))
			assign.PhasesOut[idx(u, k, N)] = out[u][k].BigInt(new(big.Int))
		}
	}
	for k := 0; k < N; k++ {
		assign.Omega[k] = *omega[k]
	}

	assign.Rs = rs.BigInt(new(big.Int))
	assign.Salt = salt.BigInt(new(big.Int))

	// EdDSA witness
	assign.PubKey.A.X = pub.A.X.BigInt(new(big.Int))
	assign.PubKey.A.Y = pub.A.Y.BigInt(new(big.Int))
	assign.Sig.R.X = sig.R.X.BigInt(new(big.Int))
	assign.Sig.R.Y = sig.R.Y.BigInt(new(big.Int))
	var sBI big.Int
	sBI.SetBytes(sig.S[:])
	assign.Sig.S = sBI

	// Public inputs
	assign.InHash = inRoot.BigInt(new(big.Int))
	assign.OutHash = outRoot.BigInt(new(big.Int))
	assign.PubKeyHash = pkHash.BigInt(new(big.Int))

	return assign, nil
}
