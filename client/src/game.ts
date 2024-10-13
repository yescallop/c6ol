import * as varint from 'varint';

/** An axis on the board. */
export enum Axis {
  /** The horizontal axis, with a unit vector of `(1, 0)`. */
  Horizontal,
  /** The diagonal axis, with a unit vector of `(1, 1)`. */
  Diagonal,
  /** The vertical axis, with a unit vector of `(0, 1)`. */
  Vertical,
  /** The anti-diagonal axis, with a unit vector of `(1, -1)`. */
  AntiDiagonal,
}

export namespace Axis {
  /** All axes on the board. */
  export const VALUES = [Axis.Horizontal, Axis.Diagonal, Axis.Vertical, Axis.AntiDiagonal];

  /** Returns the unit vector in the direction of the axis. */
  export function unitVector(axis: Axis): [number, number] {
    const VECTORS: [number, number][] = [[1, 0], [1, 1], [0, 1], [1, -1]];
    return VECTORS[axis];
  }
}

/** Maps an integer to a natural number. */
function zigzagEncode(n: number): number {
  return n >= 0 ? 2 * n : -2 * n - 1;
}

/** Maps a natural number to an integer (undoes `zigzagEncode`). */
function zigzagDecode(n: number): number {
  return n % 2 == 0 ? n / 2 : -(n + 1) / 2;
}

/** Maps two natural numbers to one. */
function elegantPair(x: number, y: number): number {
  return x < y ? y * y + x : x * x + x + y;
}

/** Maps one natural number to two (undoes `elegantPair`).  */
function elegantUnpair(z: number): [number, number] {
  let s = Math.floor(Math.sqrt(z));
  let t = z - s * s;
  return t < s ? [t, s] : [s, t - s];
}

/** A 2D point with integer coordinates. */
export class Point {
  x: number;
  y: number;

  /** Creates a point with the given coordinates. */
  constructor(x: number, y: number) {
    this.x = x;
    this.y = y;
  }

  /** Maps the point to a natural number. */
  index(): number {
    let x = zigzagEncode(this.x), y = zigzagEncode(this.y);
    return elegantPair(x, y);
  }

  /** Maps a natural number to a point (undoes `index`). */
  static fromIndex(i: number): Point {
    let [x, y] = elegantUnpair(i);
    return new Point(zigzagDecode(x), zigzagDecode(y));
  }

  /** Returns the adjacent point in the direction of the axis. */
  adjacent(axis: Axis, forward: boolean): Point {
    let [dx, dy] = Axis.unitVector(axis);
    if (forward) {
      return new Point(this.x + dx, this.y + dy);
    } else {
      return new Point(this.x - dx, this.y - dy);
    }
  }

  /** Tests if two possibly undefined points equal. */
  static equal(a: Point | undefined, b: Point | undefined): boolean {
    if (a == undefined) return b == undefined;
    return b != undefined && a.x == b.x && a.y == b.y;
  }

  /** Copies the point. */
  copy(): Point {
    return new Point(this.x, this.y);
  }
}

/** A contiguous row of stones on the board. */
export interface Row {
  start: Point;
  end: Point;
}

/** A stone on the board, either black or white. */
export enum Stone {
  // 0 would be falsy.
  Black = 1,
  White = 2,
}

export namespace Stone {
  /** Creates a stone from a number. */
  export function fromNumber(n: number): Stone {
    if (n != 1 && n != 2) throw new RangeError('stone out of range');
    return n;
  }

  /** Returns the opposite stone. */
  export function opposite(stone: Stone): Stone {
    return stone ^ 3;
  }
}

/** Allows room for extension. Equals (2^7-11^2). */
const MOVE_STONE_OFFSET = 7;

export enum MoveKind {
  Stone = -1,
  Pass = 0,
  Win = 1,
  Draw = 2,
  Resign = 3,
}

/** A move made by one player or both players. */
export type Move = {
  // One or two stones placed on the board by the current player.
  kind: MoveKind.Stone;
  pos: [Point] | [Point, Point];
} | {
  // A pass made by the current player.
  kind: MoveKind.Pass;
} | {
  // A win claimed by any player.
  kind: MoveKind.Win;
  pos: Point;
} | {
  // A draw agreed by both players.
  kind: MoveKind.Draw;
} | {
  // A resignation indicated by the player with the given stone.
  kind: MoveKind.Resign;
  stone: Stone;
};

export namespace Move {
  /** Tests if the move is an ending move. */
  export function isEndingMove(move: Move): boolean {
    let kind = move.kind;
    return kind == MoveKind.Win || kind == MoveKind.Draw || kind == MoveKind.Resign;
  }

  /** Serializes a move to a byte array. */
  export function serialize(move: Move, first: boolean): Uint8Array {
    let buf: number[] = [];
    switch (move.kind) {
      case MoveKind.Stone:
        for (let p of move.pos) {
          let x = p.index() + MOVE_STONE_OFFSET;
          varint.encode(x, buf, buf.length);
        }
        if (move.pos.length == 1 && !first)
          buf.push(MoveKind.Pass);
        break;
      case MoveKind.Win:
        buf.push(MoveKind.Win);
        varint.encode(move.pos.index(), buf, buf.length);
        break;
      case MoveKind.Resign:
        buf.push(MoveKind.Resign, move.stone);
        break;
      case MoveKind.Pass:
      case MoveKind.Draw:
        buf.push(move.kind);
        break;
    }
    return new Uint8Array(buf);
  }

  /** Deserializes a move from a byte array. */
  export function deserialize(buf: Uint8Array, first: boolean): [Move, number] {
    let x = varint.decode(buf);
    let n = varint.decode.bytes!;

    if (x >= MOVE_STONE_OFFSET) {
      let pos: [Point] | [Point, Point];
      pos = [Point.fromIndex(x - MOVE_STONE_OFFSET)];
      if (first) return [{ kind: MoveKind.Stone, pos }, n];

      x = varint.decode(buf, n);
      n += varint.decode.bytes!;

      if (x >= MOVE_STONE_OFFSET) {
        // We don't use `push` as it breaks the type system.
        pos = [pos[0], Point.fromIndex(x - MOVE_STONE_OFFSET)];
      } else if (x != MoveKind.Pass) {
        throw new RangeError('expected stone or pass');
      }
      return [{ kind: MoveKind.Stone, pos }, n];
    }

    switch (x) {
      case MoveKind.Win:
        x = varint.decode(buf, n);
        n += varint.decode.bytes!;

        let pos = Point.fromIndex(x);
        return [{ kind: MoveKind.Win, pos }, n];
      case MoveKind.Resign:
        if (n == buf.length)
          throw new RangeError('expected stone');
        let stone = Stone.fromNumber(buf[n++]);
        return [{ kind: MoveKind.Resign, stone }, n];
      case MoveKind.Pass:
      case MoveKind.Draw:
        return [{ kind: x }, n];
      default:
        throw new RangeError('unknown move kind');
    }
  }
}

/** A Connect6 game on an infinite board. */
export class Game {
  private map: Map<number, Stone> = new Map();
  private mov: Move[] = [];
  private idx: number = 0;

  /**
   * Assigns to this game the moves and the move index of another.
   *
   * The other game will be cleared.
   */
  assign(other: Game) {
    this.map = other.map;
    this.mov = other.mov;
    this.idx = other.idx;
    other.clear();
  }

  /** Clears the game. */
  clear() {
    this.map = new Map();
    this.mov = [];
    this.idx = 0;
  }

  /** Returns an array of all moves, in the past or in the future. */
  moves(): Move[] {
    return this.mov;
  }

  /** Returns the current move index. */
  moveIndex(): number {
    return this.idx;
  }

  /** Returns the previous move (if any). */
  prevMove(): Move | undefined {
    return this.idx > 0 ? this.mov[this.idx - 1] : undefined;
  }

  /** Returns the next move (if any). */
  nextMove(): Move | undefined {
    return this.idx < this.mov.length ? this.mov[this.idx] : undefined;
  }

  /** Tests if there is any move in the past. */
  hasPast(): boolean {
    return this.idx > 0;
  }

  /** Tests if there is any move in the future. */
  hasFuture(): boolean {
    return this.idx < this.mov.length;
  }

  /** Tests if the game is ended. */
  isEnded(): boolean {
    let prev = this.prevMove();
    return prev != undefined && Move.isEndingMove(prev);
  }

  /** Returns the stone to play at the given move index. */
  static turnAt(index: number) {
    return index % 2 == 0 ? Stone.Black : Stone.White;
  }

  /** Returns the current stone to play. */
  turn(): Stone {
    return Game.turnAt(this.idx);
  }

  /** Returns the stone at the given position (if any). */
  stoneAt(pos: Point): Stone | undefined {
    return this.map.get(pos.index());
  }

  /**
   * Makes a move, clearing moves in the future.
   *
   * Returns whether the move succeeded.
   */
  makeMove(move: Move): boolean {
    if (this.isEnded()) return false;

    if (move.kind == MoveKind.Stone) {
      if (this.idx == 0 && move.pos.length != 1)
        return false;

      let stone = this.turn();
      for (let p of move.pos) {
        let i = p.index();
        if (this.map.has(i)) return false;
        this.map.set(i, stone);
      }
    } else if (move.kind == MoveKind.Win) {
      if (!this.findWinRow(move.pos))
        return false;
    }

    this.mov.length = this.idx;
    this.mov.push(move);
    this.idx++;
    return true;
  }

  /** Undoes the previous move (if any). */
  undoMove(): Move | undefined {
    let prev = this.prevMove();
    if (!prev) return;
    this.idx--;

    if (prev.kind == MoveKind.Stone)
      for (let p of prev.pos) this.map.delete(p.index());
    return prev;
  }

  /** Redoes the next move (if any). */
  redoMove(): Move | undefined {
    let next = this.nextMove();
    if (!next) return;
    this.idx++;

    let stone = this.turn();
    if (next.kind == MoveKind.Stone)
      for (let p of next.pos) this.map.set(p.index(), stone);
    return next;
  }

  /** Jumps to the given move index by undoing or redoing moves. */
  jump(index: number): boolean {
    if (index > this.mov.length) return false;
    let diff = this.idx - index;
    if (diff > 0) {
      for (let i = 0; i < diff; i++)
        this.undoMove();
    } else {
      for (let i = 0; i < -diff; i++)
        this.redoMove();
    }
    return true;
  }

  /** Scans the row through a position in the direction of the axis. */
  scanRow(pos: Point, axis: Axis): [Row, number] {
    let stone = this.stoneAt(pos);
    if (!stone) return [{ start: pos, end: pos }, 0];

    let len = 1;
    let scan = (cur: Point, forward: boolean) => {
      let next = cur.adjacent(axis, forward);
      while (this.stoneAt(next) == stone) {
        len += 1;
        cur = next;
        next = cur.adjacent(axis, forward);
      }
      return cur;
    };

    let start = scan(pos, false);
    let end = scan(pos, true);
    return [{ start, end }, len];
  }

  /** Searches for a win row through the point. */
  findWinRow(pos: Point): Row | undefined {
    if (!this.stoneAt(pos)) return;
    for (let axis of Axis.VALUES) {
      let [row, len] = this.scanRow(pos, axis);
      if (len >= 6) return row;
    }
  }

  /**
   * Serializes the game to a byte array.
   *
   * If `all`, includes all moves prefixed with the current move index.
   */
  serialize(all = false): Uint8Array {
    let buf = all ? varint.encode(this.idx) : [];
    let end = all ? this.mov.length : this.idx;
    for (let i = 0; i < end; i++)
      buf.push(...Move.serialize(this.mov[i], i == 0));
    return new Uint8Array(buf);
  }

  /** Deserializes a game from a byte array. */
  static deserialize(buf: Uint8Array, all = false): Game {
    let game = new Game(), index;
    if (all) {
      index = varint.decode(buf);
      buf = buf.subarray(varint.decode.bytes!);
    }

    while (buf.length > 0) {
      let [move, n] = Move.deserialize(buf, !game.hasPast());
      if (!game.makeMove(move))
        throw new RangeError('move failed');
      buf = buf.subarray(n);
    }

    if (index != undefined && !game.jump(index))
      throw new RangeError('move index exceeds total number of moves');

    return game;
  }
}