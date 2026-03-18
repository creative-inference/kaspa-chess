// RISC Zero guest program — Kaspa Chess move validator
//
// This program runs inside the RISC Zero zkVM. The host provides
// (old_state, mv, new_state) as public inputs. The guest validates
// that the move is legal and the resulting state is correct, then
// commits the inputs so they appear in the receipt's journal.
//
// The RISC0-Groth16 proof of correct execution is verified on Kaspa L1
// via OpZkPrecompile (tag 0x20, KIP-16).

#![no_main]
risc0_zkvm::guest::entry!(main);

use risc0_zkvm::guest::env;
use kaspa_chess_core::{GameState, Move, validate_transition};

fn main() {
    // Read public inputs from the host
    let old_state: GameState = env::read();
    let mv: Move = env::read();
    let new_state: GameState = env::read();

    // Validate the move and resulting state
    validate_transition(&old_state, &mv, &new_state);

    // Commit all three to the journal — these become the public inputs
    // that OpZkPrecompile verifies on-chain
    env::commit(&old_state);
    env::commit(&mv);
    env::commit(&new_state);
}
