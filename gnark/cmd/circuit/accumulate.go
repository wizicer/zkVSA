package main

import (
	"fmt"
	"math/big"

	"github.com/consensys/gnark/frontend"
)

// PvAccumulateAllGadgetDyn enforces:
//
// out[0,*] = phases[0,*]
// out[t,k] = out[t-1,k] + ((omega[k] + ( (phase[t,k] - phase[t-1,k] - (omega[k]*ha)/S )/ha )) * hs)/S
func PvAccumulateAllGadgetDyn(
	api frontend.API,
	phasesIn []frontend.Variable, // length U*N, row-major (u then k)
	phasesOut []frontend.Variable, // length U*N, row-major (u then k)
	omega []big.Int, // length N, treated as constants
	rs frontend.Variable,
	U, l, m int,
	isHalfUp bool,
) error {

	N := 1 << uint(l)

	if m <= 0 {
		return fmt.Errorf("m must be > 0")
	}
	if U < 2 {
		return fmt.Errorf("U must be >= 2")
	}
	if len(omega) != N {
		return fmt.Errorf("omega length %d != N %d", len(omega), N)
	}
	if len(phasesIn) != U*N {
		return fmt.Errorf("phasesIn length %d != U*N %d", len(phasesIn), U*N)
	}
	if len(phasesOut) != U*N {
		return fmt.Errorf("phasesOut length %d != U*N %d", len(phasesOut), U*N)
	}

	// Precompute big constants: 2^m, 2^(l+m+2), 2^l
	ra := new(big.Int).Lsh(big.NewInt(1), uint(m))          // 2^m
	minDphi := new(big.Int).Lsh(big.NewInt(1), uint(l+m+2)) // 2^(l+m+2)
	lmBig := new(big.Int).Lsh(big.NewInt(1), uint(l-m))     // 2^(l-m)

	// For each frequency bin k, carry a local accumulator `prev` that mirrors out[u-1,k].
	for k := 0; k < N; k++ {
		// Frame 0: pass-through (and initialize `prev`)
		prev := phasesIn[idx(0, k, N)]
		api.AssertIsEqual(phasesOut[idx(0, k, N)], prev)

		// Frames 1..U-1: accumulate into a temporary `acc`, then assert equals out[u,k], then update `prev`.
		for u := 1; u < U; u++ {
			// t1 = omega[k] * 2^m
			t1 := new(big.Int).Mul(&omega[k], ra)

			// dphi = in[u,k] - in[u-1,k] - t1
			dphi := api.Sub(
				api.Sub(phasesIn[idx(u, k, N)], phasesIn[idx(u-1, k, N)]),
				t1,
			)

			// unwrap to [0, 2^(l+1)): take the lower (l+1) bits of tt = dphi + 2^(l+m+2)
			tt := api.Add(dphi, minDphi)
			bits := api.ToBinary(tt, l+m+3)                // booleanizes + reconstructs tt
			dphiUnwrapped := api.FromBinary(bits[:l+1]...) // low (l+1) bits

			// rounding by 2^m (half-up), returns (q,r); we only need q
			var t2 frontend.Variable
			if isHalfUp {
				t2, _ = EnforceHalfUpDivPow2WithHint(api, dphiUnwrapped, m)
			} else {
				t2, _ = EnforceFloorDivPow2WithHint(api, dphiUnwrapped, m)
			}

			// trueFreq = omega[k] + (t2 - 2^(l-m))
			trueFreq := api.Add(omega[k], api.Sub(t2, lmBig))

			// step = trueFreq * rs
			t3 := api.Mul(trueFreq, rs)

			// acc = prev + step; enforce out[u,k] == acc
			acc := api.Add(prev, t3)
			api.AssertIsEqual(phasesOut[idx(u, k, N)], acc)

			// slide the accumulator window
			prev = acc
		}
	}

	return nil
}
