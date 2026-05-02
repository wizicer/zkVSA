#!/usr/bin/env bash
set -euo pipefail

# Generate multiple pitch-shifted WAVs for a given input using various algorithms and semitone shifts,
# and create a minimal HTML page with audio players for the outputs.
#
# Usage:
#   scripts/generate_variants.sh <input.wav> <output_dir> [semitones_csv] [algos_csv]
#
# Examples:
#   scripts/generate_variants.sh data/common_voice_en_42706185_8k.wav target/out
#   scripts/generate_variants.sh data/in.wav out "-4,-2,-1,1,2,4" "ola,resample,td-psola,pv-tsm,pvtsm-m32"
#
# Notes:
# - Requires Rust toolchain. The script uses `cargo run --release`.
# - No JSON is generated.

if [[ $# -lt 2 ]]; then
  echo "Usage: $0 <input.wav> <output_dir> [semitones_csv] [algos_csv]" >&2
  exit 1
fi

INPUT=$(realpath "$1")
OUTDIR="$2"
SEMITONES_CSV=${3:-"-4,-2,-1,1,2,4"}
ALGOS_CSV=${4:-"ola,resample,td-psola,pv-tsm,pvtsm-m32"}

if [[ ! -f "$INPUT" ]]; then
  echo "Input file not found: $INPUT" >&2
  exit 1
fi

mkdir -p "$OUTDIR"

# Split CSV into arrays
IFS=',' read -r -a SEMITONES <<< "$SEMITONES_CSV"
IFS=',' read -r -a ALGOS <<< "$ALGOS_CSV"

# Build the project first (faster than repeated on-demand builds)
if ! cargo build --release >/dev/null; then
  echo "Failed to build project" >&2
  exit 1
fi

BIN=target/release/wav_pitchshift
# On Windows, the binary may have .exe extension; prefer non-suffixed if present
if [[ ! -x "$BIN" ]]; then
  if [[ -x "${BIN}.exe" ]]; then
    BIN="${BIN}.exe"
  else
    echo "Built binary not found: $BIN (or ${BIN}.exe)" >&2
    exit 1
  fi
fi

# Helper: sanitize semitone value for filename (keep minus and dot safe)
sanitize_semitone() {
  local s="$1"
  # Replace spaces (shouldn't exist) and forward slashes just in case
  s=${s// /}
  s=${s//\//_}
  echo "$s"
}

BASENAME=$(basename -- "$INPUT")
BASE_NOEXT=${BASENAME%.*}

# Track generated files
mapfile -t CREATED < <(echo)
CREATED=()

for s in "${SEMITONES[@]}"; do
  for algo in "${ALGOS[@]}"; do
    s_tag=$(sanitize_semitone "$s")
    outfile="$OUTDIR/${BASE_NOEXT}_${algo}_s${s_tag}.wav"
    outspec="$OUTDIR/${BASE_NOEXT}_${algo}_s${s_tag}_spec.png"
    echo "Generating: $outfile"
    # Generate WAV and its spectrogram
    "$BIN" "$INPUT" "$outfile" "$s" "$algo" --spectrogram "$outspec"
    CREATED+=("$(basename -- "$outfile")")
    CREATED+=("$(basename -- "$outspec")")
  done

done

# Copy original WAV to output for easy playback reference and generate its spectrogram
ORIG_COPY="$OUTDIR/${BASE_NOEXT}_original.wav"
cp -f "$INPUT" "$ORIG_COPY"
ORIG_SPEC="$OUTDIR/${BASE_NOEXT}_original_spec.png"
echo "Generating spectrogram: $ORIG_SPEC"
"$BIN" "$INPUT" "$ORIG_COPY" --spectrogram "$ORIG_SPEC"

# Generate an HTML index as a matrix: columns = algos, rows = semitones; cells = audio players
HTML="$OUTDIR/index.html"
{
  echo "<!doctype html>"
  echo "<html lang=\"en\">"
  echo "<head>"
  echo "  <meta charset=\"utf-8\">"
  echo "  <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">"
  echo "  <title>Generated WAV Variants</title>"
  echo "  <style>"
  echo "    body{font-family:system-ui,Segoe UI,Arial,sans-serif;margin:20px;max-width:1100px}"
  echo "    h1{font-size:1.35rem;margin:0 0 0.5rem}"
  echo "    h2{font-size:1rem;margin:1rem 0 0.5rem;color:#333}"
  echo "    table{border-collapse:collapse;width:100%}"
  echo "    th,td{border:1px solid #ddd;padding:8px;font-size:0.95rem;vertical-align:top}"
  echo "    thead th{background:#f2f2f2;min-width:70px}"
  echo "    tr:nth-child(even) td{background:#fafafa}"
  echo "    audio{width:100%}"
  echo "    code{background:#f5f5f5;padding:2px 4px;border-radius:4px}"
  echo "    img.spec{width:160px;height:70px;image-rendering:pixelated;border:1px solid #ddd;background:#fff;display:block;margin-top:6px}"
  echo "    .hidden{display:none !important}"
  echo "    .toggle-spec{font-size:0.85rem;padding:4px 8px;margin:4px 0;cursor:pointer}"
  echo "  </style>"
  echo "</head>"
  echo "<body>"
  echo "  <h1>Generated WAV Variants</h1>"
  echo "  <p>Input: <code>$BASENAME</code></p>"
  echo "  <div style=\"margin:8px 0 12px\"><button id=\"toggle-all\" class=\"toggle-spec\">Show all spectrograms</button></div>"
  echo "  <h2>Original</h2>"
  echo "  <img class=\"spec hidden\" src=\"$(basename -- \"$ORIG_SPEC\")\" alt=\"Original spectrogram\">"
  echo "  <audio controls src=\"$(basename -- \"$ORIG_COPY\")\" controlslist=\"nodownload noremoteplayback noplaybackrate\"></audio>"
  echo "  <h2>Matrix</h2>"
  echo "  <table>"
  echo -n "    <thead><tr><th>Semitone \\ Algo</th>"
  for algo in "${ALGOS[@]}"; do
    echo -n "<th>${algo}</th>"
  done
  echo "</tr></thead>"
  echo "    <tbody>"
  for s in "${SEMITONES[@]}"; do
    echo -n "      <tr><th>${s}</th>"
    s_tag=$(sanitize_semitone "$s")
    for algo in "${ALGOS[@]}"; do
      f="${BASE_NOEXT}_${algo}_s${s_tag}.wav"
      img="${BASE_NOEXT}_${algo}_s${s_tag}_spec.png"
      echo -n "<td><img class=\"spec hidden\" src=\"$img\" alt=\"Spectrogram\"><audio controls src=\"$f\" controlslist=\"nodownload noremoteplayback noplaybackrate\"></audio></td>"
    done
    echo "</tr>"
  done
  echo "    </tbody>"
  echo "  </table>"
  echo "  <script>"
  echo "  (function(){"
  echo "    var btn = document.getElementById('toggle-all');"
  echo "    function setAll(show){"
  echo "      var imgs = document.querySelectorAll('img.spec');"
  echo "      imgs.forEach(function(img){ img.classList.toggle('hidden', !show); });"
  echo "      if(btn){ btn.textContent = show ? 'Hide all spectrograms' : 'Show all spectrograms'; }"
  echo "    }"
  echo "    if(btn){"
  echo "      btn.addEventListener('click', function(){"
  echo "        var anyVisible = !!document.querySelector('img.spec:not(.hidden)');"
  echo "        setAll(!anyVisible);"
  echo "      });"
  echo "      setAll(false);"
  echo "    }"
  echo "  })();"
  echo "  </script>"
  echo "</body>"
  echo "</html>"
} > "$HTML"

echo "\nDone. Outputs in: $OUTDIR"
echo "Open $HTML in your browser to listen."
