use crate::{console_log, Event};
use c6ol_core::game::{Move, Point, Record, Stone};
use leptos::{ev, html, prelude::*};
use std::{
    collections::{HashMap, HashSet},
    f64, iter,
};
use web_sys::{
    js_sys::Array, wasm_bindgen::prelude::*, CanvasRenderingContext2d, HtmlCanvasElement,
    KeyboardEvent, MouseEvent, PointerEvent, ResizeObserver, WheelEvent,
};

const BOARD_COLOR: &str = "#ffcc66";
const CURSOR_COLOR_ACTIVE: &str = "darkred";
const CURSOR_COLOR_INACTIVE: &str = "grey";

const DEFAULT_VIEW_SIZE: i16 = 15;

// Divide `gridSize` by the following ratios to get the corresponding lengths.

const LINE_WIDTH_RATIO: f64 = 24.0;
const LINE_DASH_RATIO: f64 = 5.0;

const STONE_RADIUS_RATIO: f64 = 2.25;
const DOT_RADIUS_RATIO: f64 = STONE_RADIUS_RATIO * 6.0;

const CURSOR_LINE_WIDTH_RATIO: f64 = 16.0;
const CURSOR_OFFSET_RATIO: f64 = 8.0;
const CURSOR_SIDE_RATIO: f64 = 4.0;

const PHANTOM_MOVE_OPACITY: f64 = 0.5;

const MOVE_TEXT_WIDTH_RATIO: f64 = 2.0;
const MOVE_TEXT_BORDER_RATIO: f64 = 100.0;
const MOVE_TEXT_OPACITY: f64 = 0.5;

const DIST_FOR_PINCH_ZOOM: f64 = 2.0 * 96.0 / 2.54; // 2cm
const DIST_FOR_SWIPE_RETRACT: f64 = 4.0 * 96.0 / 2.54; // 4cm

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
}

impl From<PointerEvent> for PointerOffsets {
    fn from(e: PointerEvent) -> Self {
        Self {
            id: Some(e.pointer_id()),
            x: e.offset_x(),
            y: e.offset_y(),
        }
    }
}

impl From<MouseEvent> for PointerOffsets {
    fn from(e: MouseEvent) -> Self {
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
    /// Board position the pointer was at when it became active.
    board_pos_on_down: Point,
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
    /// Entered when a swipe retract is triggered.
    ///
    /// A swipe retract may only be triggered when the state is not `Retracted`.
    Retracted,
}

#[derive(Default)]
struct State {
    /// Pixel size of the canvas.
    size: f64,

    /// Pixel size of a single grid on the canvas.
    /// Equals `size / (viewSize + 1)`.
    grid_size: f64,

    /// Size of the view. Minimum value is 1.
    ///
    /// The *view* refers to the area where the user can see and place stones.
    /// Stones outside the view are drawn in gray on its *border*.
    view_size: i16,

    // There are three kinds of positions:
    //
    // - Board position is in grids, relative to the origin of the board.
    // - View position is in grids, relative to the top-left corner of the view.
    // - Canvas position is in pixels, relative to the top-left corner of the canvas.
    //
    // The following are board positions.
    view_center: Point,
    // The user can *hit* a cursor by clicking the view or pressing Space or Enter.
    cursor: Option<Point>,
    phantom: Option<Point>,
    tentative: Option<Point>,

    /// Info about active pointers.
    ///
    /// A pointer is added to this map on a `pointerdown` event,
    /// and removed on a `pointerup` or `pointerleave` event.
    down_pointers: HashMap<i32, Pointer>,
    /// Set as the current `viewSize` when a 2-pointer gesture begins.
    prev_view_size: i16,
    // See comments at `on_hover`.
    last_hover_before_enabled: Option<PointerOffsets>,
    // See comments at `PointerState`.
    pointer_state: PointerState,
}

enum ClampTo {
    Inside,
    InsideAndBorder,
}

impl State {
    /// Tests if a view position is out of view.
    fn view_pos_out_of_view(&self, x: i16, y: i16) -> bool {
        x < 0 || x >= self.view_size || y < 0 || y >= self.view_size
    }

    /// Converts a canvas position to view position, testing if it is out of view.
    fn canvas_to_view_pos(&self, x: i32, y: i32) -> (Point, bool) {
        let x = (x as f64 / self.grid_size).round() as i16 - 1;
        let y = (y as f64 / self.grid_size).round() as i16 - 1;
        (Point { x, y }, self.view_pos_out_of_view(x, y))
    }

    /// Converts a view position to board position.
    fn view_to_board_pos(&self, p: Point) -> Point {
        let x = p.x - self.view_size / 2 + self.view_center.x;
        let y = p.y - self.view_size / 2 + self.view_center.y;
        Point { x, y }
    }

    /// Converts a canvas position to board position, testing if it is out of view.
    fn canvas_to_board_pos(&self, x: i32, y: i32) -> (Point, bool) {
        let (p, out) = self.canvas_to_view_pos(x, y);
        (self.view_to_board_pos(p), out)
    }

    fn board_to_view_pos_unclamped(&self, p: Point) -> (i16, i16) {
        let x = p.x + self.view_size / 2 - self.view_center.x;
        let y = p.y + self.view_size / 2 - self.view_center.y;
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
            ClampTo::Inside => (0, self.view_size - 1),
            ClampTo::InsideAndBorder => (-1, self.view_size),
        };
        (Point::new(x.clamp(min, max), y.clamp(min, max)), out)
    }

    /// Converts a view position to canvas position.
    fn view_to_canvas_pos(&self, p: Point) -> (f64, f64) {
        let x = (p.x + 1) as f64 * self.grid_size;
        let y = (p.y + 1) as f64 * self.grid_size;
        (x, y)
    }
}

fn context_2d(canvas: HtmlCanvasElement) -> CanvasRenderingContext2d {
    canvas
        .get_context("2d")
        .unwrap()
        .unwrap()
        .unchecked_into::<CanvasRenderingContext2d>()
}

#[component]
pub fn GameView(
    record: ReadSignal<Record>,
    stone: ReadSignal<Option<Stone>>,
    disabled: impl Fn() -> bool + Send + Sync + 'static,
    on_event: impl Fn(Event) + Copy + 'static,
) -> impl IntoView {
    let disabled = Memo::new(move |_| disabled());

    let container_ref = NodeRef::<html::Div>::new();
    let canvas_ref = NodeRef::<html::Canvas>::new();

    let state = StoredValue::new(State {
        view_size: DEFAULT_VIEW_SIZE,
        ..Default::default()
    });

    // Tests if it is our turn to play.
    let our_turn = move || {
        let stone = stone.get_untracked();
        stone.is_some() && stone == record.read_untracked().turn()
    };

    // Draws the view.
    let draw = move || {
        console_log!("draw");

        let ctx = context_2d(canvas_ref.get_untracked().unwrap());
        let state = state.read_value();
        let State {
            size, grid_size, ..
        } = *state;

        let set_fill_style_by_stone = |stone: Stone| {
            ctx.set_fill_style_str(match stone {
                Stone::Black => "black",
                Stone::White => "white",
            });
        };

        // Draws a circle at a view position with the given radius.
        let draw_circle = |p: Point, r: f64| {
            let (x, y) = state.view_to_canvas_pos(p);
            ctx.begin_path();
            ctx.arc(x, y, r, 0.0, f64::consts::TAU).unwrap();
            ctx.fill();
        };

        // Draw the board background.
        ctx.set_fill_style_str(BOARD_COLOR);
        ctx.fill_rect(0.0, 0.0, size, size);

        ctx.set_stroke_style_str("black");
        ctx.set_line_width(grid_size / LINE_WIDTH_RATIO);

        // Draw the solid lines inside the view.
        ctx.begin_path();
        for i in 1..=state.view_size {
            let pos = grid_size * i as f64;
            ctx.move_to(grid_size, pos);
            ctx.line_to(size - grid_size, pos);
            ctx.move_to(pos, grid_size);
            ctx.line_to(pos, size - grid_size);
        }
        ctx.stroke();

        let segment = JsValue::from_f64(grid_size / LINE_DASH_RATIO);
        let segments = Array::of2(&segment, &segment);

        // Draw the dashed lines outside the view.
        ctx.begin_path();
        ctx.set_line_dash(&segments).unwrap();
        for i in 1..=state.view_size {
            let pos = grid_size * i as f64;
            ctx.move_to(0.0, pos);
            ctx.line_to(grid_size, pos);
            ctx.move_to(size - grid_size, pos);
            ctx.line_to(size, pos);

            ctx.move_to(pos, 0.0);
            ctx.line_to(pos, grid_size);
            ctx.move_to(pos, size - grid_size);
            ctx.line_to(pos, size);
        }
        ctx.stroke();
        ctx.set_line_dash(&Array::new()).unwrap();

        let record = record.read_untracked();
        let dot_radius = grid_size / DOT_RADIUS_RATIO;

        // Draw the board origin.
        let origin = Point::default();
        if let Some(p) = state.board_to_view_pos(origin) {
            if record.stone_at(origin).is_none() {
                ctx.set_fill_style_str("black");
                draw_circle(p, dot_radius);
            }
        }

        let moves = record.moves();
        let move_index = record.move_index();
        let stone_radius = grid_size / STONE_RADIUS_RATIO;
        // We project the out-of-view stones onto the view border,
        // and stores the resulting positions in this set.
        let mut out_pos = HashSet::new();

        // Draw the stones.
        for (i, &mov) in moves.iter().enumerate().take(move_index) {
            let Move::Stone(fst, snd) = mov else {
                continue;
            };
            let stone = Record::turn_at(i);

            for p in iter::once(fst).chain(snd) {
                let (p, out) = state.board_to_view_pos_clamped(p, ClampTo::InsideAndBorder);
                if out {
                    out_pos.insert(p);
                    continue;
                }

                set_fill_style_by_stone(stone);
                draw_circle(p, stone_radius);
            }
        }

        // Draw the out-of-view stones on the view border.
        ctx.set_fill_style_str("gray");
        for p in out_pos {
            draw_circle(p, stone_radius);
        }

        // Draw the previous move.
        if let Some(mov) = record.prev_move() {
            let stone = Record::turn_at(move_index - 1);
            match mov {
                Move::Stone(fst, snd) => {
                    set_fill_style_by_stone(stone.opposite());
                    for p in iter::once(fst).chain(snd) {
                        let (p, _) = state.board_to_view_pos_clamped(p, ClampTo::InsideAndBorder);
                        draw_circle(p, dot_radius);
                    }
                }
                Move::Win(_) => todo!(),
                Move::Pass | Move::Draw | Move::Resign(_) => {
                    let text = match mov {
                        Move::Pass => "PASS",
                        Move::Draw => "DRAW",
                        Move::Resign(_) => "RESIGN",
                        _ => unreachable!(),
                    };

                    ctx.set_font("10px sans-serif");
                    let actual_width = ctx.measure_text(text).unwrap().width();
                    let expected_width = size / MOVE_TEXT_WIDTH_RATIO;
                    let font_size = expected_width / actual_width * 10.0;

                    ctx.set_font(&format!("{font_size}px sans-serif"));
                    ctx.set_text_align("center");
                    ctx.set_text_baseline("middle");

                    if let Move::Draw = mov {
                        ctx.set_fill_style_str("grey");
                    } else {
                        let stone = match mov {
                            Move::Resign(stone) => stone,
                            _ => stone,
                        };
                        set_fill_style_by_stone(stone);
                    }

                    ctx.set_global_alpha(MOVE_TEXT_OPACITY);

                    ctx.fill_text(text, size / 2.0, size / 2.0).unwrap();
                    if !matches!(mov, Move::Draw) {
                        ctx.set_line_width(font_size / MOVE_TEXT_BORDER_RATIO);
                        ctx.set_stroke_style_str("grey");
                        ctx.stroke_text(text, size / 2.0, size / 2.0).unwrap();
                    }

                    ctx.set_global_alpha(1.0);
                }
            }
        }

        if let Some(stone) = stone.get_untracked() {
            // Draw the phantom stone.
            if let Some(p) = state.phantom.and_then(|p| state.board_to_view_pos(p)) {
                ctx.set_global_alpha(PHANTOM_MOVE_OPACITY);

                set_fill_style_by_stone(stone);
                draw_circle(p, stone_radius);

                ctx.set_global_alpha(1.0);
            }

            // Draw the tentative stone.
            if let Some(p) = state.tentative.and_then(|p| state.board_to_view_pos(p)) {
                set_fill_style_by_stone(stone);
                draw_circle(p, stone_radius);

                let (x, y) = state.view_to_canvas_pos(p);
                set_fill_style_by_stone(stone.opposite());
                ctx.fill_rect(
                    x - dot_radius,
                    y - dot_radius,
                    dot_radius * 2.0,
                    dot_radius * 2.0,
                );
            }
        }

        // Draw the cursor.
        if let Some(p) = state.cursor.and_then(|p| state.board_to_view_pos(p)) {
            let (x, y) = state.view_to_canvas_pos(p);

            let line_width = grid_size / CURSOR_LINE_WIDTH_RATIO;
            ctx.set_line_width(line_width);

            let offset = grid_size / CURSOR_OFFSET_RATIO;
            let side = grid_size / CURSOR_SIDE_RATIO;
            let in_offset = offset - line_width / 2.0;
            let out_offset = offset + side;

            ctx.set_stroke_style_str(if our_turn() {
                CURSOR_COLOR_ACTIVE
            } else {
                CURSOR_COLOR_INACTIVE
            });
            ctx.begin_path();
            for (dx, dy) in [(1, 1), (1, -1), (-1, -1), (-1, 1)] {
                let (dx, dy) = (dx as f64, dy as f64);
                ctx.move_to(x + in_offset * dx, y + offset * dy);
                ctx.line_to(x + out_offset * dx, y + offset * dy);
                ctx.move_to(x + offset * dx, y + in_offset * dy);
                ctx.line_to(x + offset * dx, y + out_offset * dy);
            }
            ctx.stroke();
        }
    };

    // Resizes the canvas to fit its container.
    let resize_canvas = move || {
        let rect = container_ref
            .get_untracked()
            .unwrap()
            .get_bounding_client_rect();
        let size = rect.width().min(rect.height());

        let mut state = state.write_value();
        if size == state.size {
            return;
        }
        state.size = size;
        state.grid_size = size / (state.view_size + 1) as f64;
        drop(state);

        let canvas = canvas_ref.get_untracked().unwrap();
        let size_str = &format!("{size}px")[..];
        canvas.style(("width", size_str));
        canvas.style(("height", size_str));

        let dpr = window().device_pixel_ratio();
        let physical_size = (size * dpr) as u32;
        canvas.set_width(physical_size);
        canvas.set_height(physical_size);

        // See: https://developer.mozilla.org/en-US/docs/Web/API/Window/devicePixelRatio
        context_2d(canvas).scale(dpr, dpr).unwrap();

        draw();
    };

    // We must put this outside `Effect::new` to make the `Closure`
    // live as long as the view. Otherwise, the corresponding JS
    // callback would be invalidated when the `Closure` is dropped.
    let resize_callback = Closure::<dyn Fn()>::new(resize_canvas);

    Effect::new(move || {
        ResizeObserver::new(resize_callback.as_ref().unchecked_ref())
            .unwrap()
            .observe(&container_ref.get_untracked().unwrap());
    });

    // Hits the cursor.
    //
    // Hitting an empty position puts a phantom stone there. Hitting a phantom stone
    // makes it tentative. Hitting a tentative stone makes it phantom. When there are
    // enough tentative stones for this turn, the move is automatically submitted.
    let hit_cursor = move || {
        let mut state = state.write_value();
        let State {
            cursor,
            tentative,
            phantom,
            ..
        } = *state;

        let Some(cursor) = cursor else {
            return;
        };

        if state.board_to_view_pos(cursor).is_none()
            || !our_turn()
            || record.read_untracked().stone_at(cursor).is_some()
        {
            return;
        }

        if tentative == Some(cursor) {
            state.phantom = tentative;
            state.tentative = None;
            drop(state);

            on_event(Event::Tentative(None));
            draw();
        } else if phantom == Some(cursor) {
            if !record.read_untracked().has_past() {
                on_event(Event::Submit(cursor, None));
            } else if let Some(tentative) = tentative {
                on_event(Event::Submit(tentative, Some(cursor)));
            } else {
                state.tentative = phantom;
                state.phantom = None;
                drop(state);

                on_event(Event::Tentative(phantom));
                draw();
            }
        } else {
            state.phantom = Some(cursor);
            drop(state);

            draw();
        }
    };

    // Restricts the cursor to the inside of the view.
    let clamp_cursor = move || {
        let mut state = state.write_value();
        if let Some(cursor) = state.cursor {
            let (p, out) = state.board_to_view_pos_clamped(cursor, ClampTo::Inside);
            if out {
                state.cursor = Some(state.view_to_board_pos(p));
            }
        }
    };

    // Adjusts the view center so that the only active pointer is
    // at the same board position as when it became active.
    //
    // Returns whether the view center is changed.
    let follow_board_pos_on_down = move || {
        let mut state = state.write_value();
        let pointer = state.down_pointers.values().next().unwrap();
        let p0 = pointer.board_pos_on_down;
        let (p, _) = state.canvas_to_board_pos(pointer.last.x, pointer.last.y);

        let (dx, dy) = (p.x - p0.x, p.y - p0.y);
        if dx != 0 || dy != 0 {
            state.view_center.x -= dx;
            state.view_center.y -= dy;
            true
        } else {
            false
        }
    };

    // Moves the cursor to the pointer position, or removes it when out of view.
    let update_cursor = move |po: PointerOffsets, no_draw: bool| {
        let mut state = state.write_value();
        let (p, out) = state.canvas_to_board_pos(po.x, po.y);
        let new_cursor = (!out).then_some(p);

        // Draw if the cursor should appear, move, or disappear.
        let should_draw = !no_draw && new_cursor != state.cursor;
        state.cursor = new_cursor;
        drop(state);

        if should_draw {
            draw();
        }
    };

    enum Zoom {
        Out,
        In,
    }

    // Handles zooming by wheel or keyboard.
    let zoom = move |zoom: Zoom, wheel_event: Option<PointerOffsets>| {
        let mut state = state.write_value();
        match zoom {
            Zoom::Out => state.view_size += 2,
            Zoom::In => {
                if state.view_size == 1 {
                    return;
                }
                state.view_size -= 2;
            }
        }

        state.grid_size = state.size / (state.view_size + 1) as f64;

        // When no pointer is active, zoom at the view center.
        // When exactly one pointer is active, zoom at the pointer position.
        if state.down_pointers.is_empty() {
            drop(state);

            if let Some(e) = wheel_event {
                // Zooming by wheel. Try to keep the cursor at mouse position.
                update_cursor(e, true);
            } else {
                // Zooming by keyboard. Restrict the cursor so that it doesn't go out of view.
                clamp_cursor();
            }
        } else if state.down_pointers.len() == 1 {
            // If the view is pinched, bail out to avoid problems.
            if state.pointer_state > PointerState::Moved {
                return;
            }
            state.pointer_state = PointerState::Moved;
            drop(state);

            follow_board_pos_on_down();
        } else {
            return;
        }
        draw();
    };

    // Handles `keydown` events.
    //
    // - Moves the cursor on W/A/S/D key.
    // - Moves the view center on Arrow Up/Left/Down/Right key.
    // - Zooms out on Minus key.
    // - Zooms in on Plus (Equal) key.
    // - Hits the cursor on Space/Enter key.
    // - Undoes the previous move (if any) on Backspace key.
    // - Redoes the next move (if any) on Shift+Backspace keys.
    // - Jumps to the state before the first move on Home key.
    // - Jumps to the state after the last move on End key.
    let on_keydown = move |ev: KeyboardEvent| {
        if disabled.get_untracked() {
            return;
        }

        let code = ev.code();
        let direction = match &code[..] {
            "Escape" => {
                // Required for the dialog not to close immediately.
                ev.prevent_default();
                return on_event(Event::Menu);
            }
            "KeyW" | "ArrowUp" => 0,
            "KeyA" | "ArrowLeft" => 1,
            "KeyS" | "ArrowDown" => 2,
            "KeyD" | "ArrowRight" => 3,
            "Minus" => return zoom(Zoom::Out, None),
            "Equal" => return zoom(Zoom::In, None),
            "Backspace" => {
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

                let mut state = state.write_value();
                if state.cursor.is_some() {
                    drop(state);

                    return hit_cursor();
                }

                // Put a cursor at the view center if there is no cursor.
                state.cursor = Some(state.view_center);
                drop(state);

                return draw();
            }
            _ => return,
        };

        let mut state = state.write_value();

        // If the view is being dragged or pinched, bail out to avoid problems.
        if !state.down_pointers.is_empty() {
            return;
        }

        const DIRECTION_OFFSETS: [(i16, i16); 4] = [(0, -1), (-1, 0), (0, 1), (1, 0)];

        let (dx, dy) = DIRECTION_OFFSETS[direction as usize];
        if code.starts_with("Key") {
            if let Some(cursor) = &mut state.cursor {
                cursor.x += dx;
                cursor.y += dy;

                // If the cursor is going out of view, adjust the view center to keep up.
                let cursor = *cursor;
                if state.board_to_view_pos(cursor).is_none() {
                    state.view_center.x += dx;
                    state.view_center.y += dy;
                }
            } else {
                // Put a cursor at the view center if there is no cursor.
                state.cursor = Some(state.view_center);
            }
            drop(state);
        } else {
            state.view_center.x += dx;
            state.view_center.y += dy;
            drop(state);

            // Restrict the cursor so that it doesn't go out of view.
            clamp_cursor();
        }
        draw();
    };

    // Handles `wheel` events.
    let on_wheel = move |ev: WheelEvent| {
        zoom(
            if ev.delta_y() > 0.0 {
                Zoom::Out
            } else {
                Zoom::In
            },
            Some(MouseEvent::from(ev).into()),
        );
    };

    // Handles `pointerdown` events.
    let on_pointerdown = move |ev: PointerEvent| {
        let po: PointerOffsets = ev.into();

        let mut state = state.write_value();
        let (p, _) = state.canvas_to_board_pos(po.x, po.y);
        state.down_pointers.insert(
            po.id.unwrap(),
            Pointer {
                down: po,
                last: po,
                board_pos_on_down: p,
            },
        );

        if state.down_pointers.len() == 2 {
            state.prev_view_size = state.view_size;
            state.pointer_state = PointerState::Pinched;
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

        if disabled.get_untracked() || ev.button() != 0 {
            return;
        }

        let (p, out) = state.canvas_to_board_pos(ev.offset_x(), ev.offset_y());
        if !out {
            state.cursor = Some(p);
            drop(state);

            hit_cursor();
        }
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
    let on_hover = move |po: PointerOffsets| {
        let stored_state = state;
        let mut state = state.write_value();
        if disabled.get_untracked() {
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

        // We can also get a `mouseover` event, because Firefox does not fire a
        // `pointerover` event after a dialog is closed.
        if let Some(id) = po.id {
            if let Some(pointer) = state.down_pointers.get_mut(&id) {
                pointer.last = po;
            }
        }

        if state.down_pointers.is_empty() {
            drop(state);

            update_cursor(po, false);
        } else if state.down_pointers.len() == 1 {
            if state.pointer_state > PointerState::Moved {
                return;
            }
            drop(state);

            if follow_board_pos_on_down() {
                stored_state.write_value().pointer_state = PointerState::Moved;
                draw();
            }
        } else if state.down_pointers.len() == 2 {
            let mut iter = state.down_pointers.values();
            let p1 = iter.next().unwrap();
            let p2 = iter.next().unwrap();

            let dist_diff = p1.last.dist(p2.last) - p1.down.dist(p2.down);

            let mut new_view_size =
                state.prev_view_size - (dist_diff / DIST_FOR_PINCH_ZOOM) as i16 * 2;
            if new_view_size < 1 {
                new_view_size = 1;
            }

            if new_view_size != state.view_size {
                state.view_size = new_view_size;
                state.grid_size = state.size / (new_view_size + 1) as f64;
                drop(state);

                draw();
            }
        } else if state.down_pointers.len() == 3 {
            if state.pointer_state == PointerState::Retracted {
                return;
            }

            for p in state.down_pointers.values() {
                if p.last.dist(p.down) < DIST_FOR_SWIPE_RETRACT {
                    return;
                }
            }

            state.pointer_state = PointerState::Retracted;
            if record.read_untracked().has_past() {
                on_event(Event::Undo);
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
        }
        if state.last_hover_before_enabled.and_then(|po| po.id) == po.id {
            state.last_hover_before_enabled = None;
        }
    };

    // Handles `contextmenu` events.
    let on_contextmenu = move |ev: MouseEvent| {
        ev.prevent_default();
        on_event(Event::Menu);
    };

    let handle = window_event_listener(ev::keydown, on_keydown);
    on_cleanup(move || handle.remove());

    Effect::new(move || {
        // Clear phantom and tentative stones if the record changed.
        let mut state = state.write_value();
        state.phantom = None;
        state.tentative = None;

        if record.read().move_index() == 0 && stone.get().is_none() {
            // Clear the cursor if the record and the stone are both reset.
            state.cursor = None;
        }
        drop(state);

        on_event(Event::Tentative(None));
        draw();
    });

    Effect::new(move || {
        if !disabled.get() {
            let mut state = state.write_value();
            if let Some(po) = state.last_hover_before_enabled {
                state.last_hover_before_enabled = None;
                drop(state);

                on_hover(po);
            }
        }
    });

    view! {
        <div id="view-container" node_ref=container_ref>
            <canvas
                id="view"
                node_ref=canvas_ref
                on:wheel=on_wheel
                on:pointerdown=on_pointerdown
                on:pointerup=on_pointerup
                on:pointerover=move |ev| on_hover(ev.into())
                on:pointermove=move |ev| on_hover(ev.into())
                on:mouseover=move |ev| on_hover(ev.into())
                on:pointerleave=move |ev| on_leave(ev.into())
                on:mouseleave=move |ev| on_leave(ev.into())
                on:contextmenu=on_contextmenu
            />
        </div>
    }
}
