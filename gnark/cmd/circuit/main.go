package main

import (
	"bytes"
	"encoding/csv"
	"encoding/json"
	"flag"
	"fmt"
	"io"
	"log"
	"os"
	"path/filepath"
	"regexp"
	"strconv"
	"strings"
	"time"

	"github.com/consensys/gnark-crypto/ecc"
	"golang.org/x/sys/unix"

	"github.com/consensys/gnark/backend"
	"github.com/consensys/gnark/backend/groth16"
	"github.com/consensys/gnark/backend/witness"
	"github.com/consensys/gnark/constraint"
	"github.com/consensys/gnark/constraint/solver"
	"github.com/consensys/gnark/frontend"
	"github.com/consensys/gnark/frontend/cs/r1cs"
	gkrposeidon2 "github.com/consensys/gnark/std/permutation/poseidon2/gkr-poseidon2"
	// "math/rand"
)

// ProofData represents the serialized proof, verification key, and public values
type ProofData struct {
	Proof        []byte `json:"proof"`
	VK           []byte `json:"vk"`
	PublicValues []byte `json:"public_values"`
}

func main() {
	// Parse command line arguments
	var inputFile = flag.String("input", "", "Path to input CBOR file")
	var outputFile = flag.String("output", "", "Path to expected output CBOR file")
	var benchmark = flag.Bool("benchmark", false, "Enable benchmark mode with multiple proving/verification iterations and CSV export")
	flag.Parse()

	// Enable GKR Poseidon2 compressor in the solver
	gkrposeidon2.RegisterGkrSolverOptions(ecc.BLS12_377)
	solver.RegisterHint(HalfUpDivPow2Hint)
	solver.RegisterHint(FloorDivPow2Hint)

	// Determine circuit parameters
	var U, l, m int
	var err error
	var isHalfUp bool

	if *inputFile != "" {
		// Read parameters from CBOR input file
		U, l, m, err = readCBORParameters(*inputFile)
		isHalfUp = false
		if err != nil {
			log.Fatalf("failed to read CBOR parameters: %v", err)
		}
	} else {
		// Use default parameters
		U, l, m = 10, 9, 7
		isHalfUp = false
	}

	// ---------- Load or Compile circuit ----------
	circ := NewPvAudioSignedCircuit(U, l, m, isHalfUp)
	ccs, compileTime, err := loadOrCompileCircuit(U, l, m, circ)
	if err != nil {
		log.Fatalf("compile: %v", err)
	}

	// Generate witness
	var assign *PvAudioSignedCircuit

	if *inputFile != "" {
		// Use CBOR input file
		assign, err = generateWitnessFromCBOR(*inputFile, *outputFile, isHalfUp)
		if err != nil {
			log.Fatalf("failed to generate witness from CBOR: %v", err)
		}
	} else {
		// Use default random generation
		assign, err = generateWitness(U, l, m, isHalfUp)
		if err != nil {
			log.Fatalf("failed to generate witness: %v", err)
		}
	}

	// ---------- Setup / Prove / Verify ----------
	// Measure witness generation time
	witnessStart := time.Now()
	wFull, err := frontend.NewWitness(assign, ecc.BLS12_377.ScalarField())
	if err != nil {
		log.Fatalf("full witness: %v", err)
	}
	wPub, err := frontend.NewWitness(assign, ecc.BLS12_377.ScalarField(), frontend.PublicOnly())
	if err != nil {
		log.Fatalf("public witness: %v", err)
	}
	witnessTime := time.Since(witnessStart)

	err = ccs.IsSolved(wFull)
	if err != nil {
		if err.Error() != "placeholder function: to be replaced by commitment computation" {
			log.Fatalf("is solved: %v", err)
		}
	}

	pk, vk, setupTiming, err := loadOrSetupKeys(U, l, m, ccs)
	if err != nil {
		log.Fatalf("setup: %v", err)
	}

	printStatistics(ccs, pk, vk)

	// Benchmark mode: multiple proving/verification iterations and CSV export
	if *benchmark {
		const numIterations = 5
		solverTimes := make([]time.Duration, numIterations)
		proverTimes := make([]time.Duration, numIterations)

		// Setup solver options
		solverOpts := []solver.Option{
			// solver.WithHints(HalfUpDivPow2Hint),
		}

		var proof groth16.Proof
		fmt.Printf("Running %d proving iterations with separated timing...\n", numIterations)
		for i := 0; i < numIterations; i++ {
			// Run Prove with log capture to get both solver and prover times
			tp, logs, err := ProveWithLogs(ccs, pk, wFull, solverOpts...)
			if err != nil {
				log.Fatalf("prove failed on iteration %d: %v", i+1, err)
			}
			proof = tp

			// Parse solver and prover times from logs
			solverTime, proverTime, err := parseTimingFromLogs(logs)
			if err != nil {
				fmt.Printf("Warning: could not parse timing from logs on iteration %d: %v\n", i+1, err)
				fmt.Printf("Logs: %s\n", logs)
				// Skip this iteration if we can't parse timing
				continue
			}

			solverTimes[i] = solverTime
			proverTimes[i] = proverTime

			fmt.Printf("Iteration %d - Solver: %v, Prover: %v\n", i+1, solverTimes[i], proverTimes[i])
		}

		// Calculate statistics (excluding first)
		solverStats := calculateStats(solverTimes)
		proverStats := calculateStats(proverTimes)
		fmt.Println("\n--- Solver Statistics ---")
		fmt.Printf("Average time (excluding first): %v\n", solverStats.avg)
		fmt.Printf("Minimum time: %v\n", solverStats.min)
		fmt.Printf("Maximum time: %v\n", solverStats.max)
		fmt.Printf("Total time: %v\n", solverStats.total)
		fmt.Println("\n--- Prover Statistics ---")
		fmt.Printf("Average time (excluding first): %v\n", proverStats.avg)
		fmt.Printf("Minimum time: %v\n", proverStats.min)
		fmt.Printf("Maximum time: %v\n", proverStats.max)
		fmt.Printf("Total time: %v\n", proverStats.total)
		fmt.Println("--------------------------")

		fmt.Println("Waiting 10 seconds before verification timing tests...")
		time.Sleep(10 * time.Second)

		// Verification loop with timing
		verificationTimes := make([]time.Duration, numIterations)

		fmt.Printf("Running %d verification iterations...\n", numIterations)
		for i := 0; i < numIterations; i++ {
			start := time.Now()
			if err := groth16.Verify(proof, vk, wPub); err != nil {
				log.Fatalf("verify failed on iteration %d: %v", i+1, err)
			}
			verificationTimes[i] = time.Since(start)
			fmt.Printf("Verification %d: %v\n", i+1, verificationTimes[i])
		}

		// Calculate verification statistics (excluding first)
		verificationStats := calculateStats(verificationTimes)
		fmt.Println("\n--- Verification Statistics ---")
		fmt.Printf("Average time (excluding first): %v\n", verificationStats.avg)
		fmt.Printf("Minimum time: %v\n", verificationStats.min)
		fmt.Printf("Maximum time: %v\n", verificationStats.max)
		fmt.Printf("Total time: %v\n", verificationStats.total)
		fmt.Println("------------------------------")

		var proofBuf bytes.Buffer
		if _, err := proof.WriteTo(&proofBuf); err != nil {
			log.Fatalf("serialize proof: %v", err)
		}
		// Export to CSV
		if err := exportToCSV(U, l, m, ccs, pk, vk, proofBuf.Len(), compileTime, witnessTime, setupTiming, solverStats, proverStats, verificationStats); err != nil {
			log.Printf("Failed to export CSV: %v", err)
		} else {
			fmt.Println("Results exported to benchmark_results.csv")
		}
	} else {
		// Non-benchmark mode
		fmt.Println("Proving (for serialization)...")
		proof, err := groth16.Prove(ccs, pk, wFull)
		if err != nil {
			log.Fatalf("prove: %v", err)
		}

		// ---------- Serialize proof data to JSON ----------
		var proofBuf, vkBuf, pubBuf bytes.Buffer
		if _, err := proof.WriteTo(&proofBuf); err != nil {
			log.Fatalf("serialize proof: %v", err)
		}
		if _, err := vk.WriteTo(&vkBuf); err != nil {
			log.Fatalf("serialize vk: %v", err)
		}
		if _, err := wPub.WriteTo(&pubBuf); err != nil {
			log.Fatalf("serialize public witness: %v", err)
		}

		pd := ProofData{
			Proof:        proofBuf.Bytes(),
			VK:           vkBuf.Bytes(),
			PublicValues: pubBuf.Bytes(),
		}

		jsonBytes, err := json.MarshalIndent(pd, "", "  ")
		if err != nil {
			log.Fatalf("marshal proof data: %v", err)
		}

		if err := os.WriteFile("proof_data.json", jsonBytes, 0644); err != nil {
			log.Fatalf("write proof data: %v", err)
		}
		fmt.Println("Proof and verification key saved to proof_data.json")

		// Print proof size
		fmt.Printf("Proof size: %d bytes\n", proofBuf.Len())
		fmt.Println("Verifying proof...")
		if err := groth16.Verify(proof, vk, wPub); err != nil {
			log.Fatalf("verification failed: %v", err)
		}
		fmt.Println("✅ Proof verification successful!")
	}

	fmt.Println("✅ Pv+Commitments (GKR Poseidon2) + EdDSA on BLS12-377: All operations completed successfully.")
}

type TimingStats struct {
	avg   time.Duration
	min   time.Duration
	max   time.Duration
	total time.Duration
}

func calculateStats(times []time.Duration) TimingStats {
	if len(times) == 0 {
		return TimingStats{}
	}

	// Calculate stats excluding the first measurement
	startIdx := 1
	if len(times) == 1 {
		startIdx = 0 // fallback if only one measurement
	}

	var total time.Duration
	minTime := times[startIdx]
	maxTime := times[startIdx]

	for i := startIdx; i < len(times); i++ {
		t := times[i]
		total += t
		if t < minTime {
			minTime = t
		}
		if t > maxTime {
			maxTime = t
		}
	}

	count := len(times) - startIdx
	avgTime := total / time.Duration(count)

	return TimingStats{
		avg:   avgTime,
		min:   minTime,
		max:   maxTime,
		total: total,
	}
}

// // ProveWithLogs runs groth16.Prove and returns the proof and all logs printed during proving.
// // It captures stderr because gnark prints Solve logs there (api.Println, debug lines, etc.).
// func ProveWithLogs(cs constraint.ConstraintSystem, pk groth16.ProvingKey, wit witness.Witness, solverOpts ...solver.Option) (proof groth16.Proof, logs string, err error) {
// 	// 1) capture stderr
// 	oldStderr := os.Stdout
// 	r, w, _ := os.Pipe()
// 	os.Stdout = w

// 	// 2) run Prove (pass solver options here!)
// 	proof, err = groth16.Prove(cs, pk, wit, backend.WithSolverOptions(solverOpts...))

// 	// 3) restore and collect
// 	_ = w.Close()
// 	var buf bytes.Buffer
// 	_, _ = io.Copy(&buf, r)
// 	_ = r.Close()
// 	os.Stdout = oldStderr

// 	logs = buf.String()
// 	return proof, logs, err
// }

// ProveWithLogs runs groth16.Prove and returns the proof and all logs printed during proving.
// Captures BOTH stdout and stderr at the file-descriptor (dup2) level so even pre-initialized
// loggers are redirected into our pipe.
func ProveWithLogs(
	cs constraint.ConstraintSystem,
	pk groth16.ProvingKey,
	wit witness.Witness,
	solverOpts ...solver.Option,
) (proof groth16.Proof, logs string, err error) {
	// 1) create a pipe and backup current stdout/stderr FDs
	r, w, _ := os.Pipe()

	stdoutFD := int(os.Stdout.Fd())
	stderrFD := int(os.Stderr.Fd())
	savedOut, _ := unix.Dup(stdoutFD) // duplicates of original FDs
	savedErr, _ := unix.Dup(stderrFD)

	// 2) redirect FD 1 and 2 to the pipe writer
	_ = unix.Dup2(int(w.Fd()), stdoutFD)
	_ = unix.Dup2(int(w.Fd()), stderrFD)

	// 3) drain the pipe asynchronously
	done := make(chan string, 1)
	go func() {
		var buf bytes.Buffer
		_, _ = io.Copy(&buf, r)
		_ = r.Close()
		done <- buf.String()
	}()

	// 4) run Prove (pass solver options here)
	proof, err = groth16.Prove(cs, pk, wit, backend.WithSolverOptions(solverOpts...))

	// 5) restore FDs and collect logs
	_ = w.Close()
	_ = unix.Dup2(savedOut, stdoutFD)
	_ = unix.Dup2(savedErr, stderrFD)
	_ = unix.Close(savedOut)
	_ = unix.Close(savedErr)

	logs = <-done
	return
}

var ansiRE = regexp.MustCompile(`\x1b\[[0-?]*[ -/]*[@-~]`)

func stripANSI(s string) string {
	return ansiRE.ReplaceAllString(s, "")
}

func parseTimingFromLogs(logOutput string) (solverTime, proverTime time.Duration, err error) {
	// Split log output into lines for line-by-line processing
	lines := strings.Split(logOutput, "\n")

	// Parse solver time: "constraint system solver done ... took=7697.827127"
	solverRegex := regexp.MustCompile(`.*constraint system solver done.*took=([0-9.]+)`)
	// Parse prover time: "prover done ... took=2253.871778"
	proverRegex := regexp.MustCompile(`.*prover done.*took=([0-9.]+)`)

	for _, line := range lines {
		println("parsing line: ", stripANSI(line))
		line = strings.TrimSpace(stripANSI(line))
		if line == "" {
			continue
		}

		// Check for solver timing
		if solverMatch := solverRegex.FindStringSubmatch(line); len(solverMatch) > 1 {
			if ms, parseErr := strconv.ParseFloat(solverMatch[1], 64); parseErr == nil {
				solverTime = time.Duration(ms * float64(time.Millisecond))
			}
		}

		// Check for prover timing
		if proverMatch := proverRegex.FindStringSubmatch(line); len(proverMatch) > 1 {
			if ms, parseErr := strconv.ParseFloat(proverMatch[1], 64); parseErr == nil {
				proverTime = time.Duration(ms * float64(time.Millisecond))
			}
		}
	}

	if solverTime == 0 && proverTime == 0 {
		return 0, 0, fmt.Errorf("no timing information found in logs: %s", logOutput)
	}

	return solverTime, proverTime, nil
}

func getTargetDir() string {
	return "target"
}

func getCircuitFilename(U, l, m int) string {
	return filepath.Join(getTargetDir(), fmt.Sprintf("circuit_U%d_l%d_m%d.json", U, l, m))
}

func getProvingKeyFilename(U, l, m int) string {
	return filepath.Join(getTargetDir(), fmt.Sprintf("pk_U%d_l%d_m%d.bin", U, l, m))
}

func getVerifyingKeyFilename(U, l, m int) string {
	return filepath.Join(getTargetDir(), fmt.Sprintf("vk_U%d_l%d_m%d.bin", U, l, m))
}

func loadOrCompileCircuit(U, l, m int, circ *PvAudioSignedCircuit) (constraint.ConstraintSystem, time.Duration, error) {
	// Ensure target directory exists
	if err := os.MkdirAll(getTargetDir(), 0755); err != nil {
		return nil, 0, fmt.Errorf("failed to create target directory: %v", err)
	}

	circuitFile := getCircuitFilename(U, l, m)

	// Try to load existing circuit
	if _, err := os.Stat(circuitFile); err == nil {
		fmt.Printf("Loading existing circuit from %s...\n", circuitFile)
		// For now, we'll still need to compile since gnark doesn't have direct serialization
		// But we can check if the file exists as a marker
		fmt.Println("Circuit file exists, but recompiling for compatibility...")
	}

	fmt.Println("Compiling circuit...")
	compileStart := time.Now()
	ccs, err := frontend.Compile(ecc.BLS12_377.ScalarField(), r1cs.NewBuilder, circ)
	compileTime := time.Since(compileStart)
	if err != nil {
		return nil, compileTime, err
	}
	fmt.Printf("Circuit compilation took: %v\n", compileTime)

	// Save circuit marker (we can't serialize the actual ccs easily)
	markerData := map[string]interface{}{
		"U":           U,
		"l":           l,
		"m":           m,
		"constraints": ccs.GetNbConstraints(),
		"public_vars": ccs.GetNbPublicVariables(),
		"secret_vars": ccs.GetNbSecretVariables(),
		"compiled_at": time.Now().Format(time.RFC3339),
	}

	markerBytes, err := json.MarshalIndent(markerData, "", "  ")
	if err != nil {
		return ccs, compileTime, nil // Don't fail if we can't save marker
	}

	if err := os.WriteFile(circuitFile, markerBytes, 0644); err != nil {
		fmt.Printf("Warning: failed to save circuit marker: %v\n", err)
	} else {
		fmt.Printf("Circuit marker saved to %s\n", circuitFile)
	}

	return ccs, compileTime, nil
}

type SetupTiming struct {
	LoadTime  time.Duration
	SetupTime time.Duration
	SaveTime  time.Duration
}

func loadOrSetupKeys(U, l, m int, ccs constraint.ConstraintSystem) (groth16.ProvingKey, groth16.VerifyingKey, SetupTiming, error) {
	pkFile := getProvingKeyFilename(U, l, m)
	vkFile := getVerifyingKeyFilename(U, l, m)

	// Try to load existing keys
	if _, err := os.Stat(pkFile); err == nil {
		if _, err := os.Stat(vkFile); err == nil {
			fmt.Printf("Loading existing keys from %s and %s...\n", pkFile, vkFile)

			loadStart := time.Now()
			// Load proving key
			pk := groth16.NewProvingKey(ecc.BLS12_377)
			pkFileHandle, err := os.Open(pkFile)
			if err != nil {
				fmt.Printf("Failed to open proving key file: %v, regenerating...\n", err)
			} else {
				defer pkFileHandle.Close()
				if _, err := pk.ReadFrom(pkFileHandle); err != nil {
					fmt.Printf("Failed to read proving key: %v, regenerating...\n", err)
				} else {
					// Load verifying key
					vk := groth16.NewVerifyingKey(ecc.BLS12_377)
					vkFileHandle, err := os.Open(vkFile)
					if err != nil {
						fmt.Printf("Failed to open verifying key file: %v, regenerating...\n", err)
					} else {
						defer vkFileHandle.Close()
						if _, err := vk.ReadFrom(vkFileHandle); err != nil {
							fmt.Printf("Failed to read verifying key: %v, regenerating...\n", err)
						} else {
							timing := SetupTiming{
								LoadTime: time.Since(loadStart),
							}
							fmt.Printf("Keys loaded successfully in %v!\n", timing.LoadTime)
							return pk, vk, timing, nil
						}
					}
				}
			}
		}
	}

	// Generate new keys
	var timing SetupTiming
	fmt.Println("Groth16 setup...")
	setupStart := time.Now()
	pk, vk, err := groth16.Setup(ccs)
	timing.SetupTime = time.Since(setupStart)
	if err != nil {
		return pk, vk, timing, err
	}
	fmt.Printf("Setup done in %v.\n", timing.SetupTime)

	// Save keys
	saveStart := time.Now()
	// // Save proving key
	// pkFileHandle, err := os.Create(pkFile)
	// if err != nil {
	// 	fmt.Printf("Warning: failed to create proving key file: %v\n", err)
	// } else {
	// 	defer pkFileHandle.Close()
	// 	if _, err := pk.WriteTo(pkFileHandle); err != nil {
	// 		fmt.Printf("Warning: failed to write proving key: %v\n", err)
	// 	} else {
	// 		fmt.Printf("Proving key saved to %s\n", pkFile)
	// 	}
	// }

	// // Save verifying key
	// vkFileHandle, err := os.Create(vkFile)
	// if err != nil {
	// 	fmt.Printf("Warning: failed to create verifying key file: %v\n", err)
	// } else {
	// 	defer vkFileHandle.Close()
	// 	if _, err := vk.WriteTo(vkFileHandle); err != nil {
	// 		fmt.Printf("Warning: failed to write verifying key: %v\n", err)
	// 	} else {
	// 		fmt.Printf("Verifying key saved to %s\n", vkFile)
	// 	}
	// }
	timing.SaveTime = time.Since(saveStart)
	fmt.Printf("Keys saved in %v\n", timing.SaveTime)

	return pk, vk, timing, nil
}

func exportToCSV(U, l, m int, ccs constraint.ConstraintSystem, pk groth16.ProvingKey, vk groth16.VerifyingKey, proofSize int, compileTime, witnessTime time.Duration, setupTiming SetupTiming, solverStats, proverStats, verificationStats TimingStats) error {
	filename := "benchmark_results.csv"

	// Check if file exists to determine if we need headers
	fileExists := true
	if _, err := os.Stat(filename); os.IsNotExist(err) {
		fileExists = false
	}

	file, err := os.OpenFile(filename, os.O_APPEND|os.O_CREATE|os.O_WRONLY, 0644)
	if err != nil {
		return err
	}
	defer file.Close()

	writer := csv.NewWriter(file)
	defer writer.Flush()

	// Write headers if file is new
	if !fileExists {
		headers := []string{
			"U", "l", "m", "constraints", "public_variables", "private_variables",
			"pk_size", "vk_size", "proof_size", "compile_time_ms", "witness_time_ms",
			"key_load_time_ms", "setup_time_ms", "key_save_time_ms",
			"solver_time_avg_ms", "solver_time_min_ms", "solver_time_max_ms",
			"prover_time_avg_ms", "prover_time_min_ms", "prover_time_max_ms",
			"verify_time_avg_ms", "verify_time_min_ms", "verify_time_max_ms",
		}
		if err := writer.Write(headers); err != nil {
			return err
		}
	}

	// Get key sizes
	var pkBuf, vkBuf bytes.Buffer
	pk.WriteTo(&pkBuf)
	vk.WriteTo(&vkBuf)

	// Prepare data row
	record := []string{
		strconv.Itoa(U),
		strconv.Itoa(l),
		strconv.Itoa(m),
		strconv.Itoa(ccs.GetNbConstraints()),
		strconv.Itoa(ccs.GetNbPublicVariables()),
		strconv.Itoa(ccs.GetNbSecretVariables()),
		strconv.Itoa(pkBuf.Len()),
		strconv.Itoa(vkBuf.Len()),
		strconv.Itoa(proofSize),
		strconv.FormatFloat(float64(compileTime.Nanoseconds())/1e6, 'f', 3, 64),
		strconv.FormatFloat(float64(witnessTime.Nanoseconds())/1e6, 'f', 3, 64),
		strconv.FormatFloat(float64(setupTiming.LoadTime.Nanoseconds())/1e6, 'f', 3, 64),
		strconv.FormatFloat(float64(setupTiming.SetupTime.Nanoseconds())/1e6, 'f', 3, 64),
		strconv.FormatFloat(float64(setupTiming.SaveTime.Nanoseconds())/1e6, 'f', 3, 64),
		strconv.FormatFloat(float64(solverStats.avg.Nanoseconds())/1e6, 'f', 3, 64),
		strconv.FormatFloat(float64(solverStats.min.Nanoseconds())/1e6, 'f', 3, 64),
		strconv.FormatFloat(float64(solverStats.max.Nanoseconds())/1e6, 'f', 3, 64),
		strconv.FormatFloat(float64(proverStats.avg.Nanoseconds())/1e6, 'f', 3, 64),
		strconv.FormatFloat(float64(proverStats.min.Nanoseconds())/1e6, 'f', 3, 64),
		strconv.FormatFloat(float64(proverStats.max.Nanoseconds())/1e6, 'f', 3, 64),
		strconv.FormatFloat(float64(verificationStats.avg.Nanoseconds())/1e6, 'f', 3, 64),
		strconv.FormatFloat(float64(verificationStats.min.Nanoseconds())/1e6, 'f', 3, 64),
		strconv.FormatFloat(float64(verificationStats.max.Nanoseconds())/1e6, 'f', 3, 64),
	}

	return writer.Write(record)
}

func printStatistics(ccs constraint.ConstraintSystem, pk groth16.ProvingKey, vk groth16.VerifyingKey) {
	fmt.Println("\n--- Circuit Statistics ---")
	fmt.Printf("Constraints: %d\n", ccs.GetNbConstraints())
	fmt.Printf("Public Variables: %d\n", ccs.GetNbPublicVariables())
	fmt.Printf("Secret Variables: %d\n", ccs.GetNbSecretVariables())

	var pkBuf, vkBuf bytes.Buffer
	pk.WriteTo(&pkBuf)
	vk.WriteTo(&vkBuf)

	fmt.Printf("Proving Key Size: %d bytes\n", pkBuf.Len())
	fmt.Printf("Verifying Key Size: %d bytes\n", vkBuf.Len())
	fmt.Println("------------------------")
}
