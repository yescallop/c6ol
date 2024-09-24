<script setup lang="ts">
import { onMounted, onBeforeUnmount, ref } from 'vue';
import { Board, Point, Stone } from '@/c6';

const LINE_WIDTH_RATIO = 24.0;
const STONE_RADIUS_RATIO = 2.25;
const STAR_RADIUS_RATIO = 10.0;

const canvas = ref();
let ctx: CanvasRenderingContext2D;

let size = 0;
let gridSize = 0;

let board = new Board();
let viewCenter = new Point(0, 0);
let viewSize = 15;
let cursorPos: Point | undefined;

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

function canvasToViewPos(x: number, y: number): Point | undefined {
  x = x / gridSize - 0.5, y = y / gridSize - 0.5;
  if (x < 0 || x >= viewSize || y < 0 || y >= viewSize) return;
  return new Point(x >>> 0, y >>> 0);
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
  if (p && board.get(viewToBoardPos(p))) {
    p = undefined;
  }
  return p;
}

function onClick(e: MouseEvent) {
  let p = canvasToViewPos(e.offsetX, e.offsetY);
  if (!p) return;
  p = viewToBoardPos(p);

  // let [stone,] = board.inferTurn();
  // if (board.set(p, stone)) paint();

  send(p.index());
}

function send(msg: any) {
  if (conn.readyState != conn.OPEN) {
    window.alert("连接已断开，请刷新页面。");
    return;
  }
  conn.send(msg.toString());
}

function onKey(e: KeyboardEvent) {
  switch (e.code) {
    case "KeyW":
      viewCenter.y--;
      break;
    case "KeyA":
      viewCenter.x--;
      break;
    case "KeyS":
      viewCenter.y++;
      break;
    case "KeyD":
      viewCenter.x++;
      break;
    case "Backspace":
      send(-1);
      break;
    default:
      return;
  }
  paint();
}

function onWheel(e: WheelEvent) {
  if (e.deltaY > 0) {
    viewSize += 2;
  } else {
    if (viewSize == 1) return;
    viewSize -= 2;
  }
  gridSize = size / (viewSize + 1);

  onHover(e, true);
  paint();
}

function onHover(e: MouseEvent, noPaint: boolean = false) {
  let pos = canvasToViewPos(e.offsetX, e.offsetY);
  let shouldPaint = !noPaint && unoccupied(pos) != unoccupied(cursorPos);
  cursorPos = pos;
  if (shouldPaint) paint();
}

function paint() {
  ctx.fillStyle = '#ffcc66';
  ctx.fillRect(0, 0, size, size);

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
  if (stars == 1) {
    stars += trailingSuccessiveMoves(rec, rec.length - 1);
  }
  let starStart = rec.length - stars;

  let stoneRadius = gridSize / STONE_RADIUS_RATIO;
  let starRadius = gridSize / STAR_RADIUS_RATIO;
  let outIndexes = new Set<number>();

  rec.forEach((move, index) => {
    let [p, stone] = move, out;
    [p, out] = boardToViewPos(p);
    if (out) {
      outIndexes.add(p.index());
      return;
    }
    let [x, y] = viewToCanvasPos(p);

    ctx.beginPath();
    ctx.arc(x, y, stoneRadius, 0, 2 * Math.PI);
    ctx.fillStyle = stone == Stone.Black ? 'black' : 'white';
    ctx.fill();

    if (index >= starStart) {
      ctx.beginPath();
      ctx.arc(x, y, starRadius, 0, 2 * Math.PI);
      ctx.fillStyle = stone == Stone.Black ? 'white' : 'black';
      ctx.fill();
    }
  });

  ctx.fillStyle = 'gray';
  outIndexes.forEach(i => {
    let [x, y] = viewToCanvasPos(Point.fromIndex(i));
    ctx.beginPath();
    ctx.arc(x, y, stoneRadius, 0, 2 * Math.PI);
    ctx.fill();
  });

  if (unoccupied(cursorPos)) {
    let [x, y] = viewToCanvasPos(cursorPos!);
    let [stone,] = board.inferTurn();

    ctx.globalAlpha = 0.5;
    ctx.beginPath();
    ctx.arc(x, y, stoneRadius, 0, 2 * Math.PI);
    ctx.fillStyle = stone == Stone.Black ? 'black' : 'white';
    ctx.fill();
    ctx.globalAlpha = 1;
  }
}

onMounted(() => {
  ctx = canvas.value.getContext('2d');
  resizeCanvas();
  window.addEventListener('resize', resizeCanvas);
  window.addEventListener('keydown', onKey);

  conn = new WebSocket("ws://" + document.location.host + "/ws");
  conn.onclose = () => {
    window.alert("连接已断开，请刷新页面。");
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
  <canvas ref="canvas" @click="onClick" @wheel="onWheel" @mouseenter="onHover" @mousemove="onHover"></canvas>
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
}
</style>