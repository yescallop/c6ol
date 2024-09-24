export class Point {
  x: number;
  y: number;

  constructor(x: number, y: number) {
    this.x = x;
    this.y = y;
  }

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

  static fromIndex(n: number): Point {
    if (n == 0) return new Point(0, 0);

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
}

export enum Stone {
  // 0 would be falsy.
  Black = 1,
  White = 2,
}

export class Board {
  private map: Map<number, Stone>;
  private rec: [Point, Stone][];
  private idx: number;

  constructor() {
    this.map = new Map();
    this.rec = [];
    this.idx = 0;
  }

  total(): number {
    return this.rec.length;
  }

  index(): number {
    return this.idx;
  }

  empty(): boolean {
    return this.idx == 0;
  }

  get(point: Point): Stone | undefined {
    return this.map.get(point.index());
  }

  record(): [Point, Stone][] {
    return this.rec.slice(0, this.idx);
  }

  set(p: Point, stone: Stone): boolean {
    let i = p.index();
    if (this.map.has(i)) return false;
    this.map.set(i, stone);

    this.rec.splice(this.idx);
    this.rec.push([p, stone]);
    this.idx++;
    return true;
  }

  unset(): [Point, Stone] | undefined {
    if (this.idx == 0) return;
    this.idx--;
    let last = this.rec[this.idx];

    this.map.delete(last[0].index());
    return last;
  }

  reset(): [Point, Stone] | undefined {
    if (this.idx >= this.rec.length) return;
    let next = this.rec[this.idx];
    this.idx++;

    this.map.set(next[0].index(), next[1]);
    return next;
  }

  jump(index: number) {
    if (index > this.rec.length) return;
    if (this.idx < index) {
      for (let i = this.idx; i < index; i++) {
        let next = this.rec[i];
        this.map.set(next[0].index(), next[1]);
      }
    } else {
      for (let i = this.idx - 1; i >= index; i--) {
        let last = this.rec[i];
        this.map.delete(last[0].index());
      }
    }
    this.idx = index;
  }

  inferTurn(): [Stone, boolean] {
    if (this.idx == 0) return [Stone.Black, true];

    let last = this.rec[this.idx - 1][1];
    if (this.idx == 1) return [Stone.White, last == Stone.White];

    let prevOfLast = this.rec[this.idx - 2][1];
    if (last == prevOfLast) return [last ^ 3, false];
    return [last, true];
  }
}