import * as varint from 'varint';

function zigzagEncode(x: number): number {
  return ((x << 1) ^ (x >> 31)) >>> 0;
}

function zigzagDecode(x: number): number {
  return ((x >>> 1) ^ -(x & 1)) >> 0;
}

function interleave(x: number, y: number): number {
  function scatter(x: number): number {
    x = (x | (x << 8)) & 0x00ff00ff;
    x = (x | (x << 4)) & 0x0f0f0f0f;
    x = (x | (x << 2)) & 0x33333333;
    return (x | (x << 1)) & 0x55555555;
  }
  return scatter(x) | (scatter(y) << 1);
}

function deinterleave(x: number): [number, number] {
  function gather(x: number): number {
    x &= 0x55555555;
    x = (x | (x >>> 1)) & 0x33333333;
    x = (x | (x >>> 2)) & 0x0f0f0f0f;
    x = (x | (x >>> 4)) & 0x00ff00ff;
    return (x | (x >>> 8)) & 0x0000ffff;
  }
  return [gather(x), gather(x >>> 1)];
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

  /** Maps the point to an unsigned integer. */
  index(): number {
    let x = zigzagEncode(this.x), y = zigzagEncode(this.y);
    return interleave(x, y);
  }

  /** Creates a point from an unsigned integer. */
  static fromIndex(i: number): Point {
    let [x, y] = deinterleave(i);
    return new Point(zigzagDecode(x), zigzagDecode(y));
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

  /** Serializes the point to an array of bytes. */
  serialize(): Uint8Array {
    return new Uint8Array(varint.encode(this.index()));
  }
}

/** A stone on the board, either black or white. */
export enum Stone {
  // 0 would be falsy.
  Black = 1,
  White = 2,
}

export namespace Stone {
  /** Returns the opposite stone. */
  export function opposite(stone: Stone): Stone {
    return stone ^ 3;
  }
}

/** A move on the board, namely a (position, stone) pair. */
export interface Move {
  pos: Point;
  stone: Stone;
}

/** An infinite Connect6 board. */
export class Board {
  private map: Map<number, Stone>;
  private moves: Move[];
  private idx: number;

  /** Creates an empty board. */
  constructor() {
    this.map = new Map();
    this.moves = [];
    this.idx = 0;
  }

  /**
   * Returns the total number of moves, on or off the board,
   * in the past or in the future.
   */
  total(): number {
    return this.moves.length;
  }

  /** Returns the current move index. */
  index(): number {
    return this.idx;
  }

  /** Tests if the board is empty. */
  empty(): boolean {
    return this.idx == 0;
  }

  /** Returns the stone at a point. */
  get(point: Point): Stone | undefined {
    return this.map.get(point.index());
  }

  /** Returns an array of moves in the past. */
  pastMoves(): Move[] {
    return this.moves.slice(0, this.idx);
  }

  /** Makes a move at a point, clearing moves in the future. */
  set(pos: Point, stone: Stone): boolean {
    let i = pos.index();
    if (this.map.has(i)) return false;
    this.map.set(i, stone);

    this.moves.splice(this.idx);
    this.moves.push({ pos, stone });
    this.idx++;
    return true;
  }

  /** Undoes the last move (if any). */
  unset(): Move | undefined {
    if (this.idx == 0) return;
    this.idx--;
    let last = this.moves[this.idx];

    this.map.delete(last.pos.index());
    return last;
  }

  /** Redoes the next move (if any). */
  reset(): Move | undefined {
    if (this.idx >= this.moves.length) return;
    let next = this.moves[this.idx];
    this.idx++;

    this.map.set(next.pos.index(), next.stone);
    return next;
  }

  /** Jumps to the given move index by undoing or redoing moves. */
  jump(index: number) {
    if (index > this.moves.length) return;
    if (this.idx < index) {
      for (let i = this.idx; i < index; i++) {
        let next = this.moves[i];
        this.map.set(next.pos.index(), next.stone);
      }
    } else {
      for (let i = this.idx - 1; i >= index; i--) {
        let last = this.moves[i];
        this.map.delete(last.pos.index());
      }
    }
    this.idx = index;
  }

  /**
   * Infers the next stone to play and whether the opponent
   * is to play after that, based on past moves.
   */
  inferTurn(): [Stone, boolean] {
    if (this.idx == 0) return [Stone.Black, true];

    let last = this.moves[this.idx - 1].stone;
    if (this.idx == 1) return [Stone.White, last == Stone.White];

    let prevOfLast = this.moves[this.idx - 2].stone;
    if (last == prevOfLast) return [Stone.opposite(last), false];
    return [last, true];
  }

  /** Serializes the board to an array of bytes. */
  serialize(): Uint8Array {
    let buf: number[] = [];
    for (let move of this.moves) {
      let x = (move.pos.index() << 1) | (move.stone - 1);
      varint.encode(x, buf, buf.length);
    }
    return new Uint8Array(buf);
  }

  /** Deserializes a board from an array of bytes. */
  static deserialize(buf: Uint8Array): Board | undefined {
    let board = new Board();
    let i = 0;
    while (i < buf.length) {
      let x;
      try {
        x = varint.decode(buf, i);
      } catch (e /* RangeError */) {
        return;
      }
      if (x > 0xffffffff) return;

      let pos = Point.fromIndex(x >>> 1);
      let stone = (x & 1) + 1;

      if (!board.set(pos, stone)) return;
      i += varint.decode.bytes!;
    }
    return board;
  }
}