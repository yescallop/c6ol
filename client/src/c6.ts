/** A 2D point with integer coordinates. */
export class Point {
  x: number;
  y: number;

  constructor(x: number, y: number) {
    this.x = x;
    this.y = y;
  }

  /** Returns the index of the point on the spiral. */
  index(): number {
    if (this.x == 0 && this.y == 0) return 0;

    let xAbs = Math.abs(this.x), yAbs = Math.abs(this.y);
    let vertical = xAbs > yAbs;
    let k = vertical ? xAbs : yAbs;
    let t = k << 1;
    // m=(t-1)^2

    if (vertical) {
      if (this.x > 0) {
        // Right: m+y+k-1
        return (t - 1) ** 2 + this.y + k - 1;
      } else {
        // Left: m+2t+k-1-y
        return t * t + k - this.y;
      }
    } else if (this.y > 0) {
      // Top: m+t-1+k-x
      return t * t - t + k - this.x;
    } else {
      // Bottom: m+3t-1+x+k
      return t * t + t + this.x + k;
    }
  }

  /** Creates a point from its index on the spiral. */
  static fromIndex(n: number): Point {
    if (n == 0) return new Point(0, 0);

    /** Computes the integer square root of `s`. */
    function isqrt(s: number): number {
      if (s <= 1) return s;
      let x0 = s >>> 1;
      let x1 = (x0 + (s / x0) >>> 0) >>> 1;
      while (x1 < x0) {
        x0 = x1;
        x1 = (x0 + (s / x0) >>> 0) >>> 1;
      }
      return x0;
    }

    let k = (isqrt(n) + 1) >>> 1;
    let t = k << 1;
    let m = t * t + 1; // m=(t-1)^2+2t

    if (n < m) {
      m -= t;
      if (n < m) {
        // Right
        return new Point(k, k + 1 - (m - n));
      } else {
        // Top
        return new Point(k - 1 - (n - m), k);
      }
    } else {
      m += t;
      if (n < m) {
        // Left
        return new Point(-k, -k - 1 + (m - n));
      } else {
        // Bottom
        return new Point(-k + 1 + (n - m), -k);
      }
    }
  }

  copy(): Point {
    return new Point(this.x, this.y);
  }
}

/** Tests if two possibly undefined points equal. */
export function pointEquals(a: Point | undefined, b: Point | undefined): boolean {
  if (a == undefined) return b == undefined;
  return b != undefined && a.x == b.x && a.y == b.y;
}

/** A stone on the board, either black or white. */
export enum Stone {
  // 0 would be falsy.
  Black = 1,
  White = 2,
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
   * Infers the next stone to play and whether the opposite stone
   * is going to play after that, based on past moves.
   */
  inferTurn(): [Stone, boolean] {
    if (this.idx == 0) return [Stone.Black, true];

    let last = this.moves[this.idx - 1].stone;
    if (this.idx == 1) return [Stone.White, last == Stone.White];

    let prevOfLast = this.moves[this.idx - 2].stone;
    if (last == prevOfLast) return [last ^ 3, false];
    return [last, true];
  }
}