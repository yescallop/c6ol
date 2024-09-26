<script setup lang="ts">
import { onMounted, onBeforeUnmount, ref } from 'vue';
import { Board, Point, Stone, pointEquals } from '@/c6';

const BOARD_COLOR = '#ffcc66';
const CURSOR_COLOR = 'darkred';

const LINE_WIDTH_RATIO = 24.0;
const STONE_RADIUS_RATIO = 2.25;
const STAR_RADIUS_RATIO = 10.0;

const CURSOR_LINE_WIDTH_RATIO = 16.0;
const CURSOR_OFFSET_RATIO = 8.0;
const CURSOR_SIDE_RATIO = 4.0;

const TENTATIVE_MOVE_OPACITY = 0.5;

const DIST_FOR_PINCH_ZOOM_CM = 0.5;

const canvas = ref();
let ctx: CanvasRenderingContext2D;

let size = 0;
let gridSize = 0;

let board = new Board();
let viewCenter = new Point(0, 0);
let viewSize = 15;
let cursorPos: Point | undefined;
let tentativePos: Point | undefined;

interface Pointer {
  down: PointerEvent,
  last: PointerEvent,
  boardPosOnDown: Point,
}

let downPointers = new Map<number, Pointer>();

let prevViewSize = 15;

enum ViewState {
  Calm,
  Moved,
  Pinched,
}

let viewState = ViewState.Calm;

let conn: WebSocket;

function resizeCanvas() {
  let newSize = Math.min(window.innerWidth, window.innerHeight);
  if (newSize == size) return;
  size = newSize;
  gridSize = size / (viewSize + 1);

  let c = canvas.value;
  c.style.width = c.style.height = size + 'px';

  let dpr = window.devicePixelRatio;
  c.width = c.height = size * dpr;
  ctx.scale(dpr, dpr);

  paint();
};

function canvasToViewPos(x: number, y: number): [Point, boolean] {
  x = Math.floor(x / gridSize - 0.5), y = Math.floor(y / gridSize - 0.5);
  let out = x < 0 || x >= viewSize || y < 0 || y >= viewSize;
  return [new Point(x, y), out];
}

function viewToBoardPos(p: Point): Point {
  let x = p.x - (viewSize >>> 1) + viewCenter.x;
  let y = p.y - (viewSize >>> 1) + viewCenter.y;
  return new Point(x, y);
}

function boardToViewPos(p: Point): [Point, boolean] {
  let xMin = viewCenter.x - (viewSize >>> 1);
  let yMin = viewCenter.y - (viewSize >>> 1);
  let x = p.x - xMin, y = p.y - yMin;
  let out = false;
  if ((x >>> 0) >= viewSize || (y >>> 0) >= viewSize) {
    x = x < 0 ? -1 : (x >= viewSize ? viewSize : x);
    y = y < 0 ? -1 : (y >= viewSize ? viewSize : y);
    out = true;
  }
  return [new Point(x, y), out];
}

function viewToCanvasPos(p: Point): [number, number] {
  return [(p.x + 1) * gridSize, (p.y + 1) * gridSize];
}

function unoccupied(p: Point | undefined): Point | undefined {
  if (p && board.get(p)) p = undefined;
  return p;
}

function send(msg: any) {
  if (conn.readyState != conn.OPEN)
    return window.alert('连接已断开，请刷新页面。');
  conn.send(msg.toString());
}

function onKey(e: KeyboardEvent) {
  switch (e.code) {
    case 'KeyW':
      viewCenter.y--;
      break;
    case 'KeyA':
      viewCenter.x--;
      break;
    case 'KeyS':
      viewCenter.y++;
      break;
    case 'KeyD':
      viewCenter.x++;
      break;
    case 'Backspace':
      send(-1);
    default:
      return;
  }
  paint();
}

function followBoardPosOnDown(e: MouseEvent): boolean {
  let p0: Point = downPointers.values().next().value.boardPosOnDown;
  let [p,] = canvasToViewPos(e.offsetX, e.offsetY);
  p = viewToBoardPos(p);

  let dx = p.x - p0.x, dy = p.y - p0.y;
  if (dx != 0 || dy != 0) {
    viewCenter.x -= dx, viewCenter.y -= dy;
    return true;
  }
  return false;
}

function onWheel(e: WheelEvent) {
  if (e.deltaY > 0) {
    viewSize += 2;
  } else {
    if (viewSize == 1) return;
    viewSize -= 2;
  }
  gridSize = size / (viewSize + 1);

  if (downPointers.size == 0) {
    onRelativeMove(e, true);
  } else if (downPointers.size == 1) {
    if (viewState == ViewState.Pinched) return;

    followBoardPosOnDown(e);
    viewState = ViewState.Moved;
  } else {
    return;
  }
  paint();
}

function onDown(e: PointerEvent) {
  let [p,] = canvasToViewPos(e.offsetX, e.offsetY);
  p = viewToBoardPos(p);

  downPointers.set(e.pointerId, { down: e, last: e, boardPosOnDown: p });

  if (downPointers.size == 2) {
    prevViewSize = viewSize;
    viewState = ViewState.Pinched;
  } else if (downPointers.size == 3) {
    send(-1);
  }
}

function onUp(e: PointerEvent) {
  if (!downPointers.delete(e.pointerId)) return;
  if (downPointers.size > 0) return;

  if (viewState != ViewState.Calm) {
    viewState = ViewState.Calm;
    return;
  }

  let [p, out] = canvasToViewPos(e.offsetX, e.offsetY);
  if (out) return;
  p = viewToBoardPos(p);
  if (board.get(p)) return;

  if (pointEquals(tentativePos, p)) {
    tentativePos = undefined;
    send(p.index());
  } else {
    tentativePos = p;
    paint();
  }
}

function onMove(e: PointerEvent) {
  let pointer = downPointers.get(e.pointerId);
  if (pointer) pointer.last = e;

  if (downPointers.size == 0) {
    onRelativeMove(e);
  } else if (downPointers.size == 1) {
    if (viewState == ViewState.Pinched) return;

    if (followBoardPosOnDown(e)) {
      viewState = ViewState.Moved;
      paint();
    }
  } else if (downPointers.size == 2) {
    let [p1, p2] = [...downPointers.values()];

    function dist(a: PointerEvent, b: PointerEvent): number {
      return Math.hypot(a.offsetX - b.offsetX, a.offsetY - b.offsetY);
    }
    let distDiff = dist(p1.last, p2.last) - dist(p1.down, p2.down);

    let distForPinchZoom = DIST_FOR_PINCH_ZOOM_CM * 96 * window.devicePixelRatio / 2.54;
    let newViewSize = prevViewSize - ((distDiff / distForPinchZoom) << 1);

    if (newViewSize < 1) newViewSize = 1;

    if (newViewSize != viewSize) {
      viewSize = newViewSize;
      gridSize = size / (viewSize + 1);
      paint();
    }
  }
}

function onRelativeMove(e: MouseEvent, noPaint: boolean = false) {
  let p: Point | undefined, out;
  [p, out] = canvasToViewPos(e.offsetX, e.offsetY);
  p = out ? undefined : viewToBoardPos(p);

  let shouldPaint = !noPaint && !pointEquals(unoccupied(p), unoccupied(cursorPos));
  cursorPos = p;
  if (shouldPaint) paint();
}

function onLeave(e: PointerEvent) {
  downPointers.delete(e.pointerId);
  if (downPointers.size == 0) viewState = ViewState.Calm;

  let shouldPaint = unoccupied(cursorPos);
  cursorPos = undefined;
  if (shouldPaint) paint();
}

function paintCircle(p: Point, r: number) {
  let [x, y] = viewToCanvasPos(p);
  ctx.beginPath();
  ctx.arc(x, y, r, 0, 2 * Math.PI);
  ctx.fill();
}

function paint() {
  console.log('paint');
  ctx.fillStyle = BOARD_COLOR;
  ctx.fillRect(0, 0, size, size);

  ctx.strokeStyle = 'black';
  ctx.lineWidth = gridSize / LINE_WIDTH_RATIO;

  ctx.beginPath();
  ctx.setLineDash([]);
  for (let i = 1; i <= viewSize; i++) {
    let pos = gridSize * i;
    ctx.moveTo(gridSize, pos);
    ctx.lineTo(size - gridSize, pos);
    ctx.moveTo(pos, gridSize);
    ctx.lineTo(pos, size - gridSize);
  }
  ctx.stroke();

  ctx.beginPath();
  ctx.setLineDash([gridSize / 5, gridSize / 5]);
  for (let i = 1; i <= viewSize; i++) {
    let pos = gridSize * i;
    ctx.moveTo(0, pos);
    ctx.lineTo(gridSize, pos);
    ctx.moveTo(size - gridSize, pos);
    ctx.lineTo(size, pos);

    ctx.moveTo(pos, 0);
    ctx.lineTo(pos, gridSize);
    ctx.moveTo(pos, size - gridSize);
    ctx.lineTo(pos, size);
  }
  ctx.stroke();

  let starRadius = gridSize / STAR_RADIUS_RATIO;

  let center = new Point(0, 0);
  let [p, out] = boardToViewPos(center);
  if (!out && !board.get(center)) {
    ctx.fillStyle = 'black';
    paintCircle(p, starRadius);
  }

  function trailingSuccessiveMoves(rec: [Point, Stone][], end: number): number {
    if (end == 0) return 0;
    let stone = rec[end - 1][1], count = 1;
    for (let i = end - 2; i >= 0; i--) {
      if (rec[i][1] != stone) break;
      count++;
    }
    return count;
  }

  let rec = board.record();

  let stars = trailingSuccessiveMoves(rec, rec.length);
  if (stars == 1)
    stars += trailingSuccessiveMoves(rec, rec.length - 1);
  let starStart = rec.length - stars;

  let stoneRadius = gridSize / STONE_RADIUS_RATIO;
  let outIndexes = new Set<number>();

  rec.forEach((move, index) => {
    let [p, stone] = move, out;
    [p, out] = boardToViewPos(p);
    if (out) {
      outIndexes.add(p.index());
      return;
    }

    ctx.fillStyle = stone == Stone.Black ? 'black' : 'white';
    paintCircle(p, stoneRadius);

    if (index >= starStart) {
      ctx.fillStyle = stone == Stone.Black ? 'white' : 'black';
      paintCircle(p, starRadius);
    }
  });

  ctx.fillStyle = 'gray';
  outIndexes.forEach(i => {
    paintCircle(Point.fromIndex(i), stoneRadius);
  });

  if (tentativePos && ([p, out] = boardToViewPos(tentativePos), !out) && !board.get(tentativePos)) {
    let [stone,] = board.inferTurn();

    ctx.globalAlpha = TENTATIVE_MOVE_OPACITY;
    ctx.fillStyle = stone == Stone.Black ? 'black' : 'white';
    paintCircle(p, stoneRadius);
    ctx.globalAlpha = 1;
  }

  if (cursorPos && ([p, out] = boardToViewPos(cursorPos), !out) && !board.get(cursorPos)) {
    let [x, y] = viewToCanvasPos(p);

    ctx.lineWidth = gridSize / CURSOR_LINE_WIDTH_RATIO;

    let offset = gridSize / CURSOR_OFFSET_RATIO;
    let side = gridSize / CURSOR_SIDE_RATIO;
    let inOffset = offset - ctx.lineWidth / 2;
    let outOffset = offset + side;

    ctx.strokeStyle = CURSOR_COLOR;
    ctx.beginPath();
    for (let [dx, dy] of [[1, 1], [1, -1], [-1, -1], [-1, 1]]) {
      ctx.moveTo(x + inOffset * dx, y + offset * dy);
      ctx.lineTo(x + outOffset * dx, y + offset * dy);
      ctx.moveTo(x + offset * dx, y + inOffset * dy);
      ctx.lineTo(x + offset * dx, y + outOffset * dy);
    }
    ctx.stroke();
  }
}

onMounted(() => {
  ctx = canvas.value.getContext('2d');
  resizeCanvas();
  window.addEventListener('resize', resizeCanvas);
  window.addEventListener('keydown', onKey);

  conn = new WebSocket('ws://' + document.location.hostname + ':8080/ws');
  conn.onclose = () => {
    window.alert('连接已断开，请刷新页面。');
  };
  conn.onmessage = e => {
    let rec: number[] = JSON.parse(e.data);
    board.jump(0);
    for (let n of rec) {
      board.set(Point.fromIndex(n), board.inferTurn()[0]);
    }
    paint();
  };
});

onBeforeUnmount(() => {
  window.removeEventListener('resize', resizeCanvas);
  window.removeEventListener('keydown', onKey);
});
</script>

<template>
  <canvas ref="canvas" @wheel="onWheel" @pointerdown="onDown" @pointermove="onMove" @pointerup="onUp"
    @pointerleave="onLeave"></canvas>
</template>

<style>
body {
  background-color: #ffcc66;
}

canvas {
  position: absolute;
  top: 50%;
  left: 50%;
  transform: translate(-50%, -50%);
  touch-action: none;
}
</style>