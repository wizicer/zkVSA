package main

import (
	crand "crypto/rand"
	"fmt"
	"log"
	"math/big"

	"github.com/consensys/gnark-crypto/ecc"
	tedIDs "github.com/consensys/gnark-crypto/ecc/twistededwards"
	hashfun "github.com/consensys/gnark-crypto/hash"
	cryptoeddsa "github.com/consensys/gnark-crypto/signature/eddsa"

	"github.com/consensys/gnark/backend/groth16"
	"github.com/consensys/gnark/frontend"
	"github.com/consensys/gnark/frontend/cs/r1cs"

	stdtw "github.com/consensys/gnark/std/algebra/native/twistededwards"
	"github.com/consensys/gnark/std/hash/mimc"
	stdeddsa "github.com/consensys/gnark/std/signature/eddsa"
	"github.com/consensys/gnark/test"
)

// Minimal EdDSA verification circuit on the BLS12-377 twisted Edwards companion curve.
// No Poseidon, no extra logic — just a signature check using MiMC as the hash.
type eddsaVerifyCircuit struct {
	PublicKey stdeddsa.PublicKey
	Signature stdeddsa.Signature
	Message   frontend.Variable // the message field element (could be public if desired)
}

func (c *eddsaVerifyCircuit) Define(api frontend.API) error {
	curve, err := stdtw.NewEdCurve(api, tedIDs.BLS12_377)
	if err != nil {
		return err
	}
	h, _ := mimc.NewMiMC(api) // MiMC over BLS12-377 Fr
	stdeddsa.Verify(curve, c.Signature, c.Message, c.PublicKey, &h)
	return nil
}

func main() {
	// --- Generate a keypair and sign a small field element message ---
	priv, err := cryptoeddsa.New(tedIDs.BLS12_377, crand.Reader)
	if err != nil {
		log.Fatalf("eddsa keygen: %v", err)
	}
	pk := priv.Public()

	m := big.NewInt(123456789) // example message as a field element
	sig, err := priv.Sign(m.Bytes(), hashfun.MIMC_BLS12_377.New())
	if err != nil {
		log.Fatalf("eddsa sign: %v", err)
	}

	// --- Build circuit assignment ---
	assign := eddsaVerifyCircuit{}
	assign.PublicKey.Assign(tedIDs.BLS12_377, pk.Bytes())
	assign.Signature.Assign(tedIDs.BLS12_377, sig)
	assign.Message = m // same numeric value inside the circuit

	// --- Fast witness check: no SNARK setup/prove, just constraint satisfaction ---
	if err := test.IsSolved(new(eddsaVerifyCircuit), &assign, ecc.BLS12_377.ScalarField()); err != nil {
		log.Fatalf("❌ EdDSA witness check failed: %v", err)
	}
	fmt.Println("✅ EdDSA pre-check passed (witness looks sane). Proceeding to Groth16...")

	// ---------------------------- Now run Groth16 for the EdDSA mini-circuit ---------------------------- //
	tmpl := &eddsaVerifyCircuit{}
	ccs, err := frontend.Compile(ecc.BLS12_377.ScalarField(), r1cs.NewBuilder, tmpl)
	if err != nil {
		log.Fatalf("compile eddsaVerifyCircuit: %v", err)
	}

	fmt.Println("Groth16 setup...")
	pkSNARK, vkSNARK, err := groth16.Setup(ccs)
	if err != nil {
		log.Fatalf("setup: %v", err)
	}
	fmt.Println("Setup done")

	wFull, err := frontend.NewWitness(&assign, ecc.BLS12_377.ScalarField())
	if err != nil {
		log.Fatalf("full witness: %v", err)
	}
	wPublic, err := frontend.NewWitness(&assign, ecc.BLS12_377.ScalarField(), frontend.PublicOnly())
	if err != nil {
		log.Fatalf("public witness: %v", err)
	}

	fmt.Println("Proving (Groth16)...")
	proof, err := groth16.Prove(ccs, pkSNARK, wFull)
	if err != nil {
		log.Fatalf("prove: %v", err)
	}
	fmt.Println("Proof done, verifying ...")
	if err := groth16.Verify(proof, vkSNARK, wPublic); err != nil {
		log.Fatalf("verify failed: %v", err)
	}
	fmt.Println("🎉 EdDSA mini-circuit: Groth16 proof verified.")
}
