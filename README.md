# Kaspa Chess — Trustless ZK-verified Chess on TN12

A fully trustless chess game on Kaspa's TN12 testnet. Chess move validation
runs in a RISC Zero guest program (Rust) off-chain. The Silverscript covenant
verifies the RISC0-Groth16 proof on-chain via `OpZkPrecompile` (tag `0x20`,
KIP-16). No trusted server. No arbitration.

## How it works

### Board state lives in a UTXO

Each game is a UTXO on Kaspa. The data field encodes the full game state:

```
Bytes [0..63]  board (64 squares, piece encoding 0-12)
Byte  [64]     castling rights bitmask
Byte  [65]     en passant target square (255 = none)
Byte  [66]     whose turn (1=white, 0=black)
Bytes [67..68] move counter (u16 LE)
```

### Making a move

1. Player runs the RISC Zero host locally: `./scripts/prove.sh state.json move.json`
2. Host executes the chess guest inside the zkVM, validating full FIDE rules
3. Host generates a compact RISC0-Groth16 proof (~140 sigops on L1)
4. Player builds a Kaspa transaction spending the game UTXO with witness:
   - `witness[0]` — new game state (69 bytes)
   - `witness[1]` — Groth16 proof bytes
   - `witness[2]` — move encoding (from, to, promotion — 3 bytes)
5. The Silverscript covenant calls `op_zk_precompile(0x20, proof, IMAGE_ID, journal)`
6. Kaspa nodes verify via `OpZkPrecompile` — if valid, UTXO is spent and a new UTXO
   with the updated board is created

### What the L1 does NOT do

- Does not re-execute chess logic
- Does not know the rules of chess
- Does not trust either player

It verifies a 140-sigop Groth16 proof. Rules live in the RISC Zero guest.

## KIP-16 OpZkPrecompile — what's confirmed

From rusty-kaspa PR #775 (saefstroem):

| Tag  | System            | Sigop cost | Notes                          |
|------|-------------------|-----------|--------------------------------|
| 0x20 | RISC0-Groth16     | 140       | Compact, fast L1 verification  |
| 0x21 | RISC0-Succinct    | 740       | STARK-based, quantum-resistant |

`MAX_SCRIPT_SIZE` raised to 250,000 bytes in the same PR.

Stack interface (subject to confirmation — exact opcode sequence):
```
<proof_bytes> <tag: 0x20> OP_ZKPRECOMPILE
```

The precompile verifies the proof against the guest's Image ID and checks the
journal (public inputs committed by `env::commit()` in the guest).

## This is the vProgs model

| Component     | Chess game             | vProgs (general)          |
|---------------|------------------------|---------------------------|
| State         | Board UTXO             | Sovereign vProg account   |
| Execution     | RISC Zero guest (Rust) | Off-chain Rust/zkVM       |
| Proof         | RISC0-Groth16          | ZK validity proof         |
| Verification  | OpZkPrecompile 0x20    | KIP-16 verifier opcode    |
| L1 settlement | Covenant spend         | Lane-sequenced proof      |

## Project structure

```
apps/chess/
├── circuit/
│   ├── Cargo.toml          Rust workspace
│   ├── core/               Shared chess logic (no_std compatible)
│   │   └── src/lib.rs      Board, move validation, apply_move
│   ├── guest/              RISC Zero guest program (runs in zkVM)
│   │   └── src/main.rs     Reads inputs, validates, commits journal
│   └── host/               Proof generation host
│       └── src/main.rs     Executes guest, generates Groth16 proof
├── covenant/
│   └── chess.ss            Silverscript KIP-16 covenant
├── frontend/
│   ├── index.html          Chess UI (cypherpunk aesthetic)
│   ├── chess.js            Game logic + prover/RPC stubs
│   └── styles.css
└── scripts/
    ├── setup.sh            Install RISC Zero, build circuit
    └── prove.sh            Generate proof for a move (JSON input)
```

## Setup

### Prerequisites

- Rust toolchain (rustup)
- Git, curl, bash

### 1. Build the circuit

```bash
cd apps/chess
chmod +x scripts/setup.sh
./scripts/setup.sh
```

Installs `rzup`, builds the RISC Zero guest and host. The Image ID for
`CHESS_IMAGE_ID` in `covenant/chess.ss` is printed by `risc0-build` during
compilation. Copy it in.

### 2. Run the frontend locally

```bash
cd apps/chess/frontend
python3 -m http.server 8080
# open http://localhost:8080
```

Fully playable chess locally. Kaspa RPC and proof submission are stubbed —
see `// TODO: wire Kaspa RPC` in `chess.js`.

### 3. Generate a proof

Create `state.json` (current board) and `move.json` (the move):

```json
// state.json — starting position, white to move
{
  "board": [4,2,3,5,6,3,2,4, 1,1,1,1,1,1,1,1,
            0,0,0,0,0,0,0,0, 0,0,0,0,0,0,0,0,
            0,0,0,0,0,0,0,0, 0,0,0,0,0,0,0,0,
            7,7,7,7,7,7,7,7, 10,8,9,11,12,9,8,10],
  "castling": 15,
  "ep_square": 255,
  "white_to_move": true
}

// move.json — e2-e4 (square 12 -> square 28)
{ "from": 12, "to": 28, "promotion": 0 }
```

```bash
./scripts/prove.sh state.json move.json
# outputs: proof.hex, journal.hex
```

### 4. Wire Kaspa RPC (TN12)

1. Confirm exact Silverscript `op_zk_precompile` binding (or use raw opcodes)
2. Deploy `covenant/chess.ss` with the Silverscript compiler
3. Create a game UTXO with initial board state
4. Implement `submitProofToKaspa()` in `chess.js` using the Kaspa SDK

## Open questions / TODOs

- **Silverscript binding** — Does Silverscript expose `op_zk_precompile()` or require raw `OP_ZKPRECOMPILE` opcode? (Not in current Silverscript repo)
- **Stack interface confirmation** — Exact push order for proof bytes + tag
- **Image ID** — Computed at build time; needs updating in covenant after every guest change
- **Proof size** — Measure Groth16 seal size vs Kaspa witness element size limit
- **Game termination** — Checkmate/stalemate adjudicated off-chain for now
- **Timeouts** — Add time controls via block height covenants
- **Stakes** — Add KAS wagering

## References

- [rusty-kaspa PR #775](https://github.com/kaspanet/rusty-kaspa/pull/775) — KIP-16 OpZkPrecompile implementation
- [KIP-16 PR](https://github.com/kaspanet/kips/pull/31) — Specification (open)
- [RISC Zero docs](https://dev.risczero.com) — Guest/host programming model
- [vprogs.xyz](https://vprogs.xyz) — Full vProgs architecture
