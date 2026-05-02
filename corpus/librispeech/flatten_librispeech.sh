#!/usr/bin/env bash
set -euo pipefail

# Default paths
DEFAULT_BASE="./corpora/LibriSpeech"
DEFAULT_OUTPUT="./flat"

# Function to process transcript file and create individual txt files
process_transcript() {
  local trans_file="$1"
  local output_dir="$2"
  
  if [[ -f "$trans_file" ]]; then
    while IFS= read -r line; do
      if [[ -n "$line" ]]; then
        # Extract ID and content from each line
        local id="${line%% *}"
        local content="${line#* }"
        
        # Create individual txt file
        local txt_file="$output_dir/${id}.txt"
        echo "$content" > "$txt_file"
      fi
    done < "$trans_file"
  fi
}

# Function to flatten a directory
flatten_directory() {
  local input_dir="$1"
  local output_dir="$2"
  local dataset_name="$3"
  
  if [[ -d "$input_dir" ]]; then
    echo "Flattening $dataset_name..."
    mkdir -p "$output_dir"
    
    # Process flac files
    while IFS= read -r -d '' f; do
      rel="${f#$input_dir/}"
      spk="${rel%%/*}"
      base="$(basename "$f")"
      dest="$output_dir/$spk"
      mkdir -p "$dest"
      cp -n -- "$f" "$dest/$base"
    done < <(find "$input_dir" -type f -name '*.flac' -print0)
    
    # Process transcript files
    while IFS= read -r -d '' trans_file; do
      rel="${trans_file#$input_dir/}"
      spk="${rel%%/*}"
      dest="$output_dir/$spk"
      mkdir -p "$dest"
      process_transcript "$trans_file" "$dest"
    done < <(find "$input_dir" -type f -name '*.trans.txt' -print0)
    
    echo "Done flattening $dataset_name"
  else
    echo "Warning: $input_dir not found, skipping $dataset_name"
  fi
}

if [[ $# -eq 0 ]]; then
  # No arguments - process both test-clean and test-other
  echo "Processing test-clean and test-other with default paths..."
  
  flatten_directory "$DEFAULT_BASE/test-clean" "$DEFAULT_OUTPUT/enroll-clean" "test-clean"
  flatten_directory "$DEFAULT_BASE/test-other" "$DEFAULT_OUTPUT/enroll-other" "test-other"
  
  echo "All done. Flattened files to: $DEFAULT_OUTPUT"
  exit 0
fi

if [[ $# -lt 2 ]]; then
  echo "Usage: $0 [<input_root> <output_root>]"
  echo "  No args: Process both test-clean and test-other to default output"
  echo "  With args: $0 /path/to/input /path/to/output"
  exit 1
fi

in="${1%/}"
out="${2%/}"

flatten_directory "$in" "$out" "$(basename "$in")"
