#!/usr/bin/env bash
set -euo pipefail

# Downloads LibriSpeech test sets and extracts into corpus/librispeech/corpora
# Usage: bash download_librispeech.sh

TEST_CLEAN_URL="https://openslr.trmal.net/resources/12/test-clean.tar.gz"
TEST_OTHER_URL="https://openslr.trmal.net/resources/12/test-other.tar.gz"

# Resolve directories
BASE_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TARGET_DIR="$BASE_DIR/librispeech/corpora"
TMP_DIR="$BASE_DIR/librispeech/tmp"

mkdir -p "$TARGET_DIR" "$TMP_DIR"

have_cmd() { command -v "$1" >/dev/null 2>&1; }

if ! have_cmd curl; then
  echo "Error: curl is required but not found in PATH." >&2
  exit 1
fi
if ! have_cmd tar; then
  echo "Error: tar is required but not found in PATH." >&2
  exit 1
fi

 download_and_extract() {
  local url="$1"
  local base
  base="$(basename "$url")"
  local tarpath="$TMP_DIR/$base"

  local subset
  if [[ "$url" == *"test-clean"* ]]; then
    subset="test-clean"
  else
    subset="test-other"
  fi

  local marker="$TARGET_DIR/LibriSpeech/$subset"

  if [[ -d "$marker" ]]; then
    echo "Already present: $marker — skipping."
    return
  fi

  echo "Downloading: $url"
  curl -L --fail --retry 3 --retry-delay 3 -o "$tarpath" "$url"

  echo "Extracting: $tarpath -> $TARGET_DIR"
  tar -xzf "$tarpath" -C "$TARGET_DIR"

  echo "Cleaning up: $tarpath"
  rm -f "$tarpath"
}

 download_and_extract "$TEST_CLEAN_URL"
 download_and_extract "$TEST_OTHER_URL"

echo "Done. Extracted into: $TARGET_DIR/LibriSpeech/{test-clean,test-other}"
