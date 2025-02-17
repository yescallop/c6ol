use crate::{Event, WinClaim};
use c6ol_core::game::{Direction, Move, Point, Record, Stone};
use leptos::{either::EitherOf3, ev, html, prelude::*};
use std::{
    collections::{HashMap, HashSet},
    f64::consts::FRAC_PI_4,
    fmt::Write as _,
    iter,
    time::Duration,
};
use tinyvec::ArrayVec;
use web_sys::{
    wasm_bindgen::prelude::*, CanvasRenderingContext2d, HtmlCanvasElement, KeyboardEvent,
    MouseEvent, PointerEvent, WheelEvent,
};

const CURSOR_COLOR_ACTIVE: &str = "firebrick";
const CURSOR_COLOR_INACTIVE: &str = "gray";
const WIN_RING_COLOR: &str = "seagreen";

const DEFAULT_VIEW_SIZE: i16 = 15;

const LINE_WIDTH: f32 = 1.0 / 24.0;
const LINE_DASH: f32 = 1.0 / 5.0;

const STONE_RADIUS: f32 = 1.0 / 2.25;
const DOT_RADIUS: f32 = STONE_RADIUS / 6.0;
const WIN_RING_WIDTH: f32 = STONE_RADIUS / 6.0;

const CURSOR_LINE_WIDTH: f32 = STONE_RADIUS / 6.0;
const CURSOR_SIDE: f32 = 1.0 / 4.25;
const CURSOR_OFFSET: f32 = CURSOR_SIDE / 2.0;

const PHANTOM_MOVE_OPACITY: f64 = 0.5;

const MOVE_TEXT_WIDTH_RATIO: f64 = 2.0;
const MOVE_TEXT_BORDER_RATIO: f64 = 100.0;
const MOVE_TEXT_OPACITY: f64 = 0.5;

const DIST_FOR_PINCH_ZOOM: f64 = 100.0; // ~2.65cm
const DIST_FOR_SWIPE_GESTURE: f64 = 100.0; // ~2.65cm

const LONG_PRESS_MENU_TIMEOUT: Duration = Duration::from_millis(800);

/// Represents `pointerId`, `offsetX` and `offsetY` fields
/// of a `PointerEvent` or `MouseEvent`.
///
/// This is required because those fields can change
/// after an event is handled on Firefox.
#[derive(Clone, Copy)]
struct PointerOffsets {
    id: Option<i32>,
    x: i32,
    y: i32,
}

impl PointerOffsets {
    /// Returns the Euclidean distance between the positions of two pointers.
    fn dist(self, other: Self) -> f64 {
        f64::from(self.x - other.x).hypot(f64::from(self.y - other.y))
    }

    /// Returns the angle from the other pointer to this one.
    fn angle_from(self, other: Self) -> f64 {
        f64::from(self.y - other.y).atan2(f64::from(self.x - other.x))
    }
}

impl From<&PointerEvent> for PointerOffsets {
    fn from(e: &PointerEvent) -> Self {
        Self {
            id: Some(e.pointer_id()),
            x: e.offset_x(),
            y: e.offset_y(),
        }
    }
}

impl From<&MouseEvent> for PointerOffsets {
    fn from(e: &MouseEvent) -> Self {
        Self {
            id: None,
            x: e.offset_x(),
            y: e.offset_y(),
        }
    }
}

struct Pointer {
    /// The `pointerdown` event fired when the pointer became active.
    down: PointerOffsets,
    /// Last event fired about the pointer.
    /// Can be `pointerover`, `pointermove`, or `pointerdown`.
    last: PointerOffsets,
}

#[derive(Clone, Copy, Default, Eq, Ord, PartialEq, PartialOrd)]
enum PointerState {
    /// Default state. Entered when the only active pointer becomes inactive.
    ///
    /// When a `pointerup` event about the only active pointer fires, we try
    /// to hit the cursor only when the previous state is `Calm`.
    #[default]
    Calm,
    /// Entered when the state is `Calm`, exactly one pointer is active, and
    /// the view is dragged by pointer or zoomed by wheel or keyboard.
    ///
    /// The view may be dragged by pointer or zoomed by wheel or keyboard
    /// only when the state is `Calm` or `Moved`.
    Moved,
    /// Entered when exactly one pointer is active,
    /// and a second pointer becomes active.
    Pinched,
    /// Entered when a swipe gesture is triggered.
    ///
    /// A swipe gesture may only be triggered when the state is not `Swiped`.
    Swiped,
}

#[derive(Default)]
struct State {
    /// Info about active pointers.
    ///
    /// A pointer is added to this map on a `pointerdown` event,
    /// and removed on a `pointerup` or `pointerleave` event.
    down_pointers: HashMap<i32, Pointer>,
    /// Average board position the pointers were at when the last one became active.
    board_pos_on_down: Point,
    /// Set as the current `viewSize` when a 2-pointer gesture begins.
    prev_view_size: i16,
    // See comments at `on_hover`.
    last_hover_before_enabled: Option<PointerOffsets>,
    // See comments at `PointerState`.
    pointer_state: PointerState,
    // Open a game menu after a long press of duration `LONG_PRESS_MENU_TIMEOUT`.
    long_press_handle: Option<TimeoutHandle>,
}

impl State {
    fn abort_long_press(&mut self) {
        if let Some(handle) = self.long_press_handle.take() {
            handle.clear();
        }
    }

    fn average_pointer_offsets(&self, f: impl Fn(&Pointer) -> PointerOffsets) -> (i32, i32) {
        let (sum_x, sum_y) = self
            .down_pointers
            .values()
            .fold((0, 0), |(x, y), p| (x + f(p).x, y + f(p).y));
        let len = self.down_pointers.len();
        (sum_x / len as i32, sum_y / len as i32)
    }
}

enum ClampTo {
    Inside,
    InsideAndBorder,
}

struct Calc {
    view_size: i16,
    view_center: Point,
}

impl Calc {
    /// Tests if a view position is out of view.
    fn view_pos_out_of_view(&self, x: i16, y: i16) -> bool {
        x <= 0 || x > self.view_size || y <= 0 || y > self.view_size
    }

    /// Converts a view position to board position.
    fn view_to_board_pos(&self, p: Point) -> Point {
        let x = p.x - 1 - self.view_size / 2 + self.view_center.x;
        let y = p.y - 1 - self.view_size / 2 + self.view_center.y;
        Point { x, y }
    }

    fn board_to_view_pos_unclamped(&self, p: Point) -> (i16, i16) {
        let x = p.x + 1 + self.view_size / 2 - self.view_center.x;
        let y = p.y + 1 + self.view_size / 2 - self.view_center.y;
        (x, y)
    }

    /// Converts a board position to view position, returning `None` if out of view.
    fn board_to_view_pos(&self, p: Point) -> Option<Point> {
        let (x, y) = self.board_to_view_pos_unclamped(p);
        (!self.view_pos_out_of_view(x, y)).then_some(Point { x, y })
    }

    /// Converts a board position to view position, restricting it to the given area.
    fn board_to_view_pos_clamped(&self, p: Point, clamp_to: ClampTo) -> (Point, bool) {
        let (x, y) = self.board_to_view_pos_unclamped(p);
        let out = self.view_pos_out_of_view(x, y);

        let (min, max) = match clamp_to {
            ClampTo::Inside => (1, self.view_size),
            ClampTo::InsideAndBorder => (0, self.view_size + 1),
        };
        (Point::new(x.clamp(min, max), y.clamp(min, max)), out)
    }
}

struct SvgCalc {
    calc: Calc,
    grid_size: f64,
    view_x: f64,
    view_y: f64,
}

impl SvgCalc {
    /// Converts an SVG position to view position, testing if it is out of view.
    fn svg_to_view_pos(&self, x: i32, y: i32) -> (Point, bool) {
        let x = ((x as f64 - self.view_x) / self.grid_size).round() as i16;
        let y = ((y as f64 - self.view_y) / self.grid_size).round() as i16;
        (Point { x, y }, self.calc.view_pos_out_of_view(x, y))
    }

    /// Converts an SVG position to board position, testing if it is out of view.
    fn svg_to_board_pos(&self, x: i32, y: i32) -> (Point, bool) {
        let (p, out) = self.svg_to_view_pos(x, y);
        (self.calc.view_to_board_pos(p), out)
    }
}

fn canvas_context_2d() -> CanvasRenderingContext2d {
    document()
        .create_element("canvas")
        .unwrap()
        .unchecked_into::<HtmlCanvasElement>()
        .get_context("2d")
        .unwrap()
        .unwrap()
        .unchecked_into::<CanvasRenderingContext2d>()
}

fn stone_fill(stone: Stone) -> &'static str {
    match stone {
        Stone::Black => "black",
        Stone::White => "white",
    }
}

/// The game view component.
///
/// There are three kinds of positions:
///
/// - Board position is in grids, relative to the origin of the board.
/// - View position is in grids, relative to the top-left corner of the view.
/// - SVG position is in pixels, relative to the top-left corner of the SVG element.
///
/// All `Point`s in the props are board positions.
#[component]
pub fn GameView(
    record: RwSignal<Record>,
    stone: ReadSignal<Option<Stone>>,
    disabled: impl Fn() -> bool + Send + Sync + 'static,
    pending: impl Fn() -> bool + Send + Sync + 'static,
    phantom_disabled: impl Fn() -> bool + Copy + 'static,
    on_event: impl Fn(Event) + Copy + 'static,
    /// Size of the view.
    ///
    /// Defaults to 15. Minimum value is 1. Is always odd.
    ///
    /// The *view* refers to the area where the user can see and place stones.
    /// Stones outside the view are drawn in gray on its *border*.
    #[prop(default = RwSignal::new(DEFAULT_VIEW_SIZE))]
    view_size: RwSignal<i16>,
    #[prop(optional)] view_center: RwSignal<Point>,
    #[prop(optional)] cursor_pos: RwSignal<Option<Point>>,
    #[prop(optional)] phantom_pos: RwSignal<Option<Point>>,
    #[prop(optional)] tentatives_pos: RwSignal<ArrayVec<[Point; 2]>>,
    #[prop(optional)] win_claim: RwSignal<Option<WinClaim>>,
) -> impl IntoView {
    let disabled = Memo::new(move |_| disabled());
    let pending = Memo::new(move |_| pending());

    let container_ref = NodeRef::<html::Div>::new();

    // Non-reactive state.
    let state = StoredValue::<State>::default();

    // Creates a view-board position calculator.
    let calc = move || Calc {
        view_size: view_size.get(),
        view_center: view_center.get(),
    };

    // Creates an SVG-view-board position calculator.
    let svg_calc = move || {
        let rect = container_ref.get().unwrap().get_bounding_client_rect();
        let w = rect.width();
        let h = rect.height();

        let calc = calc();
        let s = (calc.view_size + 1) as f64;

        let (grid_size, view_x, view_y) = if w > h {
            (h / s, (w - h) / 2.0, 0.0)
        } else {
            (w / s, 0.0, (h - w) / 2.0)
        };
        SvgCalc {
            calc,
            grid_size,
            view_x,
            view_y,
        }
    };

    // Tests if it is our turn to play.
    let our_turn = move || {
        let stone = stone.get();
        stone.is_some() && stone == record.read().turn()
    };

    // Hits the cursor.
    //
    // Hitting an empty position puts a phantom stone there if there are not
    // enough tentative stones for this turn. Hitting a phantom stone makes
    // it tentative. Hitting a tentative stone makes it phantom. When there
    // are enough tentative stones, the move is automatically submitted.
    let hit_cursor = move |cursor: Point| {
        let phantom = phantom_pos.get();
        let mut tentatives = tentatives_pos.get();

        if calc().board_to_view_pos(cursor).is_none() {
            return;
        }

        if let Some(claim) = win_claim.get() {
            let Some(stone) = stone.get() else {
                return;
            };

            let new_claim = match claim {
                WinClaim::PendingPoint | WinClaim::Ready(..) => WinClaim::PendingDirection(cursor),
                WinClaim::PendingDirection(p) => {
                    let Some(dir) = Direction::from_unit_vec(
                        (cursor.x - p.x).signum(),
                        (cursor.y - p.y).signum(),
                    ) else {
                        return;
                    };

                    if record
                        .write_untracked()
                        .with_temp_placements(stone, &tentatives, |record| {
                            record.test_winning_row(p, dir) == Some(cursor)
                        })
                    {
                        WinClaim::Ready(p, dir)
                    } else {
                        WinClaim::PendingDirection(cursor)
                    }
                }
            };
            win_claim.set(Some(new_claim));

            if let WinClaim::Ready(..) = new_claim {
                on_event(Event::Submit);
            }
            return;
        }

        if !our_turn() || record.read().stone_at(cursor).is_some() {
            return;
        }

        if phantom_disabled() {
            if let Some(i) = tentatives.iter().position(|&p| p == cursor) {
                tentatives.remove(i);
                tentatives_pos.set(tentatives);
            } else if tentatives.len() < record.read().max_stones_to_play() {
                tentatives.push(cursor);
                tentatives_pos.set(tentatives);

                if tentatives.len() == record.read().max_stones_to_play() {
                    on_event(Event::Submit);
                }
            }
        } else if let Some(i) = tentatives.iter().position(|&p| p == cursor) {
            phantom_pos.set(Some(tentatives.remove(i)));
            tentatives_pos.set(tentatives);
        } else if phantom == Some(cursor) {
            phantom_pos.set(None);
            tentatives.push(cursor);
            tentatives_pos.set(tentatives);

            if tentatives.len() == record.read().max_stones_to_play() {
                on_event(Event::Submit);
            }
        } else if tentatives.len() < record.read().max_stones_to_play() {
            phantom_pos.set(Some(cursor));
        }
    };

    // Restricts the cursor to the inside of the view.
    let clamp_cursor = move || {
        if let Some(cursor) = cursor_pos.get() {
            let calc = calc();
            let (p, out) = calc.board_to_view_pos_clamped(cursor, ClampTo::Inside);
            if out {
                cursor_pos.set(Some(calc.view_to_board_pos(p)));
            }
        }
    };

    // Adjusts the view center so that the only active pointer is
    // at the same board position as when it became active.
    //
    // Returns whether the view center is changed.
    let follow_board_pos_on_down = move |state: &State, dry_run: bool| {
        let p0 = state.board_pos_on_down;
        let (avg_x, avg_y) = state.average_pointer_offsets(|p| p.last);
        let (p, _) = svg_calc().svg_to_board_pos(avg_x, avg_y);

        let (dx, dy) = (p.x - p0.x, p.y - p0.y);
        if dx != 0 || dy != 0 {
            if !dry_run {
                view_center.update(|p| {
                    p.x -= dx;
                    p.y -= dy;
                });
            }
            true
        } else {
            false
        }
    };

    // Moves the cursor to the pointer position, or removes it when out of view.
    //
    // Returns the new cursor position.
    let update_cursor = move |po: PointerOffsets, dry_run: bool| {
        let (p, out) = svg_calc().svg_to_board_pos(po.x, po.y);
        let new_cursor = (!out).then_some(p);

        if !dry_run && new_cursor != cursor_pos.get() {
            cursor_pos.set(new_cursor);
        }
        new_cursor
    };

    enum Zoom {
        Out,
        In,
    }

    // Handles zooming by wheel or keyboard.
    let zoom = move |zoom: Zoom, wheel_event: Option<PointerOffsets>| {
        let old_view_size = view_size.get();
        match zoom {
            Zoom::Out => view_size.set(old_view_size + 2),
            Zoom::In => {
                if old_view_size == 1 {
                    return;
                }
                view_size.set(old_view_size - 2);
            }
        }

        let mut state = state.write_value();

        // When no pointer is active, zoom at the view center.
        // When exactly one pointer is active, zoom at the pointer position.
        if state.down_pointers.is_empty() {
            if let Some(po) = wheel_event {
                // Zooming by wheel. Try to keep the cursor at mouse position.
                update_cursor(po, false);
            } else {
                // Zooming by keyboard. Restrict the cursor so that it doesn't go out of view.
                clamp_cursor();
            }
        } else if state.down_pointers.len() == 1 {
            // If the view is pinched, bail out to avoid problems.
            if state.pointer_state > PointerState::Moved {
                return;
            }
            follow_board_pos_on_down(&state, false);
            state.pointer_state = PointerState::Moved;
        }
    };

    // Handles `keydown` events.
    //
    // - Moves the cursor on W/A/S/D.
    // - Moves the view center on Arrow Up/Left/Down/Right.
    // - Zooms out on Minus.
    // - Zooms in on Plus (Equal).
    // - Hits the cursor on Space/Enter.
    // - Undoes the previous move (if any) on Backspace or Ctrl+Z.
    // - Redoes the next move (if any) on Shift+Backspace or Ctrl+Shift+Z.
    // - Jumps to the state before the first move on Home.
    // - Jumps to the state after the last move on End.
    let on_keydown = move |ev: KeyboardEvent| {
        if disabled.get() {
            return;
        }

        let code = ev.code();
        let direction = match &code[..] {
            "Escape" => {
                // Required for the dialog not to close immediately.
                ev.prevent_default();

                if ev.repeat() {
                    return;
                }

                state.write_value().abort_long_press();
                return on_event(Event::Menu);
            }
            "KeyW" | "ArrowUp" => 0,
            "KeyA" | "ArrowLeft" => 1,
            "KeyS" | "ArrowDown" => 2,
            "KeyD" | "ArrowRight" => 3,
            "Minus" => return zoom(Zoom::Out, None),
            "Equal" => return zoom(Zoom::In, None),
            "Backspace" | "KeyZ" => {
                if code == "KeyZ" && !ev.ctrl_key() {
                    return;
                }
                return on_event(if ev.shift_key() {
                    Event::Redo
                } else {
                    Event::Undo
                });
            }
            "Home" => return on_event(Event::Home),
            "End" => return on_event(Event::End),
            "Enter" | "Space" => {
                // Required for the dialog not to close immediately.
                ev.prevent_default();

                if ev.repeat() {
                    return;
                }

                if let Some(cursor) = cursor_pos.get() {
                    return hit_cursor(cursor);
                }

                // Put a cursor at the view center if there is no cursor.
                cursor_pos.set(Some(view_center.get()));
                return;
            }
            _ => return,
        };

        let state = state.read_value();

        // If the view is being dragged or pinched, bail out to avoid problems.
        if !state.down_pointers.is_empty() {
            return;
        }

        const DIRECTION_OFFSETS: [(i16, i16); 4] = [(0, -1), (-1, 0), (0, 1), (1, 0)];

        let (dx, dy) = DIRECTION_OFFSETS[direction as usize];
        if code.starts_with("Key") {
            if let Some(mut cursor) = cursor_pos.get() {
                cursor.x += dx;
                cursor.y += dy;
                cursor_pos.set(Some(cursor));

                // If the cursor is going out of view, adjust the view center to keep up.
                if calc().board_to_view_pos(cursor).is_none() {
                    view_center.update(|p| {
                        p.x += dx;
                        p.y += dy;
                    });
                }
            } else {
                // Put a cursor at the view center if there is no cursor.
                cursor_pos.set(Some(view_center.get()));
            }
        } else {
            view_center.update(|p| {
                p.x += dx;
                p.y += dy;
            });

            // Restrict the cursor so that it doesn't go out of view.
            clamp_cursor();
        }
    };

    // Handles `wheel` events.
    let on_wheel = move |ev: WheelEvent| {
        zoom(
            if ev.delta_y() > 0.0 {
                Zoom::Out
            } else {
                Zoom::In
            },
            Some((&MouseEvent::from(ev)).into()),
        );
        state.write_value().abort_long_press();
    };

    // Handles `pointerdown` events.
    let on_pointerdown = move |ev: PointerEvent| {
        let po: PointerOffsets = (&ev).into();

        let mut state = state.write_value();

        state
            .down_pointers
            .insert(po.id.unwrap(), Pointer { down: po, last: po });

        if state.down_pointers.len() <= 2 {
            let (avg_x, avg_y) = state.average_pointer_offsets(|p| p.down);
            (state.board_pos_on_down, _) = svg_calc().svg_to_board_pos(avg_x, avg_y);
        }

        if state.down_pointers.len() == 1 {
            if ev.pointer_type() != "touch" {
                return;
            }

            let handle =
                set_timeout_with_handle(move || on_event(Event::Menu), LONG_PRESS_MENU_TIMEOUT)
                    .unwrap();
            state.long_press_handle = Some(handle);
        } else if state.down_pointers.len() == 2 {
            state.prev_view_size = view_size.get();
            state.pointer_state = PointerState::Pinched;
            if cursor_pos.get().is_some() {
                cursor_pos.set(None);
            }
            state.abort_long_press();
        }
    };

    // Handles `pointerup` events.
    //
    // Attempts to hit the cursor when the pointer is the only active one,
    // the view isn't ever dragged, zoomed, or pinched since the pointer
    // became active, the view isn't disabled, and the main button is pressed.
    let on_pointerup = move |ev: PointerEvent| {
        let mut state = state.write_value();
        if state.down_pointers.remove(&ev.pointer_id()).is_none() {
            // Bail out if the pointer is already inactive due to a `pointerleave` event.
            return;
        }
        if !state.down_pointers.is_empty() {
            return;
        }
        if state.pointer_state != PointerState::Calm {
            state.pointer_state = PointerState::Calm;
            return;
        }
        if disabled.get() || ev.button() != 0 {
            return;
        }

        if let Some(cursor) = update_cursor((&ev).into(), ev.pointer_type() == "touch") {
            hit_cursor(cursor);
        }
        state.abort_long_press();
    };

    // Handles `pointerover`, `pointermove` and `mouseover` events.
    //
    // Performs different actions according to the number of active pointers:
    //
    // - 0: Updates the cursor.
    // - 1: Drags the view if it isn't ever pinched since the pointer became active.
    // - 2: Roughly speaking, whenever the distance of pointers increases (decreases)
    //      by `DIST_FOR_PINCH_ZOOM`, `viewSize` will be decreased (increased) by 2.
    // - 3: Retracts the previous move if all pointers have moved for at least
    //      a distance of `DIST_FOR_SWIPE_RETRACT`.
    let on_hover = move |po: PointerOffsets, kind: &str| {
        let mut state = state.write_value();
        if disabled.get() {
            // We can reach here for either of the following reasons:
            // - A dialog was closed with a pointer which then entered the view,
            //   firing this event before the `close` event.
            // - A game menu was opened by touch, but a glitch keeps the browser
            //   firing pointer events on the view until the touch ends.
            //
            // Either way, we record this event. We are either to clear it when a
            // corresponding `pointerleave` event is fired, or replay it after the
            // view is enabled. The cursor will be updated only in the former case
            // if no new dialog was opened as soon as the previous one was closed.
            state.last_hover_before_enabled = Some(po);
            return;
        }

        if let Some(id) = po.id {
            if let Some(pointer) = state.down_pointers.get_mut(&id) {
                pointer.last = po;
            }
        } else {
            // Firefox does not fire a `pointerover` event after a dialog is closed,
            // so we accept `mouseover` as a replacement. We bail out here, because
            // it is spuriously fired on Chrome when touching the screen on startup
            // or after a dialog is closed.
            return;
        }

        if state.down_pointers.is_empty() {
            update_cursor(po, false);
        } else if state.down_pointers.len() == 1 {
            if state.pointer_state > PointerState::Moved {
                return;
            }

            // Avoid accidental dragging on touchscreen devices.
            // Only allow dragging with two pointers, following the midpoint instead.
            if follow_board_pos_on_down(&state, kind == "touch") {
                state.pointer_state = PointerState::Moved;
                state.abort_long_press();

                if kind == "touch" && cursor_pos.get().is_some() {
                    cursor_pos.set(None);
                }
            }

            if kind == "touch" {
                if state.pointer_state == PointerState::Swiped {
                    return;
                }

                let p = state.down_pointers.values().next().unwrap();
                if p.last.dist(p.down) < DIST_FOR_SWIPE_GESTURE {
                    return;
                }

                let angle = p.last.angle_from(p.down);

                state.pointer_state = PointerState::Swiped;

                if !(-3.0 * FRAC_PI_4..3.0 * FRAC_PI_4).contains(&angle) {
                    on_event(Event::Undo);
                } else if (-FRAC_PI_4..FRAC_PI_4).contains(&angle) {
                    on_event(Event::Redo);
                }
            }
        } else if state.down_pointers.len() == 2 {
            if state.pointer_state > PointerState::Pinched {
                return;
            }

            let mut iter = state.down_pointers.values();
            let p1 = iter.next().unwrap();
            let p2 = iter.next().unwrap();

            let dist_diff = p1.last.dist(p2.last) - p1.down.dist(p2.down);

            let mut new_view_size =
                state.prev_view_size - (dist_diff / DIST_FOR_PINCH_ZOOM) as i16 * 2;
            if new_view_size < 1 {
                new_view_size = 1;
            }

            if new_view_size != view_size.get() {
                view_size.set(new_view_size);
            }

            if kind == "touch" {
                follow_board_pos_on_down(&state, false);
            }
        }
    };

    // Handles `pointerleave` and `mouseleave` events.
    let on_leave = move |po: PointerOffsets| {
        let mut state = state.write_value();
        // We can also get a `mouseleave` event on Firefox (see above).
        if let Some(id) = po.id {
            state.down_pointers.remove(&id);
        }
        if state.down_pointers.is_empty() {
            state.pointer_state = PointerState::Calm;
            state.abort_long_press();
        }
        if state.last_hover_before_enabled.and_then(|po| po.id) == po.id {
            state.last_hover_before_enabled = None;
        }
        if cursor_pos.get().is_some() {
            cursor_pos.set(None);
        }
    };

    // Replay the recorded hover event (if any) after the view is enabled.
    Effect::new(move || {
        if !disabled.get() {
            if let Some(po) = state.write_value().last_hover_before_enabled.take() {
                update_cursor(po, false);
            }
        }
    });

    Effect::new(move || {
        record.track();
        stone.track();

        // Clear phantom, tentatives and win claim if the record or the stone changed.
        phantom_pos.set(None);
        tentatives_pos.set(ArrayVec::new());
        win_claim.set(None);
    });

    let handle = window_event_listener(ev::keydown, on_keydown);
    on_cleanup(move || handle.remove());

    let stones = move || {
        let record = record.read();
        let moves = record.moves();
        let move_index = record.move_index();

        let mut blacks = vec![];
        let mut whites = vec![];
        let mut grays = vec![];

        let calc = calc();
        let mut out_pos = HashSet::new();

        for (i, &mov) in moves.iter().enumerate().take(move_index) {
            let Move::Place(p1, p2) = mov else {
                continue;
            };
            let stone = Record::turn_at(i);

            for p in iter::once(p1).chain(p2) {
                let (p, out) = calc.board_to_view_pos_clamped(p, ClampTo::InsideAndBorder);
                if out && !out_pos.insert(p) {
                    continue;
                }

                let group = if out {
                    &mut grays
                } else if stone == Stone::Black {
                    &mut blacks
                } else {
                    &mut whites
                };
                group.push(view! { <circle cx=p.x cy=p.y r=STONE_RADIUS /> });
            }
        }

        view! {
            <g fill="black">{blacks}</g>
            <g fill="white">{whites}</g>
            <g fill="gray">{grays}</g>
        }
    };

    let win_rings = move |p: Point, dir: Option<Direction>, color: &'static str| {
        let calc = calc();
        let rings = iter::once(p)
            .chain(dir.iter().flat_map(|&d| p.adjacent_iter(d).take(5)))
            .filter_map(|p| calc.board_to_view_pos(p))
            .map(|p| view! { <circle cx=p.x cy=p.y r=STONE_RADIUS - WIN_RING_WIDTH / 2.0 /> })
            .collect::<Vec<_>>();
        view! {
            <g stroke=color stroke-width=WIN_RING_WIDTH>
                {rings}
            </g>
        }
    };

    let centered_text = move |text: &'static str, fill: &'static str| {
        let size = view_size.get() + 1;

        let ctx = canvas_context_2d();
        ctx.set_font("10px sans-serif");
        let actual_width = ctx.measure_text(text).unwrap().width();
        let expected_width = size as f64 / MOVE_TEXT_WIDTH_RATIO;
        let font_size = expected_width / actual_width * 10.0;

        view! {
            <text
                x=size / 2
                y=size / 2
                font-size=font_size as f32
                font-family="sans-serif"
                text-anchor="middle"
                dominant-baseline="middle"
                fill=fill
                fill-opacity=MOVE_TEXT_OPACITY
                stroke="gray"
                stroke-width=(font_size / MOVE_TEXT_BORDER_RATIO) as f32
                stroke-opacity=MOVE_TEXT_OPACITY
            >
                {text}
            </text>
        }
    };

    let previous_move = move || {
        let record = record.read();
        let Some(mov) = record.prev_move() else {
            return EitherOf3::A(vec![]);
        };

        let stone = Record::turn_at(record.move_index() - 1);
        match mov {
            Move::Place(p1, p2) => {
                let calc = calc();
                let circles = iter::once(p1)
                    .chain(p2)
                    .map(|p| {
                        let (p, _) = calc.board_to_view_pos_clamped(p, ClampTo::InsideAndBorder);
                        view! { <circle cx=p.x cy=p.y r=DOT_RADIUS fill=stone_fill(stone.opposite()) /> }
                    })
                    .collect();
                EitherOf3::A(circles)
            }
            Move::Win(p, dir) => EitherOf3::B(win_rings(p, Some(dir), WIN_RING_COLOR)),
            Move::Pass | Move::Draw | Move::Resign(_) => {
                let text = match mov {
                    Move::Pass => "PASS",
                    Move::Draw => "DRAW",
                    Move::Resign(_) => "RESIGN",
                    _ => unreachable!(),
                };

                let fill = if let Move::Draw = mov {
                    "gray"
                } else {
                    stone_fill(match mov {
                        Move::Resign(stone) => stone,
                        _ => stone,
                    })
                };

                EitherOf3::C(centered_text(text, fill))
            }
        }
    };

    let phantom_stone = move || {
        let stone = stone.get()?;
        let p = calc().board_to_view_pos(phantom_pos.get()?)?;

        let circle = view! {
            <circle
                cx=p.x
                cy=p.y
                r=STONE_RADIUS
                fill=stone_fill(stone)
                fill-opacity=PHANTOM_MOVE_OPACITY
            />
        };
        Some(circle)
    };

    let tentative_stones = move || {
        let Some(stone) = stone.get() else {
            return vec![];
        };
        let calc = calc();

        tentatives_pos
            .get()
            .iter()
            .filter_map(|&p| calc.board_to_view_pos(p))
            .map(|p| {
                view! {
                    <circle cx=p.x cy=p.y r=STONE_RADIUS fill=stone_fill(stone) />
                    <rect
                        x=p.x as f32 - DOT_RADIUS
                        y=p.y as f32 - DOT_RADIUS
                        width=DOT_RADIUS * 2.0
                        height=DOT_RADIUS * 2.0
                        fill="gray"
                    />
                }
            })
            .collect()
    };

    let cursor = move || {
        let p = calc().board_to_view_pos(cursor_pos.get()?)?;
        let mut d = String::new();

        for (dx, dy) in [(1.0, 1.0), (1.0, -1.0), (-1.0, -1.0), (-1.0, 1.0)] {
            let x = p.x as f32 + (CURSOR_OFFSET + CURSOR_SIDE) * dx;
            let y = p.y as f32 + CURSOR_OFFSET * dy;
            let dx = -CURSOR_SIDE * dx;
            let dy = CURSOR_SIDE * dy;
            write!(d, "M{x} {y}h{dx}v{dy}").unwrap();
        }

        let stroke = if our_turn() {
            CURSOR_COLOR_ACTIVE
        } else {
            CURSOR_COLOR_INACTIVE
        };
        Some(view! { <path stroke=stroke stroke-width=CURSOR_LINE_WIDTH d=d /> })
    };

    view! {
        <div
            class="view-container"
            node_ref=container_ref
            on:wheel=on_wheel
            on:pointerdown=on_pointerdown
            on:pointerup=on_pointerup
            on:pointermove=move |ev| on_hover((&ev).into(), &ev.pointer_type())
            on:mouseover=move |ev| on_hover((&ev).into(), "mouse")
            on:pointerleave=move |ev| on_leave((&ev).into())
            on:mouseleave=move |ev| on_leave((&ev).into())
            // Avoid touching a dialog opened by the same touch.
            on:touchend=move |ev| {
                // To avoid the error after pinching:
                // [Intervention] Ignored attempt to cancel a touchend event with
                // cancelable=false, for example because scrolling is in progress
                // and cannot be interrupted.
                if ev.cancelable() {
                    ev.prevent_default();
                }
            }
            on:keydown=on_keydown
            on:contextmenu=move |ev| {
                ev.prevent_default();
                state.write_value().abort_long_press();
                on_event(Event::Menu);
            }
        >
            <svg
                class="view"
                viewBox=move || format!("0 0 {s} {s}", s = view_size.get() + 1)
                fill="none"
            >
                // Draw the grids.
                <g stroke="black" stroke-width=LINE_WIDTH>
                    // Draw the solid lines inside the view.
                    <path d=move || {
                        let mut d = String::new();
                        let s = view_size.get();
                        for i in 1..=s {
                            write!(d, "M1 {i}H{s}M{i} 1V{s}").unwrap();
                        }
                        d
                    } />
                    // Draw the dashed lines outside the view.
                    <path
                        stroke-dasharray=LINE_DASH
                        d=move || {
                            let mut d = String::new();
                            let s = view_size.get();
                            for i in 1..=s {
                                write!(d, "M0 {i}h1M{s} {i}h1M{i} 0v1M{i} {s}v1").unwrap();
                            }
                            d
                        }
                    />
                </g>
                // Draw the board origin.
                {move || {
                    let p = calc().board_to_view_pos(Point::ORIGIN)?;
                    if record.read().stone_at(Point::ORIGIN).is_none() {
                        Some(view! { <circle cx=p.x cy=p.y r=DOT_RADIUS fill="black" /> })
                    } else {
                        None
                    }
                }}
                // Draw the stones.
                {stones}
                // Draw the previous move.
                {previous_move}
                // Draw the pending text.
                {move || { pending.get().then(|| centered_text("PENDING", "gray")) }}
                // Draw the phantom stone.
                {phantom_stone}
                // Draw the tentative stones.
                {tentative_stones}
                // Draw the win claim.
                {move || match win_claim.get()? {
                    WinClaim::PendingPoint => None,
                    WinClaim::PendingDirection(p) => Some(win_rings(p, None, "gray")),
                    WinClaim::Ready(p, dir) => Some(win_rings(p, Some(dir), "gray")),
                }}
                // Draw the cursor.
                {cursor}
            </svg>
        </div>
    }
}
