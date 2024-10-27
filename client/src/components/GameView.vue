<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref, watch } from 'vue';
import { MoveKind, Point, Record, Stone } from '@/game';
import { encodeBase64Url } from '@std/encoding/base64url';

const { record, ourStone, disabled } = defineProps<{
  record: Record;
  ourStone?: Stone;
  disabled: boolean;
}>();

const emit = defineEmits<{
  menu: [];
  move: [pos: [] | [Point] | [Point, Point]];
  undo: [];
  redo: [];
}>();

// FIXME: This is very hacky.
watch([record, () => ourStone], () => {
  phantom = tentative = undefined;
  if (!ourStone) cursor = undefined;
  draw();
});

let lastHoverBeforeEnabled: PointerEvent | undefined;

watch(() => disabled, () => {
  if (!disabled && lastHoverBeforeEnabled) {
    onHover(lastHoverBeforeEnabled);
    lastHoverBeforeEnabled = undefined;
  }
});

const BOARD_COLOR = '#ffcc66';
const CURSOR_COLOR_ACTIVE = 'darkred';
const CURSOR_COLOR_INACTIVE = 'grey';

// Divide `gridSize` by the following ratios to get the corresponding lengths.

const LINE_WIDTH_RATIO = 24;
const LINE_DASH_RATIO = 5;

const STONE_RADIUS_RATIO = 2.25;
const DOT_RADIUS_RATIO = STONE_RADIUS_RATIO * 6;

const CURSOR_LINE_WIDTH_RATIO = 16;
const CURSOR_OFFSET_RATIO = 8;
const CURSOR_SIDE_RATIO = 4;

const PHANTOM_MOVE_OPACITY = 0.5;

const PASS_FONT_SIZE_RATIO = 4;
const PASS_BORDER_RATIO = 100;
const PASS_OPACITY = 0.5;

const DIST_FOR_PINCH_ZOOM = 2 * 96 / 2.54; // 2cm
const DIST_FOR_SWIPE_RETRACT = 4 * 96 / 2.54; // 4cm

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
// The following are board positions.

const viewCenter = new Point(0, 0);
// The user can *hit* a cursor by clicking the view or pressing Space or Enter.
let cursor: Point | undefined;
let phantom: Point | undefined;
let tentative: Point | undefined;

interface Pointer {
  /** The `pointerdown` event fired when the pointer became active. */
  down: PointerEvent;
  /**
   * Last event fired about the pointer.
   * Can be `pointerenter`, `pointermove`, or `pointerdown`.
   */
  last: PointerEvent;
  /** Board position the pointer was at when it became active. */
  boardPosOnDown: Point;
}

/**
 * Info about active pointers.
 *
 * A pointer is added to this map on a `pointerdown` event,
 * and removed on a `pointerup` or `pointerleave` event.
 */
const downPointers = new Map<number, Pointer>();
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

/** Resizes the canvas to fit its container. */
function resizeCanvas() {
  const rect = canvasContainer.value!.getBoundingClientRect();
  const newSize = Math.min(rect.width, rect.height);
  if (newSize == size) return;

  size = newSize;
  gridSize = size / (viewSize + 1);

  const c = canvas.value!;
  c.style.width = c.style.height = size + 'px';

  // See: https://developer.mozilla.org/en-US/docs/Web/API/Window/devicePixelRatio
  const dpr = window.devicePixelRatio;
  c.width = c.height = size * dpr;
  ctx.scale(dpr, dpr);

  draw();
}

/** Tests if a view position is out of view. */
function outOfView(x: number, y: number): boolean {
  return x < 0 || x >= viewSize || y < 0 || y >= viewSize;
}

/** Converts a canvas position to view position, testing if it is out of view. */
function canvasToViewPos(x: number, y: number): [Point, boolean] {
  x = Math.round(x / gridSize) - 1;
  y = Math.round(y / gridSize) - 1;
  return [new Point(x, y), outOfView(x, y)];
}

/** Converts a view position to board position. */
function viewToBoardPos(p: Point): Point {
  const x = p.x - (viewSize >>> 1) + viewCenter.x;
  const y = p.y - (viewSize >>> 1) + viewCenter.y;
  return new Point(x, y);
}

/** Converts a canvas position to board position, testing if it is out of view. */
function canvasToBoardPos(x: number, y: number): [Point, boolean] {
  const [p, out] = canvasToViewPos(x, y);
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
  const x = p.x + (viewSize >>> 1) - viewCenter.x;
  const y = p.y + (viewSize >>> 1) - viewCenter.y;
  const out = outOfView(x, y);

  const [min, max] = clampTo == ClampTo.Inside ? [0, viewSize - 1] : [-1, viewSize];
  return [new Point(clamp(x, min, max), clamp(y, min, max)), out];
}

/** Converts a view position to canvas position. */
function viewToCanvasPos(p: Point): [number, number] {
  return [(p.x + 1) * gridSize, (p.y + 1) * gridSize];
}

/** Tests if it is our turn to play. */
function ourTurn(): boolean {
  return !record.isEnded() && ourStone == record.turn();
}

/**
 * Hits the cursor.
 *
 * Hitting an empty position puts a phantom stone there. Hitting a phantom stone
 * makes it tentative. Hitting a tentative stone makes it phantom. When there are
 * enough tentative stones for this turn, the move is automatically submitted.
 */
function hitCursor() {
  if (!cursor || boardToViewPos(cursor)[1] /* out */) return;
  if (!ourTurn() || record.stoneAt(cursor)) return;

  if (Point.equal(tentative, cursor)) {
    phantom = tentative;
    tentative = undefined;
    draw();
  } else if (Point.equal(phantom, cursor)) {
    if (!record.hasPast()) {
      emit('move', [cursor.copy()]);
    } else if (tentative) {
      emit('move', [tentative.copy(), cursor.copy()]);
    } else {
      tentative = phantom;
      phantom = undefined;
      draw();
    }
  } else {
    phantom = cursor.copy();
    draw();
  }
}

/** Restricts the cursor to the inside of the view. */
function clampCursor() {
  if (!cursor) return;
  const [p, out] = boardToViewPos(cursor, ClampTo.Inside);
  if (out) cursor = viewToBoardPos(p);
}

/**
 * Adjusts the view center so that the only active pointer is
 * at the same board position as when it became active.
 *
 * Returns whether the view center is changed.
 */
function followBoardPosOnDown(): boolean {
  const [pointer] = downPointers.values();
  const p0 = pointer.boardPosOnDown;
  // FIXME: This does not work correctly when zooming with Firefox.
  const [p] = canvasToBoardPos(pointer.last.offsetX, pointer.last.offsetY);

  const dx = p.x - p0.x, dy = p.y - p0.y;
  if (dx != 0 || dy != 0) {
    viewCenter.x -= dx;
    viewCenter.y -= dy;
    return true;
  }
  return false;
}

/** Moves the cursor to the pointer position, or removes it when out of view. */
function updateCursor(e: MouseEvent, noDraw = false) {
  const [p, out] = canvasToBoardPos(e.offsetX, e.offsetY);
  const newCursor = out ? undefined : p;

  // Draw if the cursor should appear, move, or disappear.
  const shouldDraw = !noDraw && !Point.equal(newCursor, cursor);
  cursor = newCursor;
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

/** Returns the Euclidean distance between the positions of two pointers. */
function dist(a: MouseEvent, b: MouseEvent): number {
  return Math.hypot(a.offsetX - b.offsetX, a.offsetY - b.offsetY);
}

/**
 * Handles `keydown` events.
 *
 * - Moves the cursor on W/A/S/D key.
 * - Moves the view center on Arrow Up/Left/Down/Right key.
 * - Zooms out on Minus key.
 * - Zooms in on Plus (Equal) key.
 * - Hits the cursor on Space/Enter key.
 * - Undoes the previous move (if any) on Backspace key.
 * - Redoes the next move (if any) on Shift+Backspace keys.
 */
function onKeyDown(e: KeyboardEvent) {
  if (disabled) return;

  let direction;
  switch (e.code) {
    case 'Escape':
      // Required for the dialog not to close immediately.
      e.preventDefault();
      return emit('menu');
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
    case 'KeyP':
      if (e.repeat || !ourTurn()) return;
      return emit('move', tentative ? [tentative.copy()] : []);
    case 'Backspace':
      if (e.repeat) return;
      if (e.shiftKey) {
        if (record.hasFuture()) emit('redo');
      } else {
        if (record.hasPast()) emit('undo');
      }
      return;
    case 'Enter':
      // Required for the dialog not to close immediately.
      e.preventDefault();
    case 'Space':
      if (e.repeat) return;
      if (cursor) return hitCursor();
      // Put a cursor at the view center if there is no cursor.
      cursor = viewCenter.copy();
      draw();
      return;
    default:
      return;
  }

  // If the view is being dragged or pinched, bail out to avoid problems.
  if (downPointers.size > 0) return;

  const DIRECTION_OFFSETS = [[0, -1], [-1, 0], [0, 1], [1, 0]];

  const [dx, dy] = DIRECTION_OFFSETS[direction];
  if (e.code.startsWith('K') /* Key */) {
    if (!cursor) {
      // Put a cursor at the view center if there is no cursor.
      cursor = viewCenter.copy();
    } else {
      cursor.x += dx;
      cursor.y += dy;
      // If the cursor is going out of view, adjust the view center to keep up.
      if (boardToViewPos(cursor)[1] /* out */) {
        viewCenter.x += dx;
        viewCenter.y += dy;
      }
    }
  } else {
    viewCenter.x += dx;
    viewCenter.y += dy;
    // Restrict the cursor so that it doesn't go out of view.
    clampCursor();
  }
  draw();
}

/** Handles `wheel` events. */
function onWheel(e: WheelEvent) {
  zoom(e.deltaY > 0 ? Zoom.Out : Zoom.In, e);
}

/** Handles `pointerdown` events. */
function onPointerDown(e: PointerEvent) {
  const [p] = canvasToBoardPos(e.offsetX, e.offsetY);
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
 * the view isn't ever dragged, zoomed, or pinched since the pointer
 * became active, the view isn't disabled, and the main button is pressed.
 */
function onPointerUp(e: PointerEvent) {
  // Bail out if the pointer is already inactive due to a `pointerleave` event.
  if (!downPointers.delete(e.pointerId)) return;
  if (downPointers.size > 0) return;

  if (viewState != ViewState.Calm) {
    viewState = ViewState.Calm;
    return;
  }
  if (disabled || e.button != 0) return;

  const [p, out] = canvasToBoardPos(e.offsetX, e.offsetY);
  if (!out) {
    cursor = p;
    hitCursor();
  }
}

/**
 * Handles `pointerenter` and `pointermove` events.
 *
 * Performs different actions according to the number of active pointers:
 *
 * - 0: Updates the cursor.
 * - 1: Drags the view if it isn't ever pinched since the pointer became active.
 * - 2: Roughly speaking, whenever the distance of pointers increases (decreases)
 *      by `DIST_FOR_PINCH_ZOOM`, `viewSize` will be decreased (increased) by 2.
 * - 3: Retracts the previous move if all pointers have moved for at least
 *      a distance of `DIST_FOR_SWIPE_RETRACT`.
 */
function onHover(e: PointerEvent) {
  if (disabled) {
    // We can reach here for either of the following reasons:
    // - A dialog was closed with a pointer which then entered the view.
    // - A game menu was opened by touch, but a glitch keeps the browser
    //   firing pointer events on the view until the touch ends.
    //
    // In either case, we record the event. We will either clear it when a
    // corresponding `pointerleave` event is fired, or replay it after the
    // view is enabled. The cursor will be updated only in the former case
    // if no new dialog was opened as soon as the previous one was closed.
    lastHoverBeforeEnabled = e;
    return;
  }

  const pointer = downPointers.get(e.pointerId);
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
    const [p1, p2] = [...downPointers.values()];

    const distDiff = dist(p1.last, p2.last) - dist(p1.down, p2.down);

    let newViewSize = prevViewSize - ((distDiff / DIST_FOR_PINCH_ZOOM) << 1);
    if (newViewSize < 1) newViewSize = 1;

    if (newViewSize != viewSize) {
      viewSize = newViewSize;
      gridSize = size / (viewSize + 1);
      draw();
    }
  } else if (downPointers.size == 3) {
    if (viewState == ViewState.Retracted) return;

    for (const p of downPointers.values()) {
      if (dist(p.last, p.down) < DIST_FOR_SWIPE_RETRACT) return;
    }

    viewState = ViewState.Retracted;
    if (record.hasPast()) emit('undo');
  }
}

/** Handles `pointerleave` events. */
function onPointerLeave(e: PointerEvent) {
  downPointers.delete(e.pointerId);
  if (downPointers.size == 0) viewState = ViewState.Calm;
  if (lastHoverBeforeEnabled?.pointerId == e.pointerId)
    lastHoverBeforeEnabled = undefined;
}

/** Draws a circle at a view position with the given radius. */
function drawCircle(p: Point, r: number) {
  const [x, y] = viewToCanvasPos(p);
  ctx.beginPath();
  ctx.arc(x, y, r, 0, 2 * Math.PI);
  ctx.fill();
}

/** Draws the view. */
function draw() {
  console.log('draw');
  // Draw the board background.
  ctx.fillStyle = BOARD_COLOR;
  ctx.fillRect(0, 0, size, size);

  ctx.strokeStyle = 'black';
  ctx.lineWidth = gridSize / LINE_WIDTH_RATIO;

  // Draw the solid lines inside the view.
  ctx.beginPath();
  for (let i = 1; i <= viewSize; i++) {
    const pos = gridSize * i;
    ctx.moveTo(gridSize, pos);
    ctx.lineTo(size - gridSize, pos);
    ctx.moveTo(pos, gridSize);
    ctx.lineTo(pos, size - gridSize);
  }
  ctx.stroke();

  const lineSeg = gridSize / LINE_DASH_RATIO;

  // Draw the dashed lines outside the view.
  ctx.beginPath();
  ctx.setLineDash([lineSeg, lineSeg]);
  for (let i = 1; i <= viewSize; i++) {
    const pos = gridSize * i;
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
  ctx.setLineDash([]);

  const dotRadius = gridSize / DOT_RADIUS_RATIO;

  // Draw the board origin.
  const origin = new Point(0, 0);
  let [p, out] = boardToViewPos(origin);
  if (!out && !record.stoneAt(origin)) {
    ctx.fillStyle = 'black';
    drawCircle(p, dotRadius);
  }

  const moves = record.moves(), moveIndex = record.moveIndex();
  const stoneRadius = gridSize / STONE_RADIUS_RATIO;
  // We project the out-of-view stones onto the view border,
  // and stores the indexes of resulting points in this set.
  const outIndexes = new Set<number>();

  // Draw the stones.
  for (let i = 0; i < moveIndex; i++) {
    const move = moves[i];
    if (move.kind != MoveKind.Stone) continue;
    const stone = Record.turnAt(i);

    for (let p of move.pos) {
      [p, out] = boardToViewPos(p);
      if (out) {
        outIndexes.add(p.index());
        continue;
      }

      ctx.fillStyle = stone == Stone.Black ? 'black' : 'white';
      drawCircle(p, stoneRadius);
    }
  }

  // Draw the out-of-view stones on the view border.
  ctx.fillStyle = 'gray';
  outIndexes.forEach(i => drawCircle(Point.fromIndex(i), stoneRadius));

  // Draw the previous move.
  const prevMove = record.prevMove();
  if (prevMove) {
    const prevStone = Record.turnAt(moveIndex - 1);
    switch (prevMove.kind) {
      case MoveKind.Stone:
        ctx.fillStyle = prevStone == Stone.Black ? 'white' : 'black';
        for (let p of prevMove.pos) {
          [p, out] = boardToViewPos(p);
          if (!out) drawCircle(p, dotRadius);
        }
        break;
      case MoveKind.Pass:
        ctx.globalAlpha = PASS_OPACITY;

        const fontSize = size / PASS_FONT_SIZE_RATIO;
        ctx.font = `${fontSize}px sans-serif`;
        ctx.textAlign = 'center';
        ctx.textBaseline = 'middle';

        ctx.fillStyle = prevStone == Stone.Black ? 'black' : 'white';
        ctx.fillText('PASS', size / 2, size / 2);

        ctx.lineWidth = fontSize / PASS_BORDER_RATIO;
        ctx.strokeStyle = prevStone == Stone.Black ? 'white' : 'black';
        ctx.strokeText('PASS', size / 2, size / 2);

        ctx.globalAlpha = 1;
        break;
      case MoveKind.Win:
        // TODO.
        break;
      case MoveKind.Draw:
        // TODO.
        break;
      case MoveKind.Resign:
        // TODO.
        break;
    }
  }

  // Draw the phantom stone.
  if (phantom && ([p, out] = boardToViewPos(phantom), !out)) {
    ctx.globalAlpha = PHANTOM_MOVE_OPACITY;
    ctx.fillStyle = ourStone == Stone.Black ? 'black' : 'white';
    drawCircle(p, stoneRadius);
    ctx.globalAlpha = 1;
  }

  // Draw the tentative stone.
  if (tentative && ([p, out] = boardToViewPos(tentative), !out)) {
    ctx.fillStyle = ourStone == Stone.Black ? 'black' : 'white';
    drawCircle(p, stoneRadius);

    const [x, y] = viewToCanvasPos(p);
    ctx.fillStyle = ourStone == Stone.Black ? 'white' : 'black';
    ctx.fillRect(x - dotRadius, y - dotRadius, dotRadius * 2, dotRadius * 2);
  }

  // Draw the cursor.
  if (cursor && ([p, out] = boardToViewPos(cursor), !out)) {
    const [x, y] = viewToCanvasPos(p);

    ctx.lineWidth = gridSize / CURSOR_LINE_WIDTH_RATIO;

    const offset = gridSize / CURSOR_OFFSET_RATIO;
    const side = gridSize / CURSOR_SIDE_RATIO;
    const inOffset = offset - ctx.lineWidth / 2;
    const outOffset = offset + side;

    ctx.strokeStyle = ourTurn() ? CURSOR_COLOR_ACTIVE : CURSOR_COLOR_INACTIVE;
    ctx.beginPath();
    for (const [dx, dy] of [[1, 1], [1, -1], [-1, -1], [-1, 1]]) {
      ctx.moveTo(x + inOffset * dx, y + offset * dy);
      ctx.lineTo(x + outOffset * dx, y + offset * dy);
      ctx.moveTo(x + offset * dx, y + inOffset * dy);
      ctx.lineTo(x + offset * dx, y + outOffset * dy);
    }
    ctx.stroke();
  }
}

function onContextMenu(e: MouseEvent) {
  e.preventDefault();
  emit('menu');
}

/**
 * Handles `copy` events.
 *
 * Copies the record URI into the clipboard.
 */
function onCopy(e: ClipboardEvent) {
  if (disabled) return;

  const uri = 'c6:' + encodeBase64Url(record.serialize(true)) + ';';
  e.clipboardData!.setData('text/plain', uri);
  // `preventDefault` is required for the change to take effect.
  e.preventDefault();
}

onMounted(() => {
  ctx = canvas.value!.getContext('2d')!;

  resizeCanvas();
  new ResizeObserver(resizeCanvas).observe(canvasContainer.value!);

  window.addEventListener('keydown', onKeyDown);
  window.addEventListener('copy', onCopy);
});

onBeforeUnmount(() => {
  window.removeEventListener('keydown', onKeyDown);
  window.removeEventListener('copy', onCopy);
});
</script>

<template>
  <div id="view-container" ref="canvasContainer">
    <canvas id="view" ref="canvas" @wheel="onWheel" @pointerdown="onPointerDown" @pointerup="onPointerUp"
      @pointerenter="onHover" @pointermove="onHover" @pointerleave="onPointerLeave"
      @contextmenu="onContextMenu"></canvas>
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
  /* FIXME: This does not work correctly with Safari. */
  touch-action: none;
}
</style>
