#!/bin/bash

# Script to run flat command with predefined semitone ranges
# Processes both enroll-clean and enroll-other sequentially

set -e

# Configuration
CORPUS="../corpus/librispeech/flat"
BIN="cargo run --release --bin dir_pvtsm --"
VARIANTS="f32,f377"
INPUT_TYPES=("clean" "other")

process_input_type() {
    local INPUT_TYPE="$1"
    local OUTPUT_DIR="$2"
    
    echo "Processing $INPUT_TYPE data..."
    echo "Input: $CORPUS/enroll-$INPUT_TYPE"
    echo "Output: $CORPUS"
    echo "Variants: $VARIANTS"
    echo

    SEMITONES=(-3 -2 -1 1 2 3)

    for semitone in "${SEMITONES[@]}"; do
        echo "Running flat command: semitone=$semitone"
        $BIN "$CORPUS/enroll-$INPUT_TYPE" "$CORPUS" --name "test_${INPUT_TYPE}" --semitones "$semitone.0" --variants "$VARIANTS"
        echo "Completed: semitone=$semitone, variant=$variant"
        echo

        echo "Running flat command: semitone=$semitone, variant=$variant, use_half_up=false"
        variant="f377"
        $BIN "$CORPUS/enroll-$INPUT_TYPE" "$CORPUS" --name "test_${INPUT_TYPE}_floor" --semitones "$semitone.0" --variants "$variant" --use-half-up=false
        echo "Completed: semitone=$semitone, variant=$variant, use_half_up=false"
        echo
    done

    echo "Running special anonymized-enroll with semitone 0.5..."
    variant="f32"
    $BIN "$CORPUS/enroll-$INPUT_TYPE" "$CORPUS" --name "anonymized-$INPUT_TYPE-enroll" --semitones "0.5" --variants "$variant"
    echo "Completed anonymized: semitone=0.5, variant=$variant"
    echo
}

echo "Starting batch processing of all input types..."
echo "=========================================="

# Process each input type
for INPUT_TYPE in "${INPUT_TYPES[@]}"; do
    process_input_type "$INPUT_TYPE" "$OUTPUT_DIR"
    
    echo "=========================================="
done

echo "All flat commands completed successfully!"
