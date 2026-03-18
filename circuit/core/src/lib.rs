// kaspa-chess-core
// Shared chess logic used by both the RISC Zero guest (inside the zkVM)
// and the host (proof generation, Kaspa RPC submission).
//
// Piece encoding:
//   0=empty
//   1=wP  2=wN  3=wB  4=wR  5=wQ  6=wK
//   7=bP  8=bN  9=bB  10=bR 11=bQ 12=bK
//
// Squares: 0=a1 .. 63=h8  (rank-major: sq = rank*8 + file)

use serde::{Deserialize, Serialize};

// ---- Types ----

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GameState {
    pub board: [u8; 64],
    /// Castling rights bitmask:
    ///   bit 0 = white kingside, bit 1 = white queenside
    ///   bit 2 = black kingside, bit 3 = black queenside
    pub castling: u8,
    /// En passant target square, 255 = none
    pub ep_square: u8,
    pub white_to_move: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Move {
    pub from: u8,
    pub to: u8,
    /// 0=none, 2=knight, 3=bishop, 4=rook, 5=queen
    pub promotion: u8,
}

// ---- Board helpers ----

pub fn rank(sq: u8) -> u8 { sq / 8 }
pub fn file(sq: u8) -> u8 { sq % 8 }
pub fn sq(r: u8, f: u8) -> u8 { r * 8 + f }

pub fn is_white(p: u8) -> bool { p >= 1 && p <= 6 }
pub fn is_black(p: u8) -> bool { p >= 7 && p <= 12 }
pub fn piece_type(p: u8) -> u8 { if p > 6 { p - 6 } else { p } }
pub fn is_empty(p: u8) -> bool { p == 0 }
pub fn is_friendly(p: u8, white: bool) -> bool { if white { is_white(p) } else { is_black(p) } }
pub fn is_enemy(p: u8, white: bool) -> bool { if white { is_black(p) } else { is_white(p) } }

pub fn find_king(board: &[u8; 64], white: bool) -> u8 {
    let king = if white { 6u8 } else { 12u8 };
    board.iter().position(|&p| p == king).expect("king not found") as u8
}

// ---- Attack detection ----

pub fn is_attacked_by(board: &[u8; 64], target: u8, by_white: bool) -> bool {
    let tr = rank(target);
    let tf = file(target);

    for i in 0u8..64 {
        let p = board[i as usize];
        if p == 0 { continue; }
        if by_white && !is_white(p) { continue; }
        if !by_white && !is_black(p) { continue; }

        let pt = piece_type(p);
        let r = rank(i);
        let f = file(i);

        match pt {
            1 => { // pawn
                let fwd: i8 = if by_white { 1 } else { -1 };
                if (r as i8 + fwd) == tr as i8 && (f as i8 - tf as i8).abs() == 1 {
                    return true;
                }
            }
            2 => { // knight
                let dr = r.abs_diff(tr);
                let df = f.abs_diff(tf);
                if (dr == 2 && df == 1) || (dr == 1 && df == 2) { return true; }
            }
            3 | 5 => { // bishop / queen diagonals
                let dr = r.abs_diff(tr);
                let df = f.abs_diff(tf);
                if dr == df && dr > 0 {
                    let rs: i8 = if tr > r { 1 } else { -1 };
                    let fs: i8 = if tf > f { 1 } else { -1 };
                    if !sliding_blocked(board, r, f, rs, fs, dr) { return true; }
                }
                if pt != 5 { continue; }
                fallthrough_rook(board, r, f, tr, tf, &mut { false });
            }
            4 => { // rook
                if r == tr && f != tf {
                    if !rank_blocked(board, r, f, tf) { return true; }
                } else if f == tf && r != tr {
                    if !file_blocked(board, f, r, tr) { return true; }
                }
            }
            6 => { // king
                if r.abs_diff(tr) <= 1 && f.abs_diff(tf) <= 1 && (r != tr || f != tf) {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

fn sliding_blocked(board: &[u8; 64], r: u8, f: u8, rs: i8, fs: i8, steps: u8) -> bool {
    for s in 1..steps {
        let mr = (r as i8 + s as i8 * rs) as u8;
        let mf = (f as i8 + s as i8 * fs) as u8;
        if !is_empty(board[sq(mr, mf) as usize]) { return true; }
    }
    false
}

fn rank_blocked(board: &[u8; 64], r: u8, f: u8, tf: u8) -> bool {
    let (mn, mx) = if f < tf { (f, tf) } else { (tf, f) };
    (mn+1..mx).any(|mf| !is_empty(board[sq(r, mf) as usize]))
}

fn file_blocked(board: &[u8; 64], f: u8, r: u8, tr: u8) -> bool {
    let (mn, mx) = if r < tr { (r, tr) } else { (tr, r) };
    (mn+1..mx).any(|mr| !is_empty(board[sq(mr, f) as usize]))
}

fn fallthrough_rook(board: &[u8; 64], r: u8, f: u8, tr: u8, tf: u8, found: &mut bool) {
    if r == tr && f != tf && !rank_blocked(board, r, f, tf) { *found = true; }
    if f == tf && r != tr && !file_blocked(board, f, r, tr) { *found = true; }
}

pub fn is_in_check(board: &[u8; 64], white: bool) -> bool {
    is_attacked_by(board, find_king(board, white), !white)
}

// ---- Move application ----

pub fn apply_move(state: &GameState, mv: &Move) -> [u8; 64] {
    let mut b = state.board;
    let piece = b[mv.from as usize];
    let pt = piece_type(piece);

    // En passant capture
    if pt == 1 && mv.to == state.ep_square && state.ep_square != 255 {
        let cap_rank = if state.white_to_move { rank(mv.to) - 1 } else { rank(mv.to) + 1 };
        b[sq(cap_rank, file(mv.to)) as usize] = 0;
    }

    // Castling: move rook
    if pt == 6 && file(mv.from) == 4 {
        let r = rank(mv.from);
        match file(mv.to) {
            6 => { b[sq(r,5) as usize] = b[sq(r,7) as usize]; b[sq(r,7) as usize] = 0; }
            2 => { b[sq(r,3) as usize] = b[sq(r,0) as usize]; b[sq(r,0) as usize] = 0; }
            _ => {}
        }
    }

    b[mv.to as usize] = piece;
    b[mv.from as usize] = 0;

    // Promotion
    if pt == 1 && mv.promotion != 0 {
        b[mv.to as usize] = if state.white_to_move { mv.promotion } else { mv.promotion + 6 };
    }

    b
}

pub fn new_ep_square(mv: &Move, board: &[u8; 64]) -> u8 {
    let piece = board[mv.from as usize];
    if piece_type(piece) == 1 && rank(mv.from).abs_diff(rank(mv.to)) == 2 {
        sq((rank(mv.from) + rank(mv.to)) / 2, file(mv.from))
    } else {
        255
    }
}

pub fn new_castling(castling: u8, mv: &Move) -> u8 {
    let mut c = castling;
    // Determined from the from/to squares, not piece type, to handle captures of rooks
    if mv.from == 4  || mv.to == 4  { c &= 0b1100; } // white king moved
    if mv.from == 60 || mv.to == 60 { c &= 0b0011; } // black king moved
    if mv.from == 0  || mv.to == 0  { c &= 0b1101; } // a1 rook
    if mv.from == 7  || mv.to == 7  { c &= 0b1110; } // h1 rook
    if mv.from == 56 || mv.to == 56 { c &= 0b0111; } // a8 rook
    if mv.from == 63 || mv.to == 63 { c &= 0b1011; } // h8 rook
    c
}

// ---- Move validation ----

pub fn is_valid_move(state: &GameState, mv: &Move) -> bool {
    let piece = state.board[mv.from as usize];
    if piece == 0 { return false; }
    if state.white_to_move && !is_white(piece) { return false; }
    if !state.white_to_move && !is_black(piece) { return false; }

    let pt = piece_type(piece);
    let white = state.white_to_move;

    let piece_ok = match pt {
        1 => valid_pawn(state, mv),
        2 => valid_knight(state, mv),
        3 => valid_bishop(&state.board, mv, white),
        4 => valid_rook(&state.board, mv, white),
        5 => valid_bishop(&state.board, mv, white) || valid_rook(&state.board, mv, white),
        6 => valid_king(state, mv),
        _ => false,
    };

    if !piece_ok { return false; }

    // Move must not leave own king in check
    let new_board = apply_move(state, mv);
    !is_in_check(&new_board, white)
}

fn valid_pawn(state: &GameState, mv: &Move) -> bool {
    let white = state.white_to_move;
    let fwd: i8 = if white { 1 } else { -1 };
    let start_rank: u8 = if white { 1 } else { 6 };
    let promo_rank: u8 = if white { 7 } else { 0 };
    let fr = rank(mv.from);
    let ff = file(mv.from);
    let tr = rank(mv.to);
    let tf = file(mv.to);
    let df = ff.abs_diff(tf);

    // Single push
    if tf == ff && (fr as i8 + fwd) == tr as i8 && is_empty(state.board[mv.to as usize]) {
        return if tr == promo_rank { mv.promotion >= 2 } else { mv.promotion == 0 };
    }
    // Double push
    let mid = sq((fr as i8 + fwd) as u8, ff);
    if tf == ff && fr == start_rank && (fr as i8 + 2*fwd) == tr as i8
        && is_empty(state.board[mv.to as usize]) && is_empty(state.board[mid as usize]) {
        return mv.promotion == 0;
    }
    // Diagonal capture
    if df == 1 && (fr as i8 + fwd) == tr as i8 {
        let captures_enemy = is_enemy(state.board[mv.to as usize], white);
        let ep = state.ep_square != 255 && mv.to == state.ep_square;
        if captures_enemy || ep {
            return if tr == promo_rank { mv.promotion >= 2 } else { mv.promotion == 0 };
        }
    }
    false
}

fn valid_knight(state: &GameState, mv: &Move) -> bool {
    let dr = rank(mv.from).abs_diff(rank(mv.to));
    let df = file(mv.from).abs_diff(file(mv.to));
    ((dr == 2 && df == 1) || (dr == 1 && df == 2))
        && !is_friendly(state.board[mv.to as usize], state.white_to_move)
}

fn valid_bishop(board: &[u8; 64], mv: &Move, white: bool) -> bool {
    let dr = rank(mv.from).abs_diff(rank(mv.to));
    let df = file(mv.from).abs_diff(file(mv.to));
    if dr != df || dr == 0 { return false; }
    if is_friendly(board[mv.to as usize], white) { return false; }
    let rs: i8 = if rank(mv.to) > rank(mv.from) { 1 } else { -1 };
    let fs: i8 = if file(mv.to) > file(mv.from) { 1 } else { -1 };
    !sliding_blocked(board, rank(mv.from), file(mv.from), rs, fs, dr)
}

fn valid_rook(board: &[u8; 64], mv: &Move, white: bool) -> bool {
    if is_friendly(board[mv.to as usize], white) { return false; }
    let same_rank = rank(mv.from) == rank(mv.to);
    let same_file = file(mv.from) == file(mv.to);
    if !same_rank && !same_file { return false; }
    if same_rank { !rank_blocked(board, rank(mv.from), file(mv.from), file(mv.to)) }
    else { !file_blocked(board, file(mv.from), rank(mv.from), rank(mv.to)) }
}

fn valid_king(state: &GameState, mv: &Move) -> bool {
    let white = state.white_to_move;
    let dr = rank(mv.from).abs_diff(rank(mv.to));
    let df = file(mv.from).abs_diff(file(mv.to));

    // Castling
    if dr == 0 && df == 2 {
        let home_rank: u8 = if white { 0 } else { 7 };
        if rank(mv.from) != home_rank || file(mv.from) != 4 { return false; }
        if is_in_check(&state.board, white) { return false; }

        return match file(mv.to) {
            6 => { // kingside
                let right = if white { state.castling & 1 != 0 } else { state.castling & 4 != 0 };
                right
                    && is_empty(state.board[sq(home_rank, 5) as usize])
                    && is_empty(state.board[sq(home_rank, 6) as usize])
                    && !is_attacked_by(&state.board, sq(home_rank, 5), !white)
            }
            2 => { // queenside
                let right = if white { state.castling & 2 != 0 } else { state.castling & 8 != 0 };
                right
                    && is_empty(state.board[sq(home_rank, 1) as usize])
                    && is_empty(state.board[sq(home_rank, 2) as usize])
                    && is_empty(state.board[sq(home_rank, 3) as usize])
                    && !is_attacked_by(&state.board, sq(home_rank, 3), !white)
            }
            _ => false,
        };
    }

    // Normal king move
    dr <= 1 && df <= 1 && (dr + df) > 0
        && !is_friendly(state.board[mv.to as usize], white)
}

// ---- Transition validator (called by guest) ----

/// Panics if the transition is invalid. Used inside the zkVM guest.
pub fn validate_transition(old: &GameState, mv: &Move, new: &GameState) {
    assert!(is_valid_move(old, mv), "illegal move");

    let expected_board = apply_move(old, mv);
    assert_eq!(new.board, expected_board, "new board state mismatch");
    assert_eq!(new.white_to_move, !old.white_to_move, "turn did not flip");
    assert_eq!(new.castling, new_castling(old.castling, mv), "castling rights mismatch");
    assert_eq!(new.ep_square, new_ep_square(mv, &old.board), "en passant square mismatch");
}
