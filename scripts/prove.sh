#!/usr/bin/env bash
# Generate a RISC0-Groth16 proof for a chess move.
#
# Usage: ./prove.sh <state.json> <move.json>
#
# state.json format:
# {
#   "board": [4,2,3,5,6,3,2,4,1,1,1,1,1,1,1,1,0,...,7,7,7,7,7,7,7,7,10,8,9,11,12,9,8,10],
#   "castling": 15,
#   "ep_square": 255,
#   "white_to_move": true
# }
#
# move.json format:
# { "from": 12, "to": 28, "promotion": 0 }
#
# Output: proof.hex and journal.hex written to current directory

set -e

STATE=${1:-state.json}
MOVE=${2:-move.json}

CIRCUIT_DIR="$(dirname "$0")/../circuit"

echo "==> Generating RISC0-Groth16 proof..."
echo "    State: $STATE"
echo "    Move:  $MOVE"

cd "$CIRCUIT_DIR"
cargo run --release --bin kaspa-chess-host -- "$STATE" "$MOVE"

echo ""
echo "proof.hex and journal.hex written."
echo "Use these as witness[1] and the journal as the expected public input in the Kaspa covenant spend."
