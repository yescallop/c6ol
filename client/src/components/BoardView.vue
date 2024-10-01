<script setup lang="ts">
import { onMounted, onBeforeUnmount, ref } from 'vue';
import { Board, type Move, Point, Stone } from '@/c6';
import { Base64 } from 'js-base64';

const BOARD_COLOR = '#ffcc66';
const CURSOR_COLOR = 'darkred';

// Divide `gridSize` by the following ratios to get the corresponding lengths.

const LINE_WIDTH_RATIO = 24.0;
const LINE_DASH_RATIO = 5.0;

const STONE_RADIUS_RATIO = 2.25;
const STAR_RADIUS_RATIO = 10.0;

const CURSOR_LINE_WIDTH_RATIO = 16.0;
const CURSOR_OFFSET_RATIO = 8.0;
const CURSOR_SIDE_RATIO = 4.0;

const TENTATIVE_MOVE_OPACITY = 0.5;

const DIST_FOR_PINCH_ZOOM = 2.0 * 96 / 2.54; // 2cm
const DIST_FOR_SWIPE_RETRACT = 4.0 * 96 / 2.54; // 4cm

const canvasContainer = ref<Element>();
const canvas = ref<HTMLCanvasElement>();
let ctx: CanvasRenderingContext2D;

/** Pixel size of the canvas. */
let size: number;
/**
 * Pixel size of a single grid on the canvas.
 * Equals `size / (viewSize + 1)`.
 */
let gridSize: number;

let board = new Board();
/**
 * Size of the view. Minimum value is 1.
 *
 * The *view* refers to the area where the user can see and place stones.
 * Stones outside the view are drawn in gray on its *border*.
 */
let viewSize = 15;

// There are three kinds of positions:
//
// - Board position is in grids, relative to the origin of the board.
// - View position is in grids, relative to the top-left corner of the view.
// - Canvas position is in pixels, relative to the top-left corner of the canvas.
//
// The following three variables are board positions.

let viewCenter = new Point(0, 0);
// The user can *hit* a cursor by clicking the view or pressing Space or Enter.
// If a tentative move is at the cursor position, hitting the cursor makes
// it an actual move. Otherwise, a tentative move is made at the position.
let cursorPos: Point | undefined;
let tentativePos: Point | undefined;

interface Pointer {
  /** The `pointerdown` event fired when the pointer became active. */
  down: PointerEvent,
  /** Last event fired about the pointer. Can be `pointermove` or `pointerdown`. */
  last: PointerEvent,
  /** Board position the pointer was at when it became active. */
  boardPosOnDown: Point,
}

/**
 * Info about active pointers.
 *
 * A pointer is added to this map on a `pointerdown` event,
 * and removed on a `pointerup` or `pointerleave` event.
 */
let downPointers = new Map<number, Pointer>();
/** Set as the current `viewSize` when a 2-pointer gesture begins. */
let prevViewSize: number;

enum ViewState {
  /**
   * Default state. Entered when the only active pointer becomes inactive.
   *
   * When a `pointerup` event about the only active pointer fires, we try
   * to hit the cursor only when the previous state is `Calm`.
   */
  Calm,
  /**
   * Entered when the state is `Calm`, exactly one pointer is active, and
   * the view is dragged by pointer or zoomed by wheel or keyboard.
   *
   * The view may be dragged by pointer or zoomed by wheel or keyboard
   * only when the state is `Calm` or `Moved`.
   */
  Moved,
  /**
   * Entered when exactly one pointer is active,
   * and a second pointer becomes active.
   */
  Pinched,
  /**
   * Entered when a swipe retract is triggered.
   *
   * A swipe retract may only be triggered when the state is not `Retracted`.
   */
  Retracted,
}

let viewState = ViewState.Calm;

let ws: WebSocket;

/** Resizes the canvas to fit its container. */
function resizeCanvas() {
  let rect = canvasContainer.value!.getBoundingClientRect();
  let newSize = Math.min(rect.width, rect.height);
  if (newSize == size) return;

  size = newSize;
  gridSize = size / (viewSize + 1);

  let c = canvas.value!;
  c.style.width = c.style.height = size + 'px';

  // See: https://developer.mozilla.org/en-US/docs/Web/API/Window/devicePixelRatio
  let dpr = window.devicePixelRatio;
  c.width = c.height = size * dpr;
  ctx.scale(dpr, dpr);

  draw();
};

/** Tests if a view position is out of view. */
function outOfView(x: number, y: number): boolean {
  return x < 0 || x >= viewSize || y < 0 || y >= viewSize;
}

/** Converts a canvas position to view position, testing if it is out of view. */
function canvasToViewPos(x: number, y: number): [Point, boolean] {
  x = Math.round(x / gridSize) - 1, y = Math.round(y / gridSize) - 1;
  return [new Point(x, y), outOfView(x, y)];
}

/** Converts a view position to board position. */
function viewToBoardPos(p: Point): Point {
  let x = p.x - (viewSize >>> 1) + viewCenter.x;
  let y = p.y - (viewSize >>> 1) + viewCenter.y;
  return new Point(x, y);
}

/** Converts a canvas position to board position, testing if it is out of view. */
function canvasToBoardPos(x: number, y: number): [Point, boolean] {
  let [p, out] = canvasToViewPos(x, y);
  return [viewToBoardPos(p), out];
}

/** Restricts `n` to the interval [`min`, `max`]. */
function clamp(n: number, min: number, max: number): number {
  return n < min ? min : (n > max ? max : n);
}

enum ClampTo {
  Inside,
  InsideAndBorder,
}

/** Converts a board position to view position, restricting it to the given area. */
function boardToViewPos(p: Point, clampTo = ClampTo.InsideAndBorder): [Point, boolean] {
  let x = p.x + (viewSize >>> 1) - viewCenter.x;
  let y = p.y + (viewSize >>> 1) - viewCenter.y;
  let out = outOfView(x, y);

  let [min, max] = clampTo == ClampTo.Inside ? [0, viewSize - 1] : [-1, viewSize];
  return [new Point(clamp(x, min, max), clamp(y, min, max)), out];
}

/** Converts a view position to canvas position. */
function viewToCanvasPos(p: Point): [number, number] {
  return [(p.x + 1) * gridSize, (p.y + 1) * gridSize];
}

/** Sends the message on the WebSocket connection. */
function send(msg: Uint8Array) {
  if (ws.readyState != WebSocket.OPEN)
    return window.alert('Connection closed, please refresh the page.');
  ws.send(msg);
}

/** Attempts to make a tentative or actual move at the cursor position. */
function hitCursor() {
  if (!cursorPos || boardToViewPos(cursorPos)[1] /* out */)
    return;
  if (board.get(cursorPos)) return;

  if (Point.equal(tentativePos, cursorPos)) {
    tentativePos = undefined;
    send(cursorPos.serialize());
  } else {
    tentativePos = cursorPos.copy();
    draw();
  }
}

/** Restricts the cursor to the inside of the view. */
function clampCursor() {
  if (!cursorPos) return;
  let [p, out] = boardToViewPos(cursorPos, ClampTo.Inside);
  if (out) cursorPos = viewToBoardPos(p);
}

/**
 * Adjusts the view center so that the only active pointer is
 * at the same board position as when it became active.
 *
 * Returns whether the view center is changed.
 */
function followBoardPosOnDown(): boolean {
  let [pointer] = downPointers.values();
  let p0 = pointer.boardPosOnDown;
  let [p,] = canvasToBoardPos(pointer.last.offsetX, pointer.last.offsetY);

  let dx = p.x - p0.x, dy = p.y - p0.y;
  if (dx != 0 || dy != 0) {
    viewCenter.x -= dx, viewCenter.y -= dy;
    return true;
  }
  return false;
}

/** Moves the cursor to the pointer position, or removes it when out of view. */
function updateCursor(e: MouseEvent, noDraw = false) {
  let p: Point | undefined, out;
  [p, out] = canvasToBoardPos(e.offsetX, e.offsetY);
  if (out) p = undefined;

  // Draw if the cursor should appear, move, or disappear.
  let shouldDraw = !noDraw && !Point.equal(p, cursorPos);
  cursorPos = p;
  if (shouldDraw) draw();
}

enum Zoom {
  Out,
  In,
}

/** Handles zooming by wheel or keyboard. */
function zoom(zoom: Zoom, wheelEvent?: WheelEvent) {
  if (zoom == Zoom.Out) {
    viewSize += 2;
  } else {
    if (viewSize == 1) return;
    viewSize -= 2;
  }

  gridSize = size / (viewSize + 1);

  // When no pointer is active, zoom at the view center.
  // When exactly one pointer is active, zoom at the pointer position.
  if (downPointers.size == 0) {
    if (wheelEvent) {
      // Zooming by wheel. Try to keep the cursor at mouse position.
      updateCursor(wheelEvent, true);
    } else {
      // Zooming by keyboard. Restrict the cursor so that it doesn't go out of view.
      clampCursor();
    }
  } else if (downPointers.size == 1) {
    // If the view is pinched, bail out to avoid problems.
    if (viewState > ViewState.Moved) return;

    followBoardPosOnDown();
    viewState = ViewState.Moved;
  } else {
    return;
  }
  draw();
}

/** Retracts the last move (if any). */
function retract() {
  send(new Uint8Array());
}

/** Returns the Euclidean distance between the positions of two pointers. */
function dist(a: MouseEvent, b: MouseEvent): number {
  return Math.hypot(a.offsetX - b.offsetX, a.offsetY - b.offsetY);
}

/**
 * Handles `keydown` events.
 *
 * - Moves the cursor on WASD keys.
 * - Moves the view center on Arrow keys.
 * - Zooms out on Minus key.
 * - Zooms in on Plus (Equal) key.
 */
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

  // If the view is being dragged or pinched, bail out to avoid problems.
  if (downPointers.size > 0) return;

  const DIRECTION_OFFSETS = [[0, -1], [-1, 0], [0, 1], [1, 0]];

  let [dx, dy] = DIRECTION_OFFSETS[direction];
  if (e.code.startsWith('K') /* Key */) {
    if (!cursorPos) {
      // Put a cursor at the view center if there is no cursor.
      cursorPos = viewCenter.copy();
    } else {
      cursorPos.x += dx, cursorPos.y += dy;
      // If the cursor is going out of view, adjust the view center to keep up.
      if (boardToViewPos(cursorPos)[1] /* out */)
        viewCenter.x += dx, viewCenter.y += dy;
    }
  } else {
    viewCenter.x += dx, viewCenter.y += dy;
    // Restrict the cursor so that it doesn't go out of view.
    clampCursor();
  }
  draw();
}

/**
 * Handles `keyup` events.
 *
 * - Hits the cursor on Space and Enter keys.
 * - Retracts the last move on Backspace key.
 */
function onKeyUp(e: KeyboardEvent) {
  switch (e.code) {
    case 'Backspace':
      return retract();
    case 'Space':
    case 'Enter':
      if (cursorPos) return hitCursor();
      // Put a cursor at the view center if there is no cursor.
      cursorPos = viewCenter.copy();
      return draw();
  }
}

/** Handles `wheel` events. */
function onWheel(e: WheelEvent) {
  zoom(e.deltaY > 0 ? Zoom.Out : Zoom.In, e);
}

/** Handles `pointerdown` events. */
function onPointerDown(e: PointerEvent) {
  let [p,] = canvasToBoardPos(e.offsetX, e.offsetY);
  downPointers.set(e.pointerId, { down: e, last: e, boardPosOnDown: p });

  if (downPointers.size == 2) {
    prevViewSize = viewSize;
    viewState = ViewState.Pinched;
  }
}

/**
 * Handles `pointerup` events.
 *
 * Attempts to hit the cursor when the pointer is the only active one,
 * the left button is pressed, and the view isn't ever dragged,
 * zoomed, or pinched since the pointer became active.
 */
function onPointerUp(e: PointerEvent) {
  // Bail out if the pointer is already inactive due to a `pointerleave` event.
  if (!downPointers.delete(e.pointerId)) return;
  if (downPointers.size > 0) return;

  if (e.button != 0) return;

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

/**
 * Handles `pointermove` events.
 *
 * Performs different actions according to the number of active pointers:
 *
 * - 0: Updates the cursor.
 * - 1: Drags the view if it isn't ever pinched since the pointer became active.
 * - 2: Roughly speaking, whenever the distance of pointers increases (decreases)
 *      by `DIST_FOR_PINCH_ZOOM`, `viewSize` will be decreased (increased) by 2.
 * - 3: Retracts the last move if all pointers have moved for at least
 *      a distance of `DIST_FOR_SWIPE_RETRACT`.
 */
function onPointerMove(e: PointerEvent) {
  let pointer = downPointers.get(e.pointerId);
  if (pointer) pointer.last = e;

  if (downPointers.size == 0) {
    updateCursor(e);
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
    retract();
  }
}

/** Handles `pointerleave` events. */
function onPointerLeave(e: PointerEvent) {
  downPointers.delete(e.pointerId);
  if (downPointers.size == 0) viewState = ViewState.Calm;

  // Draw if the cursor should disappear.
  let shouldDraw = cursorPos != undefined;
  cursorPos = undefined;
  if (shouldDraw) draw();
}

/** Draws a circle at a view position with the given radius. */
function drawCircle(p: Point, r: number) {
  let [x, y] = viewToCanvasPos(p);
  ctx.beginPath();
  ctx.arc(x, y, r, 0, 2 * Math.PI);
  ctx.fill();
}

/** Draws the view. */
function draw() {
  // Draw the board background.
  ctx.fillStyle = BOARD_COLOR;
  ctx.fillRect(0, 0, size, size);

  ctx.strokeStyle = 'black';
  ctx.lineWidth = gridSize / LINE_WIDTH_RATIO;

  // Draw the solid lines inside the view.
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

  let lineSeg = gridSize / LINE_DASH_RATIO;

  // Draw the dashed lines outside the view.
  ctx.beginPath();
  ctx.setLineDash([lineSeg, lineSeg]);
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

  // Draw the board origin.
  let origin = new Point(0, 0);
  let [p, out] = boardToViewPos(origin);
  if (!out && !board.get(origin)) {
    ctx.fillStyle = 'black';
    drawCircle(p, starRadius);
  }

  /** Returns the number of successive same-stone moves ending at the given index. */
  function trailingSuccessiveMoves(moves: Move[], end: number): number {
    if (end == 0) return 0;
    let stone = moves[end - 1].stone, count = 1;
    for (let i = end - 2; i >= 0; i--) {
      if (moves[i].stone != stone) break;
      count++;
    }
    return count;
  }

  let moves = board.pastMoves();

  // Count the number of stars we should draw.
  let stars = trailingSuccessiveMoves(moves, moves.length);
  if (stars == 1)
    stars += trailingSuccessiveMoves(moves, moves.length - 1);
  let starStart = moves.length - stars;

  let stoneRadius = gridSize / STONE_RADIUS_RATIO;
  // We project the out-of-view stones onto the view border,
  // and stores the indexes of resulting points in this set.
  let outIndexes = new Set<number>();

  // Draw the stones and the stars.
  moves.forEach((move, index) => {
    let { pos, stone } = move;
    let [p, out] = boardToViewPos(pos);
    if (out) return outIndexes.add(p.index());

    ctx.fillStyle = stone == Stone.Black ? 'black' : 'white';
    drawCircle(p, stoneRadius);

    if (index >= starStart) {
      ctx.fillStyle = stone == Stone.Black ? 'white' : 'black';
      drawCircle(p, starRadius);
    }
  });

  // Draw the out-of-view stones on the view border.
  ctx.fillStyle = 'gray';
  outIndexes.forEach(i => drawCircle(Point.fromIndex(i), stoneRadius));

  // Draw the tentative move.
  if (tentativePos && ([p, out] = boardToViewPos(tentativePos), !out) && !board.get(tentativePos)) {
    let [stone,] = board.inferTurn();

    ctx.globalAlpha = TENTATIVE_MOVE_OPACITY;
    ctx.fillStyle = stone == Stone.Black ? 'black' : 'white';
    drawCircle(p, stoneRadius);
    ctx.globalAlpha = 1;
  }

  // Draw the cursor.
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

/**
 * Handles `copy` events.
 *
 * Copies the board URI into the clipboard.
 */
function onCopy(e: ClipboardEvent) {
  let uri = 'c6:' + Base64.fromUint8Array(board.serialize(), true) + ';';
  e.clipboardData!.setData("text/plain", uri);
  // `preventDefault` is required for the change to take effect.
  e.preventDefault();
}

onMounted(() => {
  ctx = canvas.value!.getContext('2d')!;

  resizeCanvas();
  new ResizeObserver(resizeCanvas).observe(canvasContainer.value!);

  window.addEventListener('keydown', onKeyDown);
  window.addEventListener('keyup', onKeyUp);
  window.addEventListener('copy', onCopy);

  ws = new WebSocket('ws://' + document.location.host + '/ws');
  ws.binaryType = "arraybuffer";
  ws.onclose = () => window.alert('Connection closed, please refresh the page.');
  ws.onmessage = e => {
    let newBoard = Board.deserialize(new Uint8Array(e.data));
    if (!newBoard) return;
    board = newBoard;
    tentativePos = undefined;
    draw();
  };
});

onBeforeUnmount(() => {
  window.removeEventListener('keydown', onKeyDown);
  window.removeEventListener('keyup', onKeyUp);
  window.removeEventListener('copy', onCopy);
});
</script>

<template>
  <div id="view-container" ref="canvasContainer">
    <canvas id="view" ref="canvas" @wheel="onWheel" @pointerdown="onPointerDown" @pointermove="onPointerMove"
      @pointerup="onPointerUp" @pointerleave="onPointerLeave"></canvas>
  </div>
</template>

<style>
#view-container {
  height: 100%;
}

#view {
  /*
    `top` and `left` positions the top-left corner of the canvas in the center,
    and `transform` translates the canvas left and up half its size.
  */
  position: absolute;
  top: 50%;
  left: 50%;
  transform: translate(-50%, -50%);

  /*
    Touch input by default triggers browser behavior such as refresh and zooming.
    Disable it to make the pointer events fire.
  */
  touch-action: none;
}
</style>