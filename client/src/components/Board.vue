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

const DIST_FOR_PINCH_ZOOM = 2.0 * 96 / 2.54;
const DIST_FOR_SWIPE_RETRACT = 4.0 * 96 / 2.54;

const canvasContainer = ref<Element>();
const canvas = ref<HTMLCanvasElement>();
let ctx: CanvasRenderingContext2D;

let size: number;
let gridSize: number;

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
let prevViewSize: number;

enum ViewState {
  Calm,
  Moved,
  Pinched,
  Retracted,
}

let viewState = ViewState.Calm;

let ws: WebSocket;

function resizeCanvas() {
  let rect = canvasContainer.value!.getBoundingClientRect();
  let newSize = Math.min(rect.width, rect.height);
  if (newSize == size) return;

  size = newSize;
  gridSize = size / (viewSize + 1);

  let c = canvas.value!;
  c.style.width = c.style.height = size + 'px';

  let dpr = window.devicePixelRatio;
  c.width = c.height = size * dpr;
  ctx.scale(dpr, dpr);

  draw();
};

function outOfView(x: number, y: number): boolean {
  return (x >>> 0) >= viewSize || (y >>> 0) >= viewSize;
}

function canvasToViewPos(x: number, y: number): [Point, boolean] {
  x = Math.round(x / gridSize) - 1, y = Math.round(y / gridSize) - 1;
  return [new Point(x, y), outOfView(x, y)];
}

function viewToBoardPos(p: Point): Point {
  let x = p.x - (viewSize >>> 1) + viewCenter.x;
  let y = p.y - (viewSize >>> 1) + viewCenter.y;
  return new Point(x, y);
}

function canvasToBoardPos(x: number, y: number): [Point, boolean] {
  let [p, out] = canvasToViewPos(x, y);
  return [viewToBoardPos(p), out];
}

function clamp(n: number, min: number, max: number): number {
  return n < min ? min : (n > max ? max : n);
}

enum ClampTo {
  Inside,
  Border,
}

function boardToViewPos(p: Point, clampTo = ClampTo.Border): [Point, boolean] {
  let x = p.x + (viewSize >>> 1) - viewCenter.x;
  let y = p.y + (viewSize >>> 1) - viewCenter.y;
  let out = outOfView(x, y);

  let [min, max] = clampTo == ClampTo.Inside ? [0, viewSize - 1] : [-1, viewSize];
  return [new Point(clamp(x, min, max), clamp(y, min, max)), out];
}

function viewToCanvasPos(p: Point): [number, number] {
  return [(p.x + 1) * gridSize, (p.y + 1) * gridSize];
}

function send(msg: any) {
  if (ws.readyState != WebSocket.OPEN)
    return window.alert('连接已断开，请刷新页面。');
  ws.send(msg.toString());
}

function hitCursor() {
  if (!cursorPos || boardToViewPos(cursorPos)[1] /* out */)
    return;
  if (board.get(cursorPos)) return;

  if (pointEquals(tentativePos, cursorPos)) {
    tentativePos = undefined;
    send(cursorPos.index());
  } else {
    tentativePos = cursorPos.copy();
    draw();
  }
}

function clampCursor() {
  if (!cursorPos) return;
  let [p, out] = boardToViewPos(cursorPos, ClampTo.Inside);
  if (out) cursorPos = viewToBoardPos(p);
}

function followBoardPosOnDown(): boolean {
  let [pointer] = downPointers.values();
  let p0: Point = pointer.boardPosOnDown;
  let [p,] = canvasToBoardPos(pointer.last.offsetX, pointer.last.offsetY);

  let dx = p.x - p0.x, dy = p.y - p0.y;
  if (dx != 0 || dy != 0) {
    viewCenter.x -= dx, viewCenter.y -= dy;
    return true;
  }
  return false;
}

enum Zoom {
  Out,
  In,
}

function zoom(zoom: Zoom, wheelEvent?: WheelEvent) {
  if (zoom == Zoom.Out) {
    viewSize += 2;
  } else {
    if (viewSize == 1) return;
    viewSize -= 2;
  }

  gridSize = size / (viewSize + 1);

  if (downPointers.size == 0) {
    if (wheelEvent) {
      onRelativeMove(wheelEvent, true);
    } else {
      clampCursor();
    }
  } else if (downPointers.size == 1) {
    if (viewState > ViewState.Moved) return;
    followBoardPosOnDown();
    viewState = ViewState.Moved;
  } else {
    return;
  }
  draw();
}

function dist(a: MouseEvent, b: MouseEvent): number {
  return Math.hypot(a.offsetX - b.offsetX, a.offsetY - b.offsetY);
}

const DIRECTION_OFFSETS = [[0, -1], [-1, 0], [0, 1], [1, 0]];

function onKeyDown(e: KeyboardEvent) {
  let direction;
  switch (e.code) {
    case 'KeyW':
    case 'ArrowUp':
      direction = 0;
      break;
    case 'KeyA':
    case 'ArrowLeft':
      direction = 1;
      break;
    case 'KeyS':
    case 'ArrowDown':
      direction = 2;
      break;
    case 'KeyD':
    case 'ArrowRight':
      direction = 3;
      break;
    case 'Minus':
      return zoom(Zoom.Out);
    case 'Equal':
      return zoom(Zoom.In);
    default:
      return;
  }

  let [dx, dy] = DIRECTION_OFFSETS[direction];
  if (e.code.startsWith('K') /* Key */) {
    if (!cursorPos) {
      cursorPos = viewCenter.copy();
    } else {
      cursorPos.x += dx, cursorPos.y += dy;
      if (boardToViewPos(cursorPos)[1] /* out */)
        viewCenter.x += dx, viewCenter.y += dy;
    }
  } else {
    viewCenter.x += dx, viewCenter.y += dy;
    clampCursor();
  }
  draw();
}

function onKeyUp(e: KeyboardEvent) {
  switch (e.code) {
    case 'Backspace':
      return send(-1);
    case 'Space':
    case 'Enter':
      if (cursorPos) return hitCursor();
      cursorPos = viewCenter.copy();
      return draw();
  }
}

function onWheel(e: WheelEvent) {
  zoom(e.deltaY > 0 ? Zoom.Out : Zoom.In, e);
}

function onPointerDown(e: PointerEvent) {
  let [p,] = canvasToBoardPos(e.offsetX, e.offsetY);
  downPointers.set(e.pointerId, { down: e, last: e, boardPosOnDown: p });

  if (downPointers.size == 2) {
    prevViewSize = viewSize;
    viewState = ViewState.Pinched;
  }
}

function onPointerUp(e: PointerEvent) {
  if (!downPointers.delete(e.pointerId)) return;
  if (downPointers.size > 0) return;

  if (viewState != ViewState.Calm) {
    viewState = ViewState.Calm;
    return;
  }

  let [p, out] = canvasToBoardPos(e.offsetX, e.offsetY);
  if (!out) {
    cursorPos = p;
    hitCursor();
  }
}

function onPointerMove(e: PointerEvent) {
  let pointer = downPointers.get(e.pointerId);
  if (pointer) pointer.last = e;

  if (downPointers.size == 0) {
    onRelativeMove(e);
  } else if (downPointers.size == 1) {
    if (viewState > ViewState.Moved) return;

    if (followBoardPosOnDown()) {
      viewState = ViewState.Moved;
      draw();
    }
  } else if (downPointers.size == 2) {
    let [p1, p2] = [...downPointers.values()];

    let distDiff = dist(p1.last, p2.last) - dist(p1.down, p2.down);

    let newViewSize = prevViewSize - ((distDiff / DIST_FOR_PINCH_ZOOM) << 1);
    if (newViewSize < 1) newViewSize = 1;

    if (newViewSize != viewSize) {
      viewSize = newViewSize;
      gridSize = size / (viewSize + 1);
      draw();
    }
  } else if (downPointers.size == 3) {
    if (viewState == ViewState.Retracted) return;

    for (let p of downPointers.values()) {
      if (dist(p.last, p.down) < DIST_FOR_SWIPE_RETRACT) return;
    }

    viewState = ViewState.Retracted;
    send(-1);
  }
}

function onRelativeMove(e: MouseEvent, noDraw = false) {
  let p: Point | undefined, out;
  [p, out] = canvasToBoardPos(e.offsetX, e.offsetY);
  if (out) p = undefined;

  let shouldDraw = !noDraw && !pointEquals(p, cursorPos);
  cursorPos = p;
  if (shouldDraw) draw();
}

function onPointerLeave(e: PointerEvent) {
  downPointers.delete(e.pointerId);
  if (downPointers.size == 0) viewState = ViewState.Calm;

  let shouldDraw = cursorPos != undefined;
  cursorPos = undefined;
  if (shouldDraw) draw();
}

function drawCircle(p: Point, r: number) {
  let [x, y] = viewToCanvasPos(p);
  ctx.beginPath();
  ctx.arc(x, y, r, 0, 2 * Math.PI);
  ctx.fill();
}

function draw() {
  console.log('draw');
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
    drawCircle(p, starRadius);
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
    let [p, stone] = move;
    [p, out] = boardToViewPos(p);
    if (out) return outIndexes.add(p.index());

    ctx.fillStyle = stone == Stone.Black ? 'black' : 'white';
    drawCircle(p, stoneRadius);

    if (index >= starStart) {
      ctx.fillStyle = stone == Stone.Black ? 'white' : 'black';
      drawCircle(p, starRadius);
    }
  });

  ctx.fillStyle = 'gray';
  outIndexes.forEach(i => drawCircle(Point.fromIndex(i), stoneRadius));

  if (tentativePos && ([p, out] = boardToViewPos(tentativePos), !out) && !board.get(tentativePos)) {
    let [stone,] = board.inferTurn();

    ctx.globalAlpha = TENTATIVE_MOVE_OPACITY;
    ctx.fillStyle = stone == Stone.Black ? 'black' : 'white';
    drawCircle(p, stoneRadius);
    ctx.globalAlpha = 1;
  }

  if (cursorPos && ([p, out] = boardToViewPos(cursorPos), !out)) {
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
  ctx = canvas.value!.getContext('2d')!;

  resizeCanvas();
  new ResizeObserver(resizeCanvas).observe(canvasContainer.value!);

  window.addEventListener('keydown', onKeyDown);
  window.addEventListener('keyup', onKeyUp);

  ws = new WebSocket('ws://' + document.location.hostname + ':8080/ws');
  ws.onclose = () => window.alert('连接已断开，请刷新页面。');
  ws.onmessage = e => {
    let rec: number[] = JSON.parse(e.data);
    board.jump(0);
    for (let n of rec) {
      board.set(Point.fromIndex(n), board.inferTurn()[0]);
    }
    tentativePos = undefined;
    draw();
  };
});

onBeforeUnmount(() => {
  window.removeEventListener('keydown', onKeyDown);
  window.removeEventListener('keyup', onKeyUp);
});
</script>

<template>
  <div id="board-container" ref="canvasContainer">
    <canvas id="board" ref="canvas" @wheel="onWheel" @pointerdown="onPointerDown" @pointermove="onPointerMove"
      @pointerup="onPointerUp" @pointerleave="onPointerLeave"></canvas>
  </div>
</template>

<style>
#board-container {
  height: 100%;
}

#board {
  position: absolute;
  top: 50%;
  left: 50%;
  transform: translate(-50%, -50%);
  touch-action: none;
}
</style>