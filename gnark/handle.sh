DIR="../waveop/target/site_data"
SEMITONES=(3 2 1 -1 -2 -3)

set -e

function run() {
  for semitone in "${SEMITONES[@]}"; do
    NAME="${1}_f377_${semitone}"
    echo "Processing $NAME..."
    go run ./cmd/circuit/ \
      --input ${DIR}/${NAME}_input.cbor \
      --output ${DIR}/${NAME}_output.cbor
    mv proof_data.json ${DIR}/${NAME}_proof.json
    echo "Completed $NAME"
  done
}

run "672-122797-0005"
run "1580-141084-0003"
run "3729-6852-0006"
run "5639-40744-0010"