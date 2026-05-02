package main

import (
	"fmt"
	"math/big"

	"github.com/consensys/gnark/frontend"
)

// Core: z >= 0, m > 0
func halfUpDivPow2Core(z *big.Int, m int) (q, r *big.Int) {
	// pow = 2^m, half = 2^(m-1)
	pow := new(big.Int).Lsh(big.NewInt(1), uint(m))
	half := new(big.Int).Rsh(new(big.Int).Set(pow), 1)

	// z' = z + 2^(m-1)
	zp := new(big.Int).Add(z, half)

	// q = floor(z'/2^m), r = z' - q*2^m, with 0 <= r < 2^m
	q = new(big.Int).Quo(zp, pow)
	r = new(big.Int).Sub(zp, new(big.Int).Mul(q, pow))
	return
}

func floorDivPow2Core(z *big.Int, m int) (q, r *big.Int) {
	// pow = 2^m
	pow := new(big.Int).Lsh(big.NewInt(1), uint(m))

	// q = floor(z/2^m), r = z - q*2^m, with 0 <= r < 2^m
	q = new(big.Int).Quo(z, pow)
	r = new(big.Int).Sub(z, new(big.Int).Mul(q, pow))
	return
}

// HalfUpDivPow2Hint
// Input : inputs[0] = z (as field element, interpreted as non-negative integer), inputs[1] = m (>0) as field element
// Output: results[0] = q, results[1] = r, where
//
//	z' = z + 2^(m-1), q = floor(z'/2^m), r = z' - q*2^m, and 0 <= r < 2^m.
//
// Notes : This implements "round-to-nearest" for non-negative z (ties go up).
func HalfUpDivPow2Hint(mod *big.Int, inputs []*big.Int, results []*big.Int) error {
	if len(inputs) < 2 {
		return fmt.Errorf("HalfUpDivPow2Hint: need inputs z and m")
	}
	z := new(big.Int).Set(inputs[0]) // interpret in [0, p-1] as a non-negative integer
	m := new(big.Int).Set(inputs[1])

	if m.Sign() <= 0 || !m.IsInt64() {
		return fmt.Errorf("HalfUpDivPow2Hint: m must be a small positive integer")
	}
	mi := int(m.Int64())

	q, r := halfUpDivPow2Core(z, mi)

	// reduce to field (harmless since q,r are already non-negative integers)
	q.Mod(q, mod)
	r.Mod(r, mod)

	results[0].Set(q)
	results[1].Set(r)
	return nil
}

func FloorDivPow2Hint(mod *big.Int, inputs []*big.Int, results []*big.Int) error {
	if len(inputs) < 2 {
		return fmt.Errorf("FloorDivPow2Hint: need inputs z and m")
	}
	z := new(big.Int).Set(inputs[0]) // interpret in [0, p-1] as a non-negative integer
	m := new(big.Int).Set(inputs[1])

	if m.Sign() <= 0 || !m.IsInt64() {
		return fmt.Errorf("FloorDivPow2Hint: m must be a small positive integer")
	}
	mi := int(m.Int64())

	q, r := floorDivPow2Core(z, mi)

	// reduce to field (harmless since q,r are already non-negative integers)
	q.Mod(q, mod)
	r.Mod(r, mod)

	results[0].Set(q)
	results[1].Set(r)
	return nil
}

// EnforceHalfUpDivPow2WithHint
// Given z (non-negative integer in the field) and m (>0 as Go int),
// it calls the hint to get (q, r) and enforces:
//  1. z + 2^(m-1) = q * 2^m + r
//  2. 0 <= r < 2^m  (by an m-bit binary decomposition)
//
// Returns (q, r).
func EnforceHalfUpDivPow2WithHint(api frontend.API, z frontend.Variable, m int) (frontend.Variable, frontend.Variable) {
	if m <= 0 {
		panic("m must be > 0")
	}
	pow := new(big.Int).Lsh(big.NewInt(1), uint(m))    // 2^m
	half := new(big.Int).Rsh(new(big.Int).Set(pow), 1) // 2^(m-1)

	// 1) call hint: inputs = (z, m), outputs = (q, r)
	out, err := api.NewHint(HalfUpDivPow2Hint, 2, z, big.NewInt(int64(m)))
	if err != nil {
		panic(err)
	}
	q := out[0]
	r := out[1]

	// 2) main equation: z + 2^(m-1) = q*2^m + r
	lhs := api.Add(z, half)
	rhs := api.Add(api.Mul(q, pow), r)
	api.AssertIsEqual(lhs, rhs)

	// 3) range proof for r: 0 <= r < 2^m via m-bit decomposition
	api.ToBinary(r, m) // enforces each bit is boolean AND reconstructs r from bits

	return q, r
}

func EnforceFloorDivPow2WithHint(api frontend.API, z frontend.Variable, m int) (frontend.Variable, frontend.Variable) {
	if m <= 0 {
		panic("m must be > 0")
	}
	pow := new(big.Int).Lsh(big.NewInt(1), uint(m)) // 2^m

	// 1) call hint: inputs = (z, m), outputs = (q, r)
	out, err := api.NewHint(FloorDivPow2Hint, 2, z, big.NewInt(int64(m)))
	if err != nil {
		panic(err)
	}
	q := out[0]
	r := out[1]

	// 2) main equation: z = q*2^m + r
	lhs := z
	rhs := api.Add(api.Mul(q, pow), r)
	api.AssertIsEqual(lhs, rhs)

	// 3) range proof for r: 0 <= r < 2^m via m-bit decomposition
	api.ToBinary(r, m) // enforces each bit is boolean AND reconstructs r from bits

	return q, r
}
