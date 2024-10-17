import { concat } from '@std/bytes/concat';
import { decodeVarint32, encodeVarint } from '@std/encoding/varint';

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
  const s = Math.floor(Math.sqrt(z));
  const t = z - s * s;
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
    return elegantPair(zigzagEncode(this.x), zigzagEncode(this.y));
  }

  /** Maps a natural number to a point (undoes `index`). */
  static fromIndex(i: number): Point {
    const [x, y] = elegantUnpair(i);
    return new Point(zigzagDecode(x), zigzagDecode(y));
  }

  /** Returns the adjacent point in the direction of the axis. */
  adjacent(axis: Axis, forward: boolean): Point {
    const [dx, dy] = Axis.unitVector(axis);
    if (forward) {
      return new Point(this.x + dx, this.y + dy);
    } else {
      return new Point(this.x - dx, this.y - dy);
    }
  }

  /** Tests if two possibly undefined points equal. */
  static equal(a?: Point, b?: Point): boolean {
    if (a == undefined) return b == undefined;
    return b != undefined && a.x == b.x && a.y == b.y;
  }

  /** Copies the point. */
  copy(): Point {
    return new Point(this.x, this.y);
  }

  /** Serializes the point to a buffer. */
  serialize(buf: Uint8Array[]) {
    buf.push(encodeVarint(this.index())[0]);
  }

  /** Deserializes a point from a buffer. */
  static deserialize(buf: Uint8Array, offset: number): [Point, number] {
    const [x, i] = decodeVarint32(buf, offset);
    return [Point.fromIndex(x), i];
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
  export function isEnding(move: Move): boolean {
    const kind = move.kind;
    return kind == MoveKind.Win || kind == MoveKind.Draw || kind == MoveKind.Resign;
  }

  /**
   * Serializes a move to a buffer.
   *
   * If `compact`, omits the pass after a 1-stone move.
   */
  export function serialize(move: Move, buf: Uint8Array[], compact: boolean) {
    switch (move.kind) {
      case MoveKind.Stone:
        for (const pos of move.pos) {
          const x = pos.index() + MOVE_STONE_OFFSET;
          buf.push(encodeVarint(x)[0]);
        }
        if (move.pos.length == 1 && !compact)
          buf.push(Uint8Array.of(MoveKind.Pass));
        break;
      case MoveKind.Win:
        buf.push(Uint8Array.of(move.kind));
        buf.push(encodeVarint(move.pos.index())[0]);
        break;
      case MoveKind.Resign:
        buf.push(Uint8Array.of(move.kind, move.stone));
        break;
      case MoveKind.Pass:
      case MoveKind.Draw:
        buf.push(Uint8Array.of(move.kind));
        break;
    }
  }

  /**
   * Deserializes a move from a buffer.
   *
   * If `first`, eagerly returns a 1-stone move.
   */
  export function deserialize(buf: Uint8Array, offset: number, first: boolean): [Move, number] {
    let [x, i] = decodeVarint32(buf, offset);
    if (x >= MOVE_STONE_OFFSET) {
      let pos: [Point] | [Point, Point];
      pos = [Point.fromIndex(x - MOVE_STONE_OFFSET)];
      if (first || i >= buf.length)
        return [{ kind: MoveKind.Stone, pos }, i];

      [x, i] = decodeVarint32(buf, i);
      if (x >= MOVE_STONE_OFFSET) {
        // We don't use `push` as it breaks the type system.
        pos = [pos[0], Point.fromIndex(x - MOVE_STONE_OFFSET)];
      } else if (x != MoveKind.Pass) {
        throw new RangeError('expected stone or pass');
      }
      return [{ kind: MoveKind.Stone, pos }, i];
    }

    switch (x) {
      case MoveKind.Win:
        let pos;
        [pos, i] = Point.deserialize(buf, i);
        return [{ kind: MoveKind.Win, pos }, i];
      case MoveKind.Resign:
        if (i >= buf.length)
          throw new RangeError('expected stone');
        const stone = Stone.fromNumber(buf[i++]);
        return [{ kind: MoveKind.Resign, stone }, i];
      case MoveKind.Pass:
      case MoveKind.Draw:
        return [{ kind: x }, i];
      default:
        throw new RangeError('unknown move kind');
    }
  }
}

/** A Connect6 game record on an infinite board. */
export class Record {
  private map: Map<number, Stone> = new Map();
  private mov: Move[] = [];
  private idx: number = 0;

  /**
   * Assigns to this record the fields of another.
   *
   * The other record will be cleared.
   */
  assign(other: Record) {
    this.map = other.map;
    this.mov = other.mov;
    this.idx = other.idx;
    other.clear();
  }

  /** Clears the record. */
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
    const prev = this.prevMove();
    return prev != undefined && Move.isEnding(prev);
  }

  /** Returns the stone to play at the given move index. */
  static turnAt(index: number) {
    return index % 2 == 0 ? Stone.Black : Stone.White;
  }

  /** Returns the current stone to play. */
  turn(): Stone {
    return Record.turnAt(this.idx);
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
      for (const pos of move.pos)
        if (this.map.has(pos.index())) return false;

      const stone = this.turn();
      for (const pos of move.pos)
        this.map.set(pos.index(), stone);
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
    const prev = this.prevMove();
    if (!prev) return;
    this.idx--;

    if (prev.kind == MoveKind.Stone)
      for (const pos of prev.pos)
        this.map.delete(pos.index());
    return prev;
  }

  /** Redoes the next move (if any). */
  redoMove(): Move | undefined {
    const next = this.nextMove();
    if (!next) return;
    this.idx++;

    const stone = this.turn();
    if (next.kind == MoveKind.Stone)
      for (const pos of next.pos)
        this.map.set(pos.index(), stone);
    return next;
  }

  /** Jumps to the given move index by undoing or redoing moves. */
  jump(index: number): boolean {
    if (index > this.mov.length) return false;
    const diff = this.idx - index;
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
    const stone = this.stoneAt(pos);
    if (!stone) return [{ start: pos, end: pos }, 0];

    let len = 1;
    const scan = (cur: Point, forward: boolean) => {
      let next = cur.adjacent(axis, forward);
      while (this.stoneAt(next) == stone) {
        len += 1;
        cur = next;
        next = cur.adjacent(axis, forward);
      }
      return cur;
    };

    const start = scan(pos, false), end = scan(pos, true);
    return [{ start, end }, len];
  }

  /** Searches for a win row through the point. */
  findWinRow(pos: Point): Row | undefined {
    if (!this.stoneAt(pos)) return;
    for (const axis of Axis.VALUES) {
      const [row, len] = this.scanRow(pos, axis);
      if (len >= 6) return row;
    }
  }

  /**
   * Serializes the record to a buffer.
   *
   * If `all`, includes all moves prefixed with the current move index.
   */
  serialize(all: boolean): Uint8Array {
    const buf = all ? [encodeVarint(this.idx)[0]] : [];
    const end = all ? this.mov.length : this.idx;
    for (let i = 0; i < end; i++)
      Move.serialize(this.mov[i], buf, i == 0);
    return concat(buf);
  }

  /** Deserializes a record from a buffer. */
  static deserialize(buf: Uint8Array, offset: number, all: boolean): Record {
    const rec = new Record();
    let index, i = offset;
    if (all) [index, i] = decodeVarint32(buf, i);

    while (i < buf.length) {
      let move;
      [move, i] = Move.deserialize(buf, i, !rec.hasPast());
      if (!rec.makeMove(move))
        throw new RangeError('move failed');
    }

    if (index != undefined && !rec.jump(index))
      throw new RangeError('move index exceeds total number of moves');

    return rec;
  }
}
