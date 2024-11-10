//! The client library for [Connect6 Online](https://github.com/yescallop/c6ol).

mod dialog;
mod game_view;

use base64::{prelude::BASE64_STANDARD, Engine};
use c6ol_core::{
    game::{Move, Point, Record, Stone},
    protocol::{ClientMessage, Request, ServerMessage},
};
use dialog::*;
use leptos::{ev, prelude::*};
use std::{
    collections::HashMap,
    sync::atomic::{AtomicU32, Ordering},
};
use web_sys::{
    js_sys::{ArrayBuffer, Uint8Array},
    wasm_bindgen::prelude::*,
    BinaryType, CloseEvent, MessageEvent, Storage, WebSocket,
};

macro_rules! console_log {
    ($($t:tt)*) => {
        web_sys::console::log_1(
            &web_sys::wasm_bindgen::JsValue::from_str(&format_args!($($t)*).to_string())
        )
    };
}
pub(crate) use console_log;

#[derive(Clone, Copy)]
enum Action {
    MainMenu,
    Submit,
    Pass,
    PlaceSingleStone,
    Request(Request),
    Accept(Request),
    Resign,
}

enum Event {
    Menu,
    Submit(Point, Option<Point>),
    Undo,
    Redo,
    Home,
    End,
}

const STORAGE_KEY_RECORD: &str = "record";
const ANALYZE_PREFIX: &str = "analyze,";

#[derive(Clone)]
struct DialogEntry {
    id: u32,
    dialog: Dialog,
}

#[expect(dead_code)]
struct WebSocketState {
    ws: WebSocket,
    onopen: Closure<dyn FnMut()>,
    onclose: Closure<dyn Fn(CloseEvent)>,
    onmessage: Closure<dyn Fn(MessageEvent)>,
}

const CLOSE_CODE_ABNORMAL: u16 = 1006;
const CLOSE_CODE_POLICY: u16 = 1008;

fn local_storage() -> Storage {
    window().local_storage().unwrap().unwrap()
}

fn history_push_state(url: &str) {
    let history = window().history().unwrap();
    history
        .push_state_with_url(&JsValue::UNDEFINED, "", Some(url))
        .unwrap();
}

/// Entry-point for the app.
#[component]
pub fn App() -> impl IntoView {
    let record = RwSignal::new(Record::new());
    let stone = RwSignal::new(None::<Stone>);

    let game_id = RwSignal::new(String::new());
    let requests = RwSignal::new([None::<Stone>; Request::VALUES.len()]);

    let request = move |req: Request| requests.read_untracked()[req as usize];

    let dialogs = RwSignal::new(Vec::<DialogEntry>::new());

    let show_dialog = move |dialog: Dialog| {
        static NEXT_ID: AtomicU32 = AtomicU32::new(0);

        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        dialogs.write().push(DialogEntry { id, dialog });
        id
    };

    let confirm_callbacks =
        StoredValue::new(HashMap::<u32, Box<dyn FnOnce() + Send + Sync>>::new());

    let confirm = move |action: Action, callback: Box<dyn FnOnce() + Send + Sync>| {
        let id = show_dialog(Dialog::from(ConfirmDialog { action }));
        confirm_callbacks.write_value().insert(id, callback);
    };

    let ws_state = StoredValue::new_local(None::<WebSocketState>);

    let online = move || ws_state.read_value().is_some();

    Effect::new(move || {
        // Watch for changes to the record.
        let record = record.read();
        let game_id = &*game_id.read_untracked();

        if game_id == "local" {
            // Save the record to local storage.
            let mut buf = vec![];
            record.encode(&mut buf, true);
            let buf = BASE64_STANDARD.encode(buf);
            local_storage().set_item(STORAGE_KEY_RECORD, &buf).unwrap();
        }

        // FIXME: Maybe we shouldn't write to signals in an effect.
        // But it's just easier to reason about.
        if !online() && !game_id.is_empty() {
            // Update the stone accordingly when offline.
            stone.set(record.turn());
        }
    });

    // Sends the message on the WebSocket connection.
    let send = move |msg: ClientMessage| {
        if let Some(ws_state) = &*ws_state.read_value() {
            if ws_state.ws.ready_state() == WebSocket::OPEN {
                ws_state.ws.send_with_u8_array(&msg.encode()).unwrap();
                return;
            }
        }
        show_dialog(Dialog::from(ErrorDialog {
            message: "Connection is not open.".into(),
        }));
    };

    let on_close = move |ev: CloseEvent| {
        let code = ev.code();
        let mut reason = ev.reason();

        if reason.is_empty() {
            if code == CLOSE_CODE_ABNORMAL {
                reason = "Closed abnormally.".into();
            } else {
                reason = format!("Closed with code {code}.");
            }
        }
        show_dialog(Dialog::from(ConnClosedDialog { reason }));
    };

    let confirm_request = move |req: Request| {
        let requested = request(req).is_some();
        let action = if requested {
            Action::Accept(req)
        } else {
            Action::Request(req)
        };
        confirm(action, Box::new(move || send(ClientMessage::Request(req))));
    };

    let show_game_menu_dialog = move || {
        show_dialog(Dialog::from(GameMenuDialog {
            game_id: game_id.get_untracked(),
            stone: stone.get_untracked(),
            online: online(),
            record: record.read_only(),
            requests: requests.read_only(),
        }));
    };

    let first_msg_seen = StoredValue::new(false);

    let on_message = move |ev: MessageEvent| {
        let Some(msg) = ev
            .data()
            .dyn_ref::<ArrayBuffer>()
            .map(|buf| Uint8Array::new(buf).to_vec())
            .and_then(|buf| ServerMessage::decode(&buf))
        else {
            let ws_state = ws_state.read_value();
            let ws = &ws_state.as_ref().unwrap().ws;
            ws.close_with_code_and_reason(CLOSE_CODE_POLICY, "Malformed server message.")
                .unwrap();
            return;
        };

        let mut record_changed = false;
        match msg {
            ServerMessage::Started(our_stone, new_game_id) => {
                stone.set(Some(our_stone));
                if let Some(id) = new_game_id {
                    let id = String::from_utf8_lossy(&id).into_owned();
                    game_id.set(id.clone());

                    history_push_state(&format!("#{id}"));

                    show_game_menu_dialog();
                }
                for req in Request::VALUES {
                    if request(req) == Some(our_stone.opposite()) {
                        confirm_request(req);
                    }
                }
            }
            ServerMessage::Record(new_record) => {
                record.set(*new_record);
                if !first_msg_seen.get_value() {
                    show_dialog(Dialog::from(JoinDialog));
                }
                record_changed = true;
            }
            ServerMessage::Move(mov) => {
                record.write().make_move(mov);
                record_changed = true;
            }
            ServerMessage::Retract => {
                record.write().undo_move();
                record_changed = true;
            }
            ServerMessage::Request(req, req_stone) => {
                requests.write()[req as usize] = Some(req_stone);
                if stone.get_untracked() == Some(req_stone.opposite()) {
                    confirm_request(req);
                }
            }
        }

        if record_changed {
            // Clear the requests if the record changed.
            requests.write().fill(None);

            // Also remove accept dialogs and callbacks.
            let mut dialogs = dialogs.write();
            let mut callbacks = confirm_callbacks.write_value();
            let mut removed = false;

            dialogs.retain(|entry| match entry.dialog {
                Dialog::Confirm(ConfirmDialog {
                    action: Action::Accept(_),
                }) => {
                    callbacks.remove(&entry.id);
                    removed = true;
                    false
                }
                _ => true,
            });

            if !removed {
                dialogs.untrack();
            }
        }

        first_msg_seen.set_value(true);
    };

    let connect = move |init_msg: ClientMessage| {
        let host = document().location().unwrap().host().unwrap();
        let ws = WebSocket::new(&format!("ws://{host}/ws")).unwrap();
        ws.set_binary_type(BinaryType::Arraybuffer);

        let onopen = Closure::once(move || send(init_msg));
        ws.set_onopen(Some(onopen.as_ref().unchecked_ref()));

        let onclose = Closure::<dyn Fn(CloseEvent)>::new(on_close);
        ws.set_onclose(Some(onclose.as_ref().unchecked_ref()));

        first_msg_seen.set_value(false);

        let onmessage = Closure::<dyn Fn(MessageEvent)>::new(on_message);
        ws.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));

        ws_state.set_value(Some(WebSocketState {
            ws,
            onopen,
            onclose,
            onmessage,
        }));
    };

    let set_game_id = move |id: &str| {
        if let Some(ws_state) = ws_state.write_value().take() {
            let ws = ws_state.ws;
            ws.set_onopen(None);
            ws.set_onclose(None);
            ws.set_onmessage(None);
            ws.close().unwrap();
        }

        dialogs.write().clear();

        if Some(id) != location_hash().as_deref() {
            history_push_state(&format!("#{id}"));
        }

        game_id.set(id.into());

        // This needs to be untracked for the cursor to show
        // when we click "Play Offline" at the main menu.
        *stone.write_untracked() = None;
        requests.write().fill(None);
        confirm_callbacks.write_value().clear();

        if id.is_empty() {
            record.write().clear();
            show_dialog(Dialog::from(MainMenuDialog));
            return;
        }

        if id == "local" {
            if let Some(decoded_record) = local_storage()
                .get_item(STORAGE_KEY_RECORD)
                .unwrap()
                .and_then(|buf| BASE64_STANDARD.decode(buf).ok())
                .and_then(|buf| Record::decode(&mut &buf[..], true))
            {
                record.set(decoded_record);
            } else {
                record.write().clear();
            }
            return;
        }

        if let Some(buf) = id.strip_prefix(ANALYZE_PREFIX) {
            if let Some(decoded_record) = BASE64_STANDARD
                .decode(buf)
                .ok()
                .and_then(|buf| Record::decode(&mut &buf[..], false))
            {
                record.set(decoded_record);
            } else {
                show_dialog(Dialog::from(ErrorDialog {
                    message: "Failed to decode record.".into(),
                }));
            }
            return;
        }

        #[cfg(feature = "online")]
        if let Ok(id) = c6ol_core::protocol::GameId::try_from(id.as_bytes()) {
            if id.iter().all(u8::is_ascii_alphanumeric) {
                connect(ClientMessage::Join(id));
                return;
            }
        }

        show_dialog(Dialog::from(ErrorDialog {
            message: "Invalid game ID.".into(),
        }));
    };

    let on_event = move |ev: Event| match ev {
        Event::Menu => show_game_menu_dialog(),
        Event::Submit(fst, snd) => {
            if online() {
                confirm(
                    Action::Submit,
                    Box::new(move || send(ClientMessage::Place(fst, snd))),
                );
            } else {
                record.write().make_move(Move::Stone(fst, snd));
            }
        }
        Event::Undo => {
            if !record.read_untracked().has_past() {
                return;
            }
            if online() {
                if request(Request::Retract) != stone.get_untracked() {
                    confirm_request(Request::Retract);
                }
            } else {
                record.write().undo_move();
            }
        }
        Event::Redo => {
            if !record.read_untracked().has_future() {
                return;
            }
            if !online() {
                record.write().redo_move();
            }
        }
        Event::Home => {
            if !record.read_untracked().has_past() {
                return;
            }
            if online() {
                if request(Request::Reset) != stone.get_untracked() {
                    confirm_request(Request::Reset);
                }
            } else {
                record.write().jump(0);
            }
        }
        Event::End => {
            if !record.read_untracked().has_future() {
                return;
            }
            if !online() {
                let mut record = record.write();
                let len = record.moves().len();
                record.jump(len);
            }
        }
    };

    let view_state = StoredValue::<game_view::State>::default();

    let on_game_menu_return = move |ret_val: GameMenuRetVal| match ret_val {
        GameMenuRetVal::Resume => {}
        GameMenuRetVal::MainMenu => {
            if online() {
                confirm(Action::MainMenu, Box::new(move || set_game_id("")));
            } else {
                set_game_id("");
            }
        }
        GameMenuRetVal::Join => {
            show_dialog(Dialog::from(JoinDialog));
        }
        GameMenuRetVal::Undo => on_event(Event::Undo),
        GameMenuRetVal::Redo => on_event(Event::Redo),
        GameMenuRetVal::Home => on_event(Event::Home),
        GameMenuRetVal::End => on_event(Event::End),
        GameMenuRetVal::ClaimWin => {
            // TODO.
        }
        GameMenuRetVal::Resign => {
            if online() {
                confirm(
                    Action::Resign,
                    Box::new(move || send(ClientMessage::Resign)),
                );
            } else {
                let mut record = record.write();
                let stone = record.turn().unwrap();
                record.make_move(Move::Resign(stone));
            }
        }
        GameMenuRetVal::Pass => {
            let tentative = view_state.read_value().tentative();
            if online() {
                let (action, msg) = if let Some(pos) = tentative {
                    (Action::PlaceSingleStone, ClientMessage::Place(pos, None))
                } else {
                    (Action::Pass, ClientMessage::Pass)
                };
                confirm(action, Box::new(move || send(msg)));
            } else {
                let mov = if let Some(pos) = tentative {
                    Move::Stone(pos, None)
                } else {
                    Move::Pass
                };
                record.write().make_move(mov);
            }
        }
        GameMenuRetVal::Draw => {
            if online() {
                confirm_request(Request::Draw);
            } else {
                record.write().make_move(Move::Draw);
            }
        }
    };

    let on_dialog_return = move |id: u32, ret_val: RetVal| {
        // We can't simply `pop`, because if we right click as soon as we close
        // the game menu, `show_dialog` can get called before `on_dialog_return`.
        // This is similar to how a `pointerover` event gets fired before a
        // `close` event when a dialog is closed with a pointer (see comments at
        // `on_hover` in `game_menu.rs`).
        dialogs.write().retain(|entry| entry.id != id);

        match ret_val {
            RetVal::MainMenu(ret_val) => match ret_val {
                MainMenuRetVal::Offline => set_game_id("local"),
                MainMenuRetVal::Online => {
                    show_dialog(Dialog::from(OnlineMenuDialog));
                }
            },
            RetVal::OnlineMenu(ret_val) => match ret_val {
                OnlineMenuRetVal::Cancel => {
                    show_dialog(Dialog::from(MainMenuDialog));
                }
                OnlineMenuRetVal::Start(passcode) => {
                    connect(ClientMessage::Start(passcode.into_bytes().into()));
                }
                OnlineMenuRetVal::Join(game_id) => set_game_id(&game_id),
            },
            RetVal::Join(ret_val) => match ret_val {
                JoinRetVal::ViewOnly => {}
                JoinRetVal::Join(passcode) => {
                    send(ClientMessage::Start(passcode.into_bytes().into()));
                }
            },
            RetVal::ConnClosed(ret_val) => match ret_val {
                ConnClosedRetVal::Menu => set_game_id(""),
                ConnClosedRetVal::Retry => set_game_id(&game_id.get()),
            },
            RetVal::GameMenu(ret_val) => on_game_menu_return(ret_val),
            RetVal::Confirm(ret_val) => {
                let callback = confirm_callbacks.write_value().remove(&id);
                if let ConfirmRetVal::Confirm = ret_val {
                    if let Some(callback) = callback {
                        callback();
                    }
                }
            }
            RetVal::Error(ret_val) => match ret_val {
                ErrorRetVal::MainMenu => set_game_id(""),
            },
        }
    };

    let on_hash_change = move || {
        set_game_id(location_hash().as_deref().unwrap_or_default());
    };
    on_hash_change();

    let handle_hashchange = window_event_listener(ev::hashchange, move |_| on_hash_change());

    let handle_storage = window_event_listener(ev::storage, move |ev| {
        if *game_id.read_untracked() == "local" && ev.key().as_deref() == Some(STORAGE_KEY_RECORD) {
            if let Some(buf) = ev
                .new_value()
                .and_then(|buf| BASE64_STANDARD.decode(buf).ok())
                .and_then(|buf| Record::decode(&mut &buf[..], true))
            {
                record.set(buf);
            }
        }
    });

    on_cleanup(move || {
        handle_hashchange.remove();
        handle_storage.remove();
    });

    view! {
        <game_view::GameView
            record=record.read_only()
            stone=stone.read_only()
            state=view_state
            disabled=move || !dialogs.read().is_empty()
            on_event=on_event
        />
        <For each=move || dialogs.get() key=|entry| entry.id let(DialogEntry { id, dialog })>
            {dialog.show(id, on_dialog_return)}
        </For>
    }
}
