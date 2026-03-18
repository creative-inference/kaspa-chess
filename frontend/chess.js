// Kaspa Chess — Client Logic
// Mirrors the Noir circuit encoding exactly.
//
// Piece encoding:
//   0=empty, 1=wP, 2=wN, 3=wB, 4=wR, 5=wQ, 6=wK
//             7=bP, 8=bN, 9=bB, 10=bR, 11=bQ, 12=bK
//
// Squares: 0=a1..63=h8

const PIECE_UNICODE = {
    1: '♙', 2: '♘', 3: '♗', 4: '♖', 5: '♕', 6: '♔',
    7: '♟', 8: '♞', 9: '♝', 10: '♜', 11: '♛', 12: '♚',
};

const FILES = 'abcdefgh';

function rank(sq) { return Math.floor(sq / 8); }
function file(sq) { return sq % 8; }
function toSq(r, f) { return r * 8 + f; }
function isWhite(p) { return p >= 1 && p <= 6; }
function isBlack(p) { return p >= 7 && p <= 12; }
function pieceType(p) { return p > 6 ? p - 6 : p; }
function isEmpty(p) { return p === 0; }

// Initial board position
function initialBoard() {
    const b = new Uint8Array(64);
    // White pieces (rank 0)
    b[0]=4; b[1]=2; b[2]=3; b[3]=5; b[4]=6; b[5]=3; b[6]=2; b[7]=4;
    for (let f=0;f<8;f++) b[8+f]=1;   // white pawns rank 1
    // Black pieces (rank 7)
    b[56]=10; b[57]=8; b[58]=9; b[59]=11; b[60]=12; b[61]=9; b[62]=8; b[63]=10;
    for (let f=0;f<8;f++) b[48+f]=7;  // black pawns rank 6
    return b;
}

const INITIAL_STATE = {
    board: initialBoard(),
    castling: 0b1111,  // all rights
    ep: 255,
    whiteToMove: true,
    moveHistory: [],
};

// ---- Move generation (mirrors circuit logic) ----

function slidingAttacks(board, from, dirs) {
    const attacks = [];
    for (const [dr, df] of dirs) {
        let r = rank(from) + dr;
        let f = file(from) + df;
        while (r >= 0 && r < 8 && f >= 0 && f < 8) {
            const sq = toSq(r, f);
            attacks.push(sq);
            if (!isEmpty(board[sq])) break;
            r += dr; f += df;
        }
    }
    return attacks;
}

function isAttackedBy(board, sq, byWhite) {
    for (let i = 0; i < 64; i++) {
        const p = board[i];
        if (!p) continue;
        if (byWhite ? !isWhite(p) : !isBlack(p)) continue;
        const pt = pieceType(p);
        const r = rank(i), f = file(i), tr = rank(sq), tf = file(sq);

        if (pt === 1) { // pawn
            if (byWhite && r + 1 === tr && Math.abs(f - tf) === 1) return true;
            if (!byWhite && r - 1 === tr && Math.abs(f - tf) === 1) return true;
        }
        if (pt === 2) { // knight
            const dr = Math.abs(r-tr), df = Math.abs(f-tf);
            if ((dr===2&&df===1)||(dr===1&&df===2)) return true;
        }
        if (pt === 3 || pt === 5) { // bishop/queen diagonals
            if (Math.abs(r-tr) === Math.abs(f-tf) && r !== tr) {
                const rs = tr>r?1:-1, fs = tf>f?1:-1;
                let blocked = false;
                for (let s=1; s<Math.abs(r-tr); s++)
                    if (!isEmpty(board[toSq(r+s*rs, f+s*fs)])) { blocked=true; break; }
                if (!blocked) return true;
            }
        }
        if (pt === 4 || pt === 5) { // rook/queen straights
            if (r===tr && f!==tf) {
                let blocked = false;
                const mn=Math.min(f,tf), mx=Math.max(f,tf);
                for (let mf=mn+1;mf<mx;mf++) if (!isEmpty(board[toSq(r,mf)])) { blocked=true; break; }
                if (!blocked) return true;
            }
            if (f===tf && r!==tr) {
                let blocked = false;
                const mn=Math.min(r,tr), mx=Math.max(r,tr);
                for (let mr=mn+1;mr<mx;mr++) if (!isEmpty(board[toSq(mr,f)])) { blocked=true; break; }
                if (!blocked) return true;
            }
        }
        if (pt === 6) { // king
            if (Math.abs(r-tr)<=1 && Math.abs(f-tf)<=1 && (r!==tr||f!==tf)) return true;
        }
    }
    return false;
}

function findKing(board, white) {
    const k = white ? 6 : 12;
    return board.indexOf(k);
}

function isInCheck(board, white) {
    return isAttackedBy(board, findKing(board, white), !white);
}

function applyMove(state, from, to, promo) {
    const b = new Uint8Array(state.board);
    const piece = b[from];
    const pt = pieceType(piece);

    // En passant
    if (pt === 1 && to === state.ep && state.ep !== 255) {
        const capR = state.whiteToMove ? rank(to)-1 : rank(to)+1;
        b[toSq(capR, file(to))] = 0;
    }

    // Castling — move rook
    if (pt === 6 && Math.abs(file(from)-file(to)) === 2) {
        const r = rank(from);
        if (file(to) === 6) { b[toSq(r,5)]=b[toSq(r,7)]; b[toSq(r,7)]=0; }
        if (file(to) === 2) { b[toSq(r,3)]=b[toSq(r,0)]; b[toSq(r,0)]=0; }
    }

    b[to] = piece;
    b[from] = 0;

    if (pt === 1 && promo) {
        b[to] = state.whiteToMove ? promo : promo + 6;
    }

    return b;
}

function getLegalMoves(state, from) {
    const piece = state.board[from];
    if (!piece) return [];
    if (state.whiteToMove && !isWhite(piece)) return [];
    if (!state.whiteToMove && !isBlack(piece)) return [];

    const pt = pieceType(piece);
    const r = rank(from), f = file(from);
    const candidates = [];

    if (pt === 1) { // pawn
        const fwd = state.whiteToMove ? 1 : -1;
        const startR = state.whiteToMove ? 1 : 6;
        const tr1 = r + fwd;
        if (tr1 >= 0 && tr1 < 8) {
            if (isEmpty(state.board[toSq(tr1,f)])) candidates.push(toSq(tr1,f));
            if (r === startR && isEmpty(state.board[toSq(tr1,f)]) && isEmpty(state.board[toSq(r+2*fwd,f)]))
                candidates.push(toSq(r+2*fwd,f));
            for (const df of [-1,1]) {
                const tf = f+df;
                if (tf<0||tf>7) continue;
                const ts = toSq(tr1,tf);
                const target = state.board[ts];
                if ((state.whiteToMove ? isBlack(target) : isWhite(target)) || ts===state.ep)
                    candidates.push(ts);
            }
        }
    }
    if (pt === 2) { // knight
        for (const [dr,df] of [[-2,-1],[-2,1],[-1,-2],[-1,2],[1,-2],[1,2],[2,-1],[2,1]]) {
            const tr=r+dr, tf=f+df;
            if (tr>=0&&tr<8&&tf>=0&&tf<8) candidates.push(toSq(tr,tf));
        }
    }
    if (pt === 3 || pt === 5) // bishop / queen
        slidingAttacks(state.board, from, [[-1,-1],[-1,1],[1,-1],[1,1]]).forEach(s=>candidates.push(s));
    if (pt === 4 || pt === 5) // rook / queen
        slidingAttacks(state.board, from, [[-1,0],[1,0],[0,-1],[0,1]]).forEach(s=>candidates.push(s));

    if (pt === 6) { // king
        for (const [dr,df] of [[-1,-1],[-1,0],[-1,1],[0,-1],[0,1],[1,-1],[1,0],[1,1]]) {
            const tr=r+dr, tf=f+df;
            if (tr>=0&&tr<8&&tf>=0&&tf<8) candidates.push(toSq(tr,tf));
        }
        // Castling
        const homeR = state.whiteToMove ? 0 : 7;
        if (r === homeR && f === 4) {
            if ((state.castling & (state.whiteToMove?1:4)) && isEmpty(state.board[toSq(homeR,5)]) && isEmpty(state.board[toSq(homeR,6)]) && !isInCheck(state.board, state.whiteToMove) && !isAttackedBy(state.board, toSq(homeR,5), !state.whiteToMove))
                candidates.push(toSq(homeR,6));
            if ((state.castling & (state.whiteToMove?2:8)) && isEmpty(state.board[toSq(homeR,1)]) && isEmpty(state.board[toSq(homeR,2)]) && isEmpty(state.board[toSq(homeR,3)]) && !isInCheck(state.board, state.whiteToMove) && !isAttackedBy(state.board, toSq(homeR,3), !state.whiteToMove))
                candidates.push(toSq(homeR,2));
        }
    }

    // Filter out friendly captures and moves that leave king in check
    return candidates.filter(to => {
        const target = state.board[to];
        if (state.whiteToMove && isWhite(target)) return false;
        if (!state.whiteToMove && isBlack(target)) return false;
        const nb = applyMove(state, from, to, 0);
        return !isInCheck(nb, state.whiteToMove);
    });
}

function updateCastling(castling, from, to) {
    let c = castling;
    const piece = game.board[from]; const pt = pieceType(piece);
    if (pt === 6) { c = isWhite(piece) ? c & 0b1100 : c & 0b0011; }
    if (from===0||to===0) c &= 0b1101;
    if (from===7||to===7) c &= 0b1110;
    if (from===56||to===56) c &= 0b0111;
    if (from===63||to===63) c &= 0b1011;
    return c;
}

// ---- Game state ----

let game = {
    board: initialBoard(),
    castling: 0b1111,
    ep: 255,
    whiteToMove: true,
    moveHistory: [],
    capturedWhite: [],
    capturedBlack: [],
    selected: null,
    legalMoves: [],
    lastFrom: null,
    lastTo: null,
};

// ---- UI ----

function renderBoard() {
    const boardEl = document.getElementById('board');
    boardEl.innerHTML = '';
    for (let row = 7; row >= 0; row--) {
        for (let col = 0; col < 8; col++) {
            const sq = toSq(row, col);
            const div = document.createElement('div');
            div.className = 'square ' + ((row+col)%2===0 ? 'dark' : 'light');
            if (sq === game.selected) div.classList.add('selected');
            if (game.legalMoves.includes(sq)) div.classList.add('legal-move');
            if (sq === game.lastFrom || sq === game.lastTo) div.classList.add('last-move');

            const p = game.board[sq];
            if (p) div.textContent = PIECE_UNICODE[p];

            // Coords
            if (col === 0) { const c=document.createElement('span'); c.className='coord rank'; c.textContent=row+1; div.appendChild(c); }
            if (row === 0) { const c=document.createElement('span'); c.className='coord file'; c.textContent=FILES[col]; div.appendChild(c); }

            div.addEventListener('click', () => onSquareClick(sq));
            boardEl.appendChild(div);
        }
    }

    // Status
    const inCheck = isInCheck(game.board, game.whiteToMove);
    document.getElementById('turn').textContent = game.whiteToMove ? 'White' : 'Black';
    document.getElementById('check-status').textContent = inCheck ? ' — CHECK' : '';

    // Captured
    document.getElementById('captured-white').textContent = game.capturedWhite.map(p=>PIECE_UNICODE[p]).join('');
    document.getElementById('captured-black').textContent = game.capturedBlack.map(p=>PIECE_UNICODE[p]).join('');
}

function onSquareClick(sq) {
    if (game.legalMoves.includes(sq)) {
        executeMove(game.selected, sq);
        return;
    }
    const p = game.board[sq];
    if (p && (game.whiteToMove ? isWhite(p) : isBlack(p))) {
        game.selected = sq;
        game.legalMoves = getLegalMoves(game, sq);
    } else {
        game.selected = null;
        game.legalMoves = [];
    }
    renderBoard();
}

function executeMove(from, to, promo) {
    const piece = game.board[from];
    const pt = pieceType(piece);
    const promoRank = game.whiteToMove ? 7 : 0;

    // Promotion check
    if (pt === 1 && rank(to) === promoRank && !promo) {
        showPromoModal(from, to);
        return;
    }

    const target = game.board[to];
    if (target) {
        if (isWhite(target)) game.capturedWhite.push(target);
        else game.capturedBlack.push(target);
    }

    const newBoard = applyMove(game, from, to, promo || 0);
    const newEp = (pt===1 && Math.abs(rank(from)-rank(to))===2) ? toSq((rank(from)+rank(to))/2, file(from)) : 255;

    // Record move in SAN (simplified)
    const san = moveToSan(from, to, promo);
    addMoveToHistory(san, game.whiteToMove);

    game.board = newBoard;
    game.castling = updateCastling(game.castling, from, to);
    game.ep = newEp;
    game.lastFrom = from;
    game.lastTo = to;
    game.whiteToMove = !game.whiteToMove;
    game.selected = null;
    game.legalMoves = [];

    // TODO: wire Kaspa RPC — submit proof and new state to L1
    // submitProofToKaspa(oldState, move, newState, proof);

    renderBoard();
}

function moveToSan(from, to, promo) {
    const p = game.board[from];
    const pt = pieceType(p);
    const ptNames = ['','','N','B','R','Q','K'];
    let san = pt > 1 ? ptNames[pt] : '';
    san += FILES[file(from)] + (rank(from)+1);
    san += game.board[to] ? 'x' : '-';
    san += FILES[file(to)] + (rank(to)+1);
    if (promo) san += '=' + ['','','N','B','R','Q'][promo];
    return san;
}

function addMoveToHistory(san, wasWhite) {
    const hist = document.getElementById('move-history');
    if (wasWhite) {
        const moveNum = Math.floor(game.moveHistory.length / 2) + 1;
        const div = document.createElement('div');
        div.className = 'move-pair';
        div.id = 'move-' + moveNum;
        div.innerHTML = `<span class="move-num">${moveNum}.</span><span class="move-san">${san}</span>`;
        hist.appendChild(div);
    } else {
        const moveNum = Math.ceil(game.moveHistory.length / 2);
        const pair = document.getElementById('move-' + moveNum);
        if (pair) {
            const span = document.createElement('span');
            span.className = 'move-san';
            span.textContent = san;
            pair.appendChild(span);
        }
    }
    game.moveHistory.push(san);
    hist.scrollTop = hist.scrollHeight;
}

// Promotion modal
function showPromoModal(from, to) {
    const modal = document.getElementById('promo-modal');
    const isW = game.whiteToMove;
    document.getElementById('promo-q').textContent = isW ? '♕' : '♛';
    document.getElementById('promo-r').textContent = isW ? '♖' : '♜';
    document.getElementById('promo-b').textContent = isW ? '♗' : '♝';
    document.getElementById('promo-n').textContent = isW ? '♘' : '♞';
    modal.classList.add('active');
    modal.dataset.from = from;
    modal.dataset.to = to;
}

document.addEventListener('DOMContentLoaded', () => {
    document.querySelectorAll('.promo-btn').forEach(btn => {
        btn.addEventListener('click', () => {
            const modal = document.getElementById('promo-modal');
            const promo = parseInt(btn.dataset.piece);
            const from = parseInt(modal.dataset.from);
            const to = parseInt(modal.dataset.to);
            modal.classList.remove('active');
            executeMove(from, to, promo);
        });
    });

    document.getElementById('btn-new').addEventListener('click', () => {
        game.board = initialBoard();
        game.castling = 0b1111;
        game.ep = 255;
        game.whiteToMove = true;
        game.moveHistory = [];
        game.capturedWhite = [];
        game.capturedBlack = [];
        game.selected = null;
        game.legalMoves = [];
        game.lastFrom = null;
        game.lastTo = null;
        document.getElementById('move-history').innerHTML = '';
        renderBoard();
    });

    renderBoard();
});

// TODO: wire Noir.js proving
// async function generateMoveProof(oldState, move, newState) {
//     const { Noir } = await import('@noir-lang/noir_js');
//     const { UltraHonkBackend } = await import('@aztec/bb.js');
//     const circuit = await fetch('/circuit/kaspa_chess.json').then(r => r.json());
//     const backend = new UltraHonkBackend(circuit.bytecode);
//     const noir = new Noir(circuit);
//     const { witness } = await noir.execute({ old_state: oldState, mv: move, new_state: newState });
//     return backend.generateProof(witness);
// }

// TODO: wire Kaspa RPC
// async function submitProofToKaspa(oldState, move, newState, proof) {
//     const newStateBytes = encodeState(newState);
//     const proofBytes = proof.proof;
//     const moveBytes = new Uint8Array([move.from, move.to, move.promotion]);
//     // Build UTXO spend transaction with witness: [newStateBytes, proofBytes, moveBytes]
//     // Submit via Kaspa RPC
// }
