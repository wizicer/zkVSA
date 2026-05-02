package main

import (
	"fmt"
	"math/big"

	"github.com/consensys/gnark/frontend"
	"github.com/consensys/gnark/std/hash/mimc"
	gkrposeidon2 "github.com/consensys/gnark/std/permutation/poseidon2/gkr-poseidon2"
	"github.com/consensys/gnark/std/signature/eddsa"

	t "github.com/consensys/gnark-crypto/ecc/twistededwards"
	te "github.com/consensys/gnark/std/algebra/native/twistededwards"
)

// Maps (t,k) to flattened one-dimensional index
func idx(t, k, N int) int { return t*N + k }

// ---------------- GKR Poseidon2 compression (feed-forward) ----------------

func foldPoseidon2GKR(pos2 *gkrposeidon2.GkrCompressions, xs []frontend.Variable) frontend.Variable {
	acc := frontend.Variable(0)
	for i := 0; i < len(xs); i++ {
		acc = pos2.Compress(acc, xs[i]) // Same feed-forward semantics as in the example
	}
	return acc
}

// ---------------- Entry circuit: commitments + EdDSA ----------------

type PvAudioSignedCircuit struct {
	// private input
	PhasesIn []frontend.Variable // Length U*N (row-major u,k)
	Salt     frontend.Variable   // Private salt for all commitments

	PubKey eddsa.PublicKey
	Sig    eddsa.Signature

	// private witness
	PhasesOut []frontend.Variable // Length U*N (row-major u,k)

	// public input
	Rs frontend.Variable `gnark:",public"` // Rs depends on semitone

	// public output
	InHash     frontend.Variable `gnark:",public"` // Poseidon2-GKR( salt || phases... )
	OutHash    frontend.Variable `gnark:",public"` // Poseidon2-GKR( salt || out... )
	PubKeyHash frontend.Variable `gnark:",public"` // Poseidon2-GKR( salt || pk.X || pk.Y )

	// circuit constants
	U     int // U frames
	l     int // N = 2^l frequency bins
	m     int // R_a = 2^m
	Omega []big.Int

	isHalfUp bool
}

var _ frontend.Circuit = (*PvAudioSignedCircuit)(nil)

func (c *PvAudioSignedCircuit) Define(api frontend.API) error {
	if c.U <= 0 || c.l <= 0 {
		return fmt.Errorf("U or l not set")
	}
	N := 1 << c.l
	if len(c.Omega) != N || len(c.PhasesIn) != c.U*N || len(c.PhasesOut) != c.U*N {
		return fmt.Errorf("shape mismatch")
	}
	if c.m <= 0 || c.m > 64 {
		return fmt.Errorf("m out of range")
	}

	// Create single GKR Poseidon2 compression instance
	pos2 := gkrposeidon2.NewGkrCompressions(api)

	// 1) PV accumulate
	err := PvAccumulateAllGadgetDyn(api, c.PhasesIn, c.PhasesOut, c.Omega, c.Rs, c.U, c.l, c.m, c.isHalfUp)
	if err != nil {
		return err
	}

	// 2) Commitments (each first mixed with salt; GKR Poseidon2 compressor)
	// InRoot
	inInputs := make([]frontend.Variable, 0, 1+len(c.PhasesIn))
	inInputs = append(inInputs, c.Salt)
	inInputs = append(inInputs, c.PhasesIn...)
	api.AssertIsEqual(c.InHash, foldPoseidon2GKR(pos2, inInputs))

	// OutRoot
	outInputs := make([]frontend.Variable, 0, 1+len(c.PhasesOut))
	outInputs = append(outInputs, c.Salt)
	outInputs = append(outInputs, c.PhasesOut...)
	api.AssertIsEqual(c.OutHash, foldPoseidon2GKR(pos2, outInputs))

	// 3) Public key hash (public key coordinates private, their hash public)
	pkInputs := []frontend.Variable{c.PubKey.A.X, c.PubKey.A.Y}
	api.AssertIsEqual(c.PubKeyHash, foldPoseidon2GKR(pos2, pkInputs))

	// 4) EdDSA verification: directly verify InRoot
	curve, err := te.NewEdCurve(api, t.BLS12_377)
	if err != nil {
		return err
	}
	h, _ := mimc.NewMiMC(api)
	eddsa.Verify(curve, c.Sig, c.InHash, c.PubKey, &h)

	return nil
}

// Constructor: set T/N/S before compilation and allocate slices (gnark requires fixed shape)
func NewPvAudioSignedCircuit(U, l, m int, isHalfUp bool) *PvAudioSignedCircuit {
	N := 1 << l
	omega := make([]big.Int, N)
	for k := 0; k < N; k++ {
		omega[k] = *big.NewInt(int64(k) * 2)
	}
	return &PvAudioSignedCircuit{
		U:         U,
		l:         l,
		m:         m,
		PhasesIn:  make([]frontend.Variable, U*N),
		PhasesOut: make([]frontend.Variable, U*N),
		Omega:     omega,
		isHalfUp:  isHalfUp,
	}
}
