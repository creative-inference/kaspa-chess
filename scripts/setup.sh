#!/usr/bin/env bash
set -e

# Install RISC Zero toolchain and build the chess guest + host

echo "==> Installing RISC Zero toolchain..."
curl -L https://risczero.com/install | bash
source "$HOME/.bashrc" 2>/dev/null || true
rzup install

echo "==> RISC Zero version:"
rzup --version

echo "==> Building chess circuit (guest + host)..."
cd "$(dirname "$0")/../circuit"
cargo build --release

echo ""
echo "Done. Artifacts:"
echo "  guest ELF:  circuit/guest/target/riscv32im-risc0-zkvm-elf/release/kaspa_chess_guest"
echo "  host binary: circuit/target/release/kaspa-chess-host"
echo ""
echo "The RISC Zero Image ID (CHESS_IMAGE_ID) is printed by risc0-build during compilation."
echo "Copy it into covenant/chess.ss CHESS_IMAGE_ID constant."
