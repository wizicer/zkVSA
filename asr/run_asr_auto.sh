#!/bin/bash

# Automatic ASR testing script
# Converts Makefile functionality to bash with file existence checks
# Runs ASR tests only if corresponding result files don't exist

set -e

# Configuration
DATA_ROOT="../corpus/librispeech/flat"
RESULT_DIR="results"
GPU_CONFIG_FILE="gpu_id.txt"

# Create results directory if it doesn't exist
mkdir -p "$RESULT_DIR"

# Function to read GPU ID from external file
get_gpu_id() {
    if [ -f "$GPU_CONFIG_FILE" ]; then
        local gpu_id=$(cat "$GPU_CONFIG_FILE" | tr -d '[:space:]')
        if [[ "$gpu_id" =~ ^[0-9]+$ ]]; then
            echo "$gpu_id"
        else
            echo "0"  # Default if file contains invalid content
        fi
    else
        echo "0"  # Default if file doesn't exist
    fi
}

# Function equivalent to run_asr_multi from Makefile
run_asr_multi() {
    local preset="$1"
    local input_name="$2" 
    local output_name="$input_name"
    
    local result_file="$RESULT_DIR/${preset}_${output_name}_metrics.json"
    
    # Check if result file already exists
    if [ -f "$result_file" ]; then
        echo "Skipping $output_name: Result file $result_file already exists"
        return 0
    fi
    
    # Read current GPU ID from file
    local current_gpu_id=$(get_gpu_id)
    echo "Running ASR test: $output_name (preset: $preset, data: $input_name, GPU: $current_gpu_id)"
    
    # Run the ASR command
    python3 asr.py --data_root "$DATA_ROOT/$input_name" --preset "$preset" \
        --out out \
        --gpu_id "$current_gpu_id"
    
    # Copy results
    if [ -f "out/metrics.json" ]; then
        cp "out/metrics.json" "$result_file"
        echo "✓ Completed $output_name: Results saved to $result_file"
    else
        echo "⚠ Warning: No metrics.json found for $output_name"
    fi
    
    echo
}

echo "Starting automatic ASR testing..."
echo "Data root: $DATA_ROOT"
echo "Results directory: $RESULT_DIR"
echo "GPU config file: $GPU_CONFIG_FILE (current GPU: $(get_gpu_id))"
echo

# Test configuration arrays
SEMITONES=(3 2 1 -1 -2 -3)
# VARIANTS=("f32" "m32" "f377")
VARIANTS=("f32" "f377")
TYPES=("clean" "other")
MODEL="whisper-large-v3"

# Benchmark configurations
declare -A BENCHMARK_CONFIGS=(
    ["tb-small"]="whisper-small"
    ["tb-medium"]="whisper-medium"
    ["tb-large"]="whisper-large-v3"
    ["tb-wav2vec2"]="wav2vec2-960h"
    ["tb-xlsr-53"]="xlsr-53"
)

# Function to run semitone tests for a specific semitone value
run_semitone_tests() {
    local semitone="$1"
    local group_name="Test Group $semitone"
    
    echo "=========================================="
    echo "Running $group_name"
    echo "=========================================="
    
    for variant in "${VARIANTS[@]}"; do
        for type in "${TYPES[@]}"; do
            local data_dir="test_${type}_${semitone}_${variant}"
            run_asr_multi "$MODEL" "$data_dir"

            if [ "$variant" = "f377" ]; then
                local data_dir="test_${type}_floor_${semitone}_${variant}"
                run_asr_multi "$MODEL" "$data_dir"
            fi
        done
    done
}

# Function to run benchmark tests
run_benchmark_tests() {
    echo "=========================================="
    echo "Running Benchmark Tests"
    echo "=========================================="
    
    for test_name in "${!BENCHMARK_CONFIGS[@]}"; do
        local preset="${BENCHMARK_CONFIGS[$test_name]}"
        run_asr_multi "$preset" "enroll"
    done
}

# Run all test groups using loops
echo "Starting comprehensive ASR testing..."

# Run semitone tests for all configured values
for semitone in "${SEMITONES[@]}"; do
    run_semitone_tests "$semitone"
done

# Run baseline tests
run_asr_multi "$MODEL" "anonymized-enroll_0.5_f32"
run_asr_multi "$MODEL" "enroll-clean"
run_asr_multi "$MODEL" "enroll-other"

echo "=========================================="
echo "All ASR tests completed!"
echo "Results are available in the $RESULT_DIR directory"
echo "=========================================="

# Summary of results
echo "Summary of generated results:"
find "$RESULT_DIR" -name "*.json" -type f | sort
