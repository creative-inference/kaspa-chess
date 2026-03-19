// Kaspa Chess — RISC Zero host
//
// Generates a RISC0-Groth16 proof for a chess move, then prints the
// proof bytes and journal (public inputs) ready for submission to Kaspa
// via OpZkPrecompile (tag 0x20, KIP-16).
//
// Usage:
//   cargo run -- --state state.json --move move.json
//
// Output:
//   proof_hex: <hex-encoded proof bytes for OpZkPrecompile witness>
//   journal_hex: <hex-encoded public inputs = old_state || move || new_state>

use std::path::PathBuf;
use risc0_zkvm::{default_prover, ExecutorEnv, ProverOpts};
use kaspa_chess_core::{GameState, Move, apply_move, new_castling, new_ep_square};
use serde::{Deserialize, Serialize};

// The compiled guest ELF is embedded at build time by risc0-build
const CHESS_GUEST_ELF: &[u8] = include_bytes!("../../guest/target/riscv32im-risc0-zkvm-elf/release/kaspa_chess_guest");
const CHESS_GUEST_ID: [u32; 8] = [0u32; 8]; // replaced by risc0-build

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();

    // CLI: read state + move from JSON files
    // Usage: kaspa-chess-host [state.json] [move.json]
    // state.json includes chess state only (board, castling, ep_square, white_to_move)
    // Player pubkeys and metadata are in the UTXO data but not part of the ZK proof
    let state_path = args.get(1).map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("state.json"));
    let move_path = args.get(2).map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("move.json"));

    let old_state: GameState = serde_json::from_str(&std::fs::read_to_string(&state_path)?)?;
    let mv: Move = serde_json::from_str(&std::fs::read_to_string(&move_path)?)?;

    // Compute new state
    let new_board = apply_move(&old_state, &mv);
    let new_state = GameState {
        board: new_board,
        castling: new_castling(old_state.castling, &mv),
        ep_square: new_ep_square(&mv, &old_state.board),
        white_to_move: !old_state.white_to_move,
    };

    println!("Generating RISC0-Groth16 proof...");

    // Build executor environment with inputs
    let env = ExecutorEnv::builder()
        .write(&old_state)?
        .write(&mv)?
        .write(&new_state)?
        .build()?;

    // Generate Groth16 proof (tag 0x20 for OpZkPrecompile)
    let prover = default_prover();
    let receipt = prover.prove_with_opts(
        env,
        CHESS_GUEST_ELF,
        &ProverOpts::groth16(),
    )?.receipt;

    // Extract Groth16 proof bytes
    let groth16_receipt = receipt.inner.groth16()
        .expect("expected Groth16 receipt");
    let proof_bytes = groth16_receipt.seal.clone();
    let journal_bytes = receipt.journal.bytes.clone();

    // OpZkPrecompile witness layout (tag 0x20 = RISC0-Groth16):
    //   Stack (top to bottom when executing):
    //   [1]  tag byte: 0x20
    //   [2]  proof bytes (groth16 seal)
    //   OpZkPrecompile
    //
    // Public inputs are the journal — committed by the guest via env::commit()
    // The covenant reads journal bytes as: old_state || move || new_state
    println!("\n=== Proof generated ===");
    println!("proof_hex:   {}", hex::encode(&proof_bytes));
    println!("journal_hex: {}", hex::encode(&journal_bytes));
    println!("tag:         0x20 (RISC0-Groth16, KIP-16 OpZkPrecompile)");

    // Write to files for use in covenant witness
    std::fs::write("proof.hex", hex::encode(&proof_bytes))?;
    std::fs::write("journal.hex", hex::encode(&journal_bytes))?;
    println!("\nWritten to proof.hex and journal.hex");

    Ok(())
}
