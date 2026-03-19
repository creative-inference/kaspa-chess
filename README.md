# Kaspa Chess

Trustless multiplayer chess on Kaspa TN12. Move validation runs in a RISC Zero
guest program off-chain. The RISC0-Groth16 proof is verified on-chain via
`OpZkPrecompile` (tag `0x20`, KIP-16). Stakes are locked in the game UTXO and
released to the winner. No server. No arbitration. No trust.

## How it works

### The game is a UTXO

Each active game is a single UTXO on Kaspa. Its data field encodes the full game
state — board position, whose turn it is, and both players' public keys:

```
Bytes [0..63]    board (64 squares, piece encoding 0-12)
Byte  [64]       castling rights bitmask
Byte  [65]       en passant target square (255 = none)
Byte  [66]       whose turn (1=white, 0=black)
Bytes [67..68]   move counter (u16 LE)
Bytes [69..100]  white player pubkey (32 bytes, Schnorr)
Bytes [101..132] black player pubkey (32 bytes, Schnorr)
Byte  [133]      flags: bit0=stake active, bit1=white draw offer, bit2=black draw offer
Bytes [134..135] timeout in blocks (u16 LE, 0=no timeout)
Byte  [136]      last move block (low byte, for timeout enforcement)
```

### Making a move

1. The active player generates a RISC0-Groth16 proof locally:
   ```bash
   ./scripts/prove.sh state.json move.json
   # outputs: proof.hex, journal.hex
   ```
2. They build a Kaspa transaction spending the game UTXO with three witnesses:
   - `witness[0]` — new game state (137 bytes)
   - `witness[1]` — Groth16 proof bytes
   - `witness[2]` — move encoding (from, to, promotion — 3 bytes)
3. The `chess_move` covenant checks:
   - The spender's signature matches the active player's pubkey
   - The ZK proof is valid (`OpZkPrecompile` tag `0x20`)
   - Both player pubkeys are unchanged in the new state
   - The stake value passes through
4. Kaspa nodes verify the proof. If valid, a new UTXO with the updated board is created.

### What the L1 does NOT do

- Does not re-execute chess logic
- Does not know the rules of chess
- Does not trust either player

It verifies one Groth16 proof (140 sigops). The rules live entirely in the RISC Zero guest.

## Covenants

| Covenant | Spending party | Purpose |
|---|---|---|
| `chess_new` | Challenger (Auth) | Create game UTXO with both pubkeys, optional stake |
| `chess_move` | Active player (Cov + sig) | Submit legal move + ZK proof, stake carries forward |
| `chess_resign` | Either player (Auth) | Resign — winner receives full stake |
| `chess_draw_claim` | Either player (Auth) | Claim draw when both flags set — stake split 50/50 |
| `chess_timeout` | Waiting player (Auth) | Claim stake if opponent exceeds block-height time limit |

## KIP-16 OpZkPrecompile

From [rusty-kaspa PR #775](https://github.com/kaspanet/rusty-kaspa/pull/775):

| Tag | System | Sigop cost |
|-----|--------|-----------|
| `0x20` | RISC0-Groth16 | 140 |
| `0x21` | RISC0-Succinct | 740 |

`MAX_SCRIPT_SIZE` raised to 250,000 bytes in the same PR.

Stack interface (confirm with KIP-16 spec before deploying):
```
<proof_bytes> <0x20> OP_ZKPRECOMPILE
```

## Project structure

```
kaspa-chess/
├── circuit/
│   ├── Cargo.toml          Rust workspace
│   ├── core/src/lib.rs     Chess logic: board, move validation, apply_move
│   ├── guest/src/main.rs   RISC Zero guest — validates move, commits journal
│   └── host/src/main.rs    Proof generation host — outputs proof.hex + journal.hex
├── covenant/
│   └── chess.ss            Silverscript covenants (move, resign, draw, timeout)
├── frontend/
│   ├── index.html          Browser chess UI
│   ├── chess.js            Game logic + Kaspa RPC stubs
│   └── styles.css          Cypherpunk styling
└── scripts/
    ├── setup.sh            Install RISC Zero, build circuit
    └── prove.sh            Generate proof from state.json + move.json
```

## Setup

### Prerequisites

- Rust toolchain (`rustup`)
- Git, curl, bash (macOS/Linux/WSL)

### 1. Build the circuit

```bash
chmod +x scripts/setup.sh
./scripts/setup.sh
```

Installs `rzup`, builds the RISC Zero guest and host. The **Image ID** for
`CHESS_IMAGE_ID` in `covenant/chess.ss` is printed by `risc0-build` during
compilation — copy it in before deploying.

### 2. Run the frontend locally

```bash
cd frontend
python3 -m http.server 8080
# open http://localhost:8080
```

Fully playable chess in the browser. Kaspa RPC and proof submission are stubbed —
see `// TODO: wire Kaspa RPC` in `chess.js`.

### 3. Generate a proof

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

// move.json — e2 to e4 (square 12 -> square 28)
{ "from": 12, "to": 28, "promotion": 0 }
```

```bash
./scripts/prove.sh state.json move.json
# outputs proof.hex and journal.hex
```

### 4. Deploy on TN12

1. Build the circuit and note the Image ID
2. Update `CHESS_IMAGE_ID` in `covenant/chess.ss`
3. Confirm the `op_zk_precompile` Silverscript binding (or use raw `OP_ZKPRECOMPILE`)
4. Compile and deploy the covenant with the Silverscript compiler
5. Implement `submitProofToKaspa()` in `chess.js` using the Kaspa SDK

## Security properties

- **No illegal moves possible** — rejected by every Kaspa node via ZK proof verification
- **No impersonation** — only the active player's Schnorr signature can spend the UTXO
- **No stake theft** — stake value is enforced to pass through on every move
- **No stalling** — optional block-height timeout allows the waiting player to claim
- **No trusted server** — all game logic is client-side + on-chain

## This is the vProgs model

This app demonstrates the vProgs architecture using only what is available on TN12 today:

| Component | Chess | vProgs (general) |
|---|---|---|
| State | Game UTXO (137 bytes) | Sovereign vProg account |
| Execution | RISC Zero guest (Rust) | Off-chain Rust / zkVM |
| Proof | RISC0-Groth16 | ZK validity proof |
| Verification | OpZkPrecompile `0x20` | KIP-16 verifier opcode |
| Composability | Single game UTXO | Cross-vProg atomic transactions |

When vProgs ship, a chess tournament could atomically interact with a wagering
vProg, leaderboard vProg, and prize pool vProg in a single transaction.

## Open questions

- **Silverscript binding** — `op_zk_precompile()` not yet in Silverscript repo; raw opcodes may be needed
- **Stack interface** — exact push order for proof bytes + tag needs confirmation from KIP-16 spec
- **Image ID** — recomputed on every guest change; covenant must be redeployed
- **Checkmate detection** — currently adjudicated off-chain; a `chess_checkmate` covenant could enforce it
- **Full timeout** — `last_move_block` currently stores low byte only; full 64-bit block height needs KIP-10 introspection

## References

- [rusty-kaspa PR #775](https://github.com/kaspanet/rusty-kaspa/pull/775) — KIP-16 OpZkPrecompile implementation (saefstroem)
- [KIP-16 PR #31](https://github.com/kaspanet/kips/pull/31) — Specification (open)
- [RISC Zero docs](https://dev.risczero.com) — Guest/host programming model
- [Silverscript](https://github.com/kaspanet/silverscript) — Kaspa covenant language
- [vprogs.xyz](https://vprogs.xyz) — Full vProgs architecture
