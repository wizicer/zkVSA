//go:build js && wasm

package main

import (
	"bytes"
	"encoding/json"
	"syscall/js"

	"github.com/consensys/gnark-crypto/ecc"
	"github.com/consensys/gnark/backend/groth16"
	"github.com/consensys/gnark/frontend"
	"github.com/consensys/gnark/frontend/cs/r1cs"
)

var isInitialized = false

// ProofData represents the serialized proof, verification key, and public values
type ProofData struct {
	Proof        []byte `json:"proof"`
	VK           []byte `json:"vk"`
	PublicValues []byte `json:"public_values"`
}

// SimpleCircuit defines the same circuit as in the original project
type SimpleCircuit struct {
	Secret      frontend.Variable `gnark:",secret"`
	PublicValue frontend.Variable `gnark:",public"`
}

// Define declares the circuit's constraints
func (circuit *SimpleCircuit) Define(api frontend.API) error {
	doubled := api.Mul(circuit.Secret, 2)
	api.AssertIsEqual(doubled, circuit.PublicValue)
	return nil
}

// verifyProof verifies a Groth16 proof
func verifyProof(this js.Value, args []js.Value) interface{} {
	js.Global().Get("console").Call("log", "verifyProof called with", len(args), "arguments")

	if len(args) != 1 {
		js.Global().Get("console").Call("log", "Error: Expected exactly one argument")
		result := js.Global().Get("Object").New()
		result.Set("success", false)
		result.Set("error", "Expected exactly one argument (proof data JSON)")
		return result
	}

	proofDataJSON := args[0].String()
	js.Global().Get("console").Call("log", "Proof data JSON length:", len(proofDataJSON))

	// Parse the proof data
	var proofData ProofData
	if err := json.Unmarshal([]byte(proofDataJSON), &proofData); err != nil {
		result := js.Global().Get("Object").New()
		result.Set("success", false)
		result.Set("error", "Failed to parse proof data: "+err.Error())
		return result
	}

	// Compile the circuit to get the constraint system
	var circuit SimpleCircuit
	_, err := frontend.Compile(ecc.BLS12_377.ScalarField(), r1cs.NewBuilder, &circuit)
	if err != nil {
		result := js.Global().Get("Object").New()
		result.Set("success", false)
		result.Set("error", "Failed to compile circuit: "+err.Error())
		return result
	}

	// Deserialize the verification key
	vk := groth16.NewVerifyingKey(ecc.BLS12_377)
	_, err = vk.ReadFrom(bytes.NewReader(proofData.VK))
	if err != nil {
		result := js.Global().Get("Object").New()
		result.Set("success", false)
		result.Set("error", "Failed to deserialize verification key: "+err.Error())
		return result
	}

	// Deserialize the proof
	proof := groth16.NewProof(ecc.BLS12_377)
	_, err = proof.ReadFrom(bytes.NewReader(proofData.Proof))
	if err != nil {
		result := js.Global().Get("Object").New()
		result.Set("success", false)
		result.Set("error", "Failed to deserialize proof: "+err.Error())
		return result
	}

	// Deserialize the public witness directly from bytes
	publicWitness, err := frontend.NewWitness(nil, ecc.BLS12_377.ScalarField())
	if err != nil {
		result := js.Global().Get("Object").New()
		result.Set("success", false)
		result.Set("error", "Failed to create public witness: "+err.Error())
		return result
	}

	_, err = publicWitness.ReadFrom(bytes.NewReader(proofData.PublicValues))
	if err != nil {
		result := js.Global().Get("Object").New()
		result.Set("success", false)
		result.Set("error", "Failed to deserialize public witness: "+err.Error())
		return result
	}

	// Verify the proof
	err = groth16.Verify(proof, vk, publicWitness)
	if err != nil {
		result := js.Global().Get("Object").New()
		result.Set("success", false)
		result.Set("error", "Proof verification failed: "+err.Error())
		return result
	}

	// Create a JavaScript object
	result := js.Global().Get("Object").New()
	result.Set("success", true)
	result.Set("message", "Proof verification successful!")
	return result
}

// Simple test function to check if basic Go functionality works
func testFunction(this js.Value, args []js.Value) interface{} {
	js.Global().Get("console").Call("log", "Test function called successfully!")
	result := js.Global().Get("Object").New()
	result.Set("success", true)
	result.Set("message", "Test function works!")
	return result
}

func main() {
	js.Global().Get("console").Call("log", "Go WASM module starting...")

	// Register the test function first
	js.Global().Set("testFunction", js.FuncOf(testFunction))
	js.Global().Get("console").Call("log", "Test function registered")

	// Register the verifyProof function globally
	js.Global().Set("verifyProof", js.FuncOf(verifyProof))
	js.Global().Get("console").Call("log", "verifyProof function registered")

	// Mark as initialized
	isInitialized = true
	js.Global().Get("console").Call("log", "WASM module fully initialized")

	// Keep the program running
	select {}
}
