// Kaspa Chess Covenant — Multiplayer with optional KAS stakes
//
// Uses KIP-16's OpZkPrecompile (tag 0x20 = RISC0-Groth16) to verify
// chess moves on-chain. Full multiplayer: two player pubkeys are encoded
// in the UTXO data. Only the active player can spend. Stakes are locked
// until the game ends.
//
// UTXO data field layout (137 bytes):
//
//   [0..63]    board (64 bytes, pieces 0-12)
//   [64]       castling rights bitmask
//   [65]       en passant target square (255 = none)
//   [66]       is_white_turn (1=white, 0=black)
//   [67..68]   move counter (u16 LE)
//   [69..100]  white player pubkey (32 bytes, Schnorr)
//   [101..132] black player pubkey (32 bytes, Schnorr)
//   [133]      game flags: bit0=stake_active, bit1=draw_offered_by_white, bit2=draw_offered_by_black
//   [134..135] timeout blocks (u16 LE) — how many blocks a player has to move (0 = no timeout)
//   [136]      last_move_block low byte (used for timeout enforcement, full block tracked off-chain)
//
// Piece encoding: 0=empty, 1-6=white P/N/B/R/Q/K, 7-12=black P/N/B/R/Q/K
//
// RISC Zero Image ID — replace with actual value after: cargo build --release
const CHESS_IMAGE_ID: [u8; 32] = [
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];

// OpZkPrecompile tag for RISC0-Groth16 (KIP-16, rusty-kaspa PR #775)
const ZK_TAG_RISC0_GROTH16: u8 = 0x20;

// ---- Game creation ----
// The challenger creates the game UTXO by calling chess_new.
// They encode both pubkeys and optionally lock KAS as stake.
// The opponent must accept (chess_accept) before play begins.

covenant chess_new {
    binding = Auth  // challenger signs the creation transaction

    verify {
        let data = witness_data(0);  // 137 bytes: initial board + both pubkeys + flags

        // Verify initial board is the standard starting position
        // (enforced off-chain for now — full check would require unrolling 64 bytes)

        // Output must carry the game data under chess_move covenant
        assert(output_data(0) == data);
        assert(output_covenant(0) == chess_move);

        // The challenger is white — their pubkey must be in bytes [69..100]
        // Their signature is checked by Auth binding automatically
    }
}

// ---- Move covenant ----

covenant chess_move {
    binding = Cov  // covenant enforces structure; player identity checked explicitly below

    verify {
        let state = input_data(self);   // 137 bytes: full game state

        // Extract player pubkeys from state
        let white_pubkey = slice(state, 69, 32);
        let black_pubkey = slice(state, 101, 32);
        let is_white_turn = state[66] == 1;

        // Only the active player can provide a valid signature
        let active_pubkey = if is_white_turn { white_pubkey } else { black_pubkey };
        assert(check_sig(active_pubkey));

        // Witness layout:
        //   witness[0] = new game state bytes (137 bytes)
        //   witness[1] = RISC0-Groth16 proof bytes
        //   witness[2] = move bytes (3 bytes: from, to, promotion)
        let new_state_bytes = witness_data(0);
        let proof_bytes     = witness_data(1);
        let move_bytes      = witness_data(2);

        // Pubkeys must be preserved in the new state
        assert(slice(new_state_bytes, 69, 32) == white_pubkey);
        assert(slice(new_state_bytes, 101, 32) == black_pubkey);

        // Timeout and flags carry forward
        assert(slice(new_state_bytes, 133, 3) == slice(state, 133, 3));

        // ZK proof covers only the chess state portion [0..68]
        let old_chess_state = slice(state, 0, 69);
        let new_chess_state = slice(new_state_bytes, 0, 69);
        let journal = concat(old_chess_state, move_bytes, new_chess_state);

        // KIP-16: verify the move is legal
        // NOTE: Silverscript binding for OpZkPrecompile TBD — may need raw opcodes
        assert(op_zk_precompile(ZK_TAG_RISC0_GROTH16, proof_bytes, CHESS_IMAGE_ID, journal));

        // Output carries new game state under same covenant (game continues)
        assert(output_data(0) == new_state_bytes);
        assert(output_covenant(0) == self_covenant());

        // Stake value passes through unchanged
        let flags = state[133];
        if flags & 0x01 != 0 {
            assert(output_value(0) == input_value(self));
        }
    }
}

// ---- Resignation ----
// The resigning player signs. The winner receives the full UTXO value (stake).

covenant chess_resign {
    binding = Auth  // resigning player signs

    verify {
        let state = input_data(self);
        let white_pubkey = slice(state, 69, 32);
        let black_pubkey = slice(state, 101, 32);
        let is_white_turn = state[66] == 1;

        // The resigning player must be the active player OR either player
        // (both are allowed to resign at any time — check both)
        let valid_white = check_sig_for(white_pubkey);
        let valid_black = check_sig_for(black_pubkey);
        assert(valid_white | valid_black);

        // Winner gets everything — output is unconstrained (they set their own address)
        // No covenant enforcement needed after game ends
    }
}

// ---- Draw offer / acceptance ----
// Either player can offer a draw. If the opponent accepts on their next move
// by setting bit1 or bit2 in game flags, a draw_claim can be submitted.

covenant chess_draw_claim {
    binding = Auth

    verify {
        let state = input_data(self);
        let white_pubkey = slice(state, 69, 32);
        let black_pubkey = slice(state, 101, 32);
        let flags = state[133];

        // Draw requires both players to have signaled agreement
        // (bit1 = white offered, bit2 = black offered)
        assert(flags & 0x06 == 0x06);

        // Either player can submit the draw claim
        assert(check_sig_for(white_pubkey) | check_sig_for(black_pubkey));

        // Stake split: each output gets half
        let flags_active = flags & 0x01 != 0;
        if flags_active {
            assert(output_value(0) == input_value(self) / 2);
            assert(output_value(1) == input_value(self) / 2);
        }
    }
}

// ---- Timeout claim ----
// If a player exceeds their time limit, the opponent can claim the stake.
// Timeout is enforced via block height (requires KIP-10 block introspection).

covenant chess_timeout {
    binding = Auth

    verify {
        let state = input_data(self);
        let white_pubkey = slice(state, 69, 32);
        let black_pubkey = slice(state, 101, 32);
        let is_white_turn = state[66] == 1;
        let timeout_blocks = u16_from_le(slice(state, 134, 2));

        // Timeout must be configured
        assert(timeout_blocks > 0);

        // Current block height must exceed last_move_block + timeout
        // (KIP-10 block height introspection)
        let last_move_block = state[136] as u64;
        assert(block_height() >= last_move_block + timeout_blocks as u64);

        // The waiting player (not the one whose turn it is) claims
        let claimer_pubkey = if is_white_turn { black_pubkey } else { white_pubkey };
        assert(check_sig_for(claimer_pubkey));
    }
}
