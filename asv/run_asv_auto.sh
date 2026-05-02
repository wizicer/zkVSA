#!/bin/bash

# Automatic ASV testing script
# Converts Makefile functionality to bash with file existence checks
# Runs ASV tests only if corresponding result files don't exist

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

# Function equivalent to run_asv_multi from Makefile
run_asv_multi() {
    local enroll_dir="$1"
    local test_dirs="$2"
    local output_name="$3"
    
    local result_file="$RESULT_DIR/${output_name}_summary.json"
    
    # Check if result file already exists
    if [ -f "$result_file" ]; then
        echo "Skipping $output_name: Result file $result_file already exists"
        return 0
    fi
    
    # Read current GPU ID from file
    local current_gpu_id=$(get_gpu_id)
    echo "Running ASV test: $output_name (test_dirs: $test_dirs, GPU: $current_gpu_id)"
    
    # Run the ASV command
    python3 asv.py --data_root "$DATA_ROOT" --out out --test_dirs $test_dirs --gpu_id "$current_gpu_id" --enroll_dir "$enroll_dir"
    
    # Copy results
    if [ -f "out/summary.json" ]; then
        cp "out/summary.json" "$result_file"
        echo "✓ Completed $output_name: Results saved to $result_file"
    else
        echo "⚠ Warning: No summary.json found for $output_name"
    fi
    
    echo
}

echo "Starting automatic ASV testing..."
echo "Data root: $DATA_ROOT"
echo "Results directory: $RESULT_DIR"
echo "GPU config file: $GPU_CONFIG_FILE (current GPU: $(get_gpu_id))"
echo

# Test configuration arrays
SEMITONES=(3 2 1 -1 -2 -3)
VARIANTS=("f32" "f377")
TYPES=("clean" "other")

# Function to generate test directory names
# Usage: generate_test_dirs [type1] [type2] ...
# If no types specified, uses all types from TYPES array
generate_test_dirs() {
    local types_to_use=("$@")
    local all_dirs=()
    
    # If no types specified, use all types
    if [ ${#types_to_use[@]} -eq 0 ]; then
        types_to_use=("${TYPES[@]}")
    fi
    
    # Generate test combinations for specified types
    for type in "${types_to_use[@]}"; do
        # Add enrollment baseline
        all_dirs+=("enroll-${type}")

        for semitone in "${SEMITONES[@]}"; do
            for variant in "${VARIANTS[@]}"; do
                # Standard test directory
                all_dirs+=("test_${type}_${semitone}_${variant}")
                
                # Floor variant only for f377
                if [ "$variant" = "f377" ]; then
                    all_dirs+=("test_${type}_floor_${semitone}_${variant}")
                fi
            done
        done
    done
    
    echo "${all_dirs[@]}"
}

# Run all test groups using loops
echo "Starting comprehensive ASV testing..."

# Generate type-specific test directories
CLEAN_TEST_DIRS=($(generate_test_dirs "clean"))
OTHER_TEST_DIRS=($(generate_test_dirs "other"))
ALL_TEST_DIRS=($(generate_test_dirs))

echo "Generated ${#CLEAN_TEST_DIRS[@]} clean test directories"
echo "Generated ${#OTHER_TEST_DIRS[@]} other test directories"
echo "Generated ${#ALL_TEST_DIRS[@]} total test directories for anonymized enrollment"

# Run batch ASV tests with type-specific directories
CLEAN_DIRS_STRING="${CLEAN_TEST_DIRS[*]}"
OTHER_DIRS_STRING="${OTHER_TEST_DIRS[*]}"
ALL_DIRS_STRING="${ALL_TEST_DIRS[*]}"

run_asv_multi "enroll-clean" "$CLEAN_DIRS_STRING" "o_a_clean_tests_batch"
run_asv_multi "enroll-other" "$OTHER_DIRS_STRING" "o_a_other_tests_batch"
run_asv_multi "anonymized-enroll_0.5_f32" "$ALL_DIRS_STRING" "a05_f32_a_tests_batch"
run_asv_multi "anonymized-clean-enroll_0.5_f32" "$CLEAN_DIRS_STRING" "a05_f32_a_clean_tests_batch"

echo "=========================================="
echo "All ASV tests completed!"
echo "Results are available in the $RESULT_DIR directory"
echo "=========================================="
