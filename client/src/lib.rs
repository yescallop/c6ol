//! The client library for [Connect6 Online](https://github.com/yescallop/c6ol).

mod argon2id;
mod dialog;
mod game_view;

use base64::engine::{DecodePaddingMode, Engine, GeneralPurpose, GeneralPurposeConfig};
use c6ol_core::{
    game::{Direction, Move, Point, Record, RecordEncodingScheme, Stone},
    protocol::{
        ClientMessage, GameId, GameOptions, Message, Player, PlayerSlots, Request, ServerMessage,
    },
};
use dialog::*;
use leptos::{ev, prelude::*};
use std::{
    sync::atomic::{AtomicU32, Ordering},
    time::Duration,
};
use tinyvec::ArrayVec;
use web_sys::{
    BinaryType, CloseEvent, MessageEvent, Storage, WebSocket,
    js_sys::{ArrayBuffer, Uint8Array},
    wasm_bindgen::prelude::*,
};

const BASE64_STD: GeneralPurpose = GeneralPurpose::new(
    &base64::alphabet::STANDARD,
    GeneralPurposeConfig::new()
        .with_encode_padding(false)
        .with_decode_padding_mode(DecodePaddingMode::Indifferent),
);

const BASE64_URL: GeneralPurpose = GeneralPurpose::new(
    &base64::alphabet::URL_SAFE,
    GeneralPurposeConfig::new()
        .with_encode_padding(false)
        .with_decode_padding_mode(DecodePaddingMode::RequireNone),
);

#[allow(unused)]
macro_rules! console_log {
    ($($t:tt)*) => {
        web_sys::console::log_1(
            &web_sys::wasm_bindgen::JsValue::from_str(&format_args!($($t)*).to_string())
        )
    };
}
#[allow(unused)]
pub(crate) use console_log;

#[derive(Clone)]
enum Confirm {
    MainMenu,
    Submit(Point, Option<Point>),
    Pass(Option<Point>),
    BeginClaim,
    Claim(ArrayVec<[Point; 2]>, Point, Direction),
    RequestDraw,
    RequestRetract,
    Requested(Player, Request),
    RequestAccepted,
    RequestDeclined,
    Resign { online: bool },
    ConnClosed(String),
    Error(String),
}

enum Event {
    Menu,
    Submit,
    Undo,
    Redo,
    Home,
    End,
    Resign,
    Draw,
}

#[derive(Clone, Copy)]
enum WinClaim {
    PendingPoint,
    PendingDirection(Point),
    Ready(Point, Direction),
}

const STORAGE_KEY_RECORD: &str = "record";
const RECORD_PREFIX_LEGACY: &str = "analyze,";
const RECORD_PREFIX: &str = "rec,";

#[derive(Clone)]
struct DialogEntry {
    id: u32,
    dialog: Dialog,
}

struct WebSocketState {
    ws: WebSocket,
    init_msg: ClientMessage,
    #[expect(dead_code)]
    onopen: Closure<dyn FnMut()>,
    #[expect(dead_code)]
    onclose: Closure<dyn Fn(CloseEvent)>,
    #[expect(dead_code)]
    onmessage: Closure<dyn Fn(MessageEvent)>,
    was_active: bool,
    reconnect_handle: Option<TimeoutHandle>,
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

const RECONNECT_TIMEOUT: Duration = Duration::from_millis(500);

#[derive(Clone, Copy, PartialEq, Eq)]
enum GameKind {
    Pending,
    Local,
    Record,
    Online(GameId),
}

/// Entry-point for the app.
#[component]
pub fn App() -> impl IntoView {
    let record = RwSignal::new(Record::new());

    let tentatives_pos = RwSignal::new(ArrayVec::new());
    let win_claim = RwSignal::new(None);

    let game_kind = RwSignal::new(GameKind::Pending);

    let player = RwSignal::new(None::<Player>);
    let requests = RwSignal::new(PlayerSlots::<Option<Request>>::default());
    let options = RwSignal::new(None::<GameOptions>);

    let stone = Memo::new(move |_| {
        if let Some(player) = player.get()
            && let Some(options) = options.get()
        {
            Some(options.stone_of(player))
        } else {
            record.read().turn()
        }
    });

    let dialog_entries = RwSignal::new(Vec::<DialogEntry>::new());

    let show_dialog = move |dialog: Dialog| {
        static NEXT_ID: AtomicU32 = AtomicU32::new(0);

        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        dialog_entries.write().push(DialogEntry { id, dialog });
    };

    let confirm = move |confirm: Confirm| show_dialog(Dialog::from(ConfirmDialog(confirm)));

    let ws_state = RwSignal::new_local(None::<WebSocketState>);

    let online = move || ws_state.read().is_some();

    Effect::new(move || {
        if game_kind.get() == GameKind::Local {
            // Save the record to local storage.
            let mut buf = vec![];
            record.read().encode(&mut buf, RecordEncodingScheme::all());
            let buf = BASE64_STD.encode(buf);
            local_storage().set_item(STORAGE_KEY_RECORD, &buf).unwrap();
        }
    });

    // Sends the message on the WebSocket connection.
    let send = move |msg: ClientMessage| {
        if let Some(ws_state) = &*ws_state.read_untracked()
            && ws_state.ws.ready_state() == WebSocket::OPEN
        {
            ws_state
                .ws
                .send_with_u8_array(&msg.encode_to_vec())
                .unwrap();
            return;
        }
        confirm(Confirm::Error("Connection is not open.".into()));
    };

    let show_game_menu_dialog = move || {
        if dialog_entries
            .read()
            .iter()
            .any(|entry| matches!(entry.dialog, Dialog::GameMenu(_)))
        {
            // Skip if game menu is already open.
            return;
        }

        show_dialog(Dialog::from(GameMenuDialog {
            game_kind: game_kind.read_only(),
            stone,
            online: online(),
            player: player.read_only(),
            record: record.read_only(),
            win_claim: win_claim.read_only(),
            requests: requests.read_only(),
        }));
    };

    let on_message = move |ev: MessageEvent| {
        let Some(msg) = ev
            .data()
            .dyn_ref::<ArrayBuffer>()
            .map(|buf| Uint8Array::new(buf).to_vec())
            .and_then(|buf| ServerMessage::decode(&mut &buf[..]))
        else {
            let ws_state = ws_state.read();
            let ws = &ws_state.as_ref().unwrap().ws;
            ws.close_with_code_and_reason(CLOSE_CODE_POLICY, "Malformed server message.")
                .unwrap();
            return;
        };

        ws_state.write_untracked().as_mut().unwrap().was_active = true;

        let mut record_changed = false;
        match msg {
            ServerMessage::Started(id) => {
                history_push_state(&format!("#{id}"));
                game_kind.set(GameKind::Online(id));
            }
            ServerMessage::Authenticated(assigned_player) => {
                player.set(Some(assigned_player));
                if options.get().is_some() {
                    show_game_menu_dialog();
                }
                if let Some(req) = requests.read()[assigned_player.opposite()] {
                    confirm(Confirm::Requested(assigned_player, req));
                }
            }
            ServerMessage::Options(new_options) => {
                if player.get().is_some() {
                    if options.get().is_none() {
                        show_game_menu_dialog();
                    }
                } else {
                    show_dialog(Dialog::from(AuthDialog));
                }
                options.set(Some(new_options));
            }
            ServerMessage::Record(new_record) => {
                record.set(*new_record);
                record_changed = true;
            }
            ServerMessage::Move(mov) => {
                record.write().make_move(mov);
                record_changed = true;
            }
            ServerMessage::Retract => {
                let mut record = record.write();
                record.undo_move();
                record.clear_future();
                record_changed = true;
            }
            ServerMessage::Request(initiator, req) => {
                requests.write()[initiator] = Some(req);

                if let Some(player) = player.get()
                    && player != initiator
                {
                    confirm(Confirm::Requested(player, req));
                }
            }
            ServerMessage::AcceptRequest(initiator) => {
                // This is no-op for now, but might be useful later.
                requests.write()[initiator.opposite()] = None;

                if player.get() == Some(initiator.opposite()) {
                    confirm(Confirm::RequestAccepted);
                }
            }
            ServerMessage::DeclineRequest(initiator) => {
                requests.write()[initiator.opposite()] = None;

                if player.get() == Some(initiator.opposite()) {
                    confirm(Confirm::RequestDeclined);
                }
            }
        }

        if record_changed {
            // Clear the requests if the record changed.
            requests.write().fill(None);

            // Also close all confirm dialogs.
            let mut entries = dialog_entries.write();
            let mut removed = false;

            entries.retain(|entry| match &entry.dialog {
                Dialog::Confirm(ConfirmDialog(_)) => {
                    removed = true;
                    false
                }
                _ => true,
            });

            if !removed {
                entries.untrack();
            }
        }
    };

    let clear_all = move || {
        record.write().clear();

        player.set(None);
        requests.write().fill(None);
        options.set(None);

        dialog_entries.write().clear();
    };

    fn connect(
        init_msg: ClientMessage,
        ws_state: RwSignal<Option<WebSocketState>, LocalStorage>,
        game_kind: RwSignal<GameKind>,
        send: impl Fn(ClientMessage) + Copy + 'static,
        clear_all: impl Fn() + Copy + 'static,
        on_message: impl Fn(MessageEvent) + Copy + 'static,
        confirm: impl Fn(Confirm) + Copy + 'static,
    ) {
        let proto = if location().protocol().unwrap() == "https:" {
            "wss:"
        } else {
            "ws:"
        };
        let host = location().host().unwrap();

        let ws = WebSocket::new(&format!("{proto}//{host}/ws")).unwrap();
        ws.set_binary_type(BinaryType::Arraybuffer);

        let onopen = Closure::once(move || {
            send(init_msg);
        });
        ws.set_onopen(Some(onopen.as_ref().unchecked_ref()));

        let onclose = Closure::new(move |ev: CloseEvent| {
            clear_all();

            let code = ev.code();
            let mut reason = ev.reason();

            if reason.is_empty() {
                // The reason being empty means it's not the server closing the connection.
                // If the socket was open and the game has started, attempt to reconnect.
                let mut state = ws_state.write_untracked();
                let state = state.as_mut().unwrap();

                if state.was_active
                    && let GameKind::Online(id) = game_kind.get()
                {
                    state.reconnect_handle = set_timeout_with_handle(
                        move || {
                            connect(
                                ClientMessage::Join(id),
                                ws_state,
                                game_kind,
                                send,
                                clear_all,
                                on_message,
                                confirm,
                            );
                        },
                        RECONNECT_TIMEOUT,
                    )
                    .ok();
                    return;
                }

                if code == CLOSE_CODE_ABNORMAL {
                    reason = "Connection failed.".into();
                } else {
                    reason = format!("Connection closed with code {code}.");
                }
            }
            confirm(Confirm::ConnClosed(reason));
        });
        ws.set_onclose(Some(onclose.as_ref().unchecked_ref()));

        let onmessage = Closure::<dyn Fn(MessageEvent)>::new(move |ev| untrack(|| on_message(ev)));
        ws.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));

        ws_state.set(Some(WebSocketState {
            ws,
            init_msg,
            onopen,
            onclose,
            onmessage,
            was_active: false,
            reconnect_handle: None,
        }));
    }

    let connect = move |init_msg| {
        connect(
            init_msg, ws_state, game_kind, send, clear_all, on_message, confirm,
        );
    };

    let set_game_id = move |id: &str| {
        if ws_state.read_untracked().is_some() {
            let WebSocketState {
                ws,
                reconnect_handle,
                ..
            } = ws_state.write().take().unwrap();

            ws.set_onopen(None);
            ws.set_onclose(None);
            ws.set_onmessage(None);
            ws.close().unwrap();

            if let Some(handle) = reconnect_handle {
                handle.clear();
            }
        }

        clear_all();

        if location_hash().as_deref() != Some(id) {
            history_push_state(&format!("#{id}"));
        }

        if id.is_empty() {
            game_kind.set(GameKind::Pending);

            show_dialog(Dialog::from(MainMenuDialog));
            return;
        }

        if id == "local" {
            game_kind.set(GameKind::Local);

            if let Some(rec) = local_storage().get_item(STORAGE_KEY_RECORD).unwrap()
                && let Ok(rec) = BASE64_STD.decode(rec)
                && let Some(rec) = Record::decode(&mut &rec[..])
            {
                record.set(rec);
            } else {
                record.write().clear();
            }
            return;
        }

        for (prefix, base64_engine) in [
            (RECORD_PREFIX_LEGACY, BASE64_STD),
            (RECORD_PREFIX, BASE64_URL),
        ] {
            if let Some(rec) = id.strip_prefix(prefix) {
                if let Ok(rec) = base64_engine.decode(rec)
                    && let Some(rec) = Record::decode(&mut &rec[..])
                {
                    game_kind.set(GameKind::Record);

                    record.set(rec);
                } else {
                    confirm(Confirm::Error("Failed to decode record.".into()));
                }
                return;
            }
        }

        if let Some(id) = GameId::from_base62(id.as_bytes()) {
            game_kind.set(GameKind::Online(id));

            connect(ClientMessage::Join(id));
            return;
        }

        confirm(Confirm::Error("Invalid game ID.".into()));
    };

    let on_event = move |ev: Event| match ev {
        Event::Menu => show_game_menu_dialog(),
        Event::Submit => {
            let tentatives = tentatives_pos.get();
            let claim = win_claim.get();
            if online() {
                confirm(match claim {
                    Some(WinClaim::Ready(p, dir)) => Confirm::Claim(tentatives, p, dir),
                    _ => match tentatives[..] {
                        [] => Confirm::Pass(None),
                        [p] if record.read().has_past() => Confirm::Pass(Some(p)),
                        [p] => Confirm::Submit(p, None),
                        [p1, p2] => Confirm::Submit(p1, Some(p2)),
                        _ => unreachable!(),
                    },
                });
            } else {
                let mut record = record.write();

                if let Some(WinClaim::Ready(p, dir)) = claim {
                    if !tentatives.is_empty() {
                        record.make_move(Move::Place(tentatives[0], tentatives.get(1).copied()));
                    }
                    record.make_move(Move::Win(p, dir));
                } else {
                    record.make_move(match tentatives[..] {
                        [] => Move::Pass,
                        [p] => Move::Place(p, None),
                        [p1, p2] => Move::Place(p1, Some(p2)),
                        _ => unreachable!(),
                    });
                }
            }
        }
        Event::Undo => {
            if !record.read().has_past() {
                return;
            }
            if !online() {
                record.write().undo_move();
            } else if let Some(player) = player.get() {
                let requests = requests.read();
                if let Some(req @ Request::Retract) = requests[player.opposite()] {
                    confirm(Confirm::Requested(player, req));
                } else if requests[player].is_none() {
                    confirm(Confirm::RequestRetract);
                }
            }
        }
        Event::Redo => {
            if !record.read().has_future() {
                return;
            }
            if !online() {
                record.write().redo_move();
            }
        }
        Event::Home => {
            if let Some(player) = player.get() {
                let requests = requests.read();
                if let Some(req @ Request::Reset { .. }) = requests[player.opposite()] {
                    confirm(Confirm::Requested(player, req));
                } else if requests[player].is_none()
                    && let Some(options) = options.get()
                {
                    show_dialog(Dialog::from(ResetDialog {
                        player,
                        old_options: options,
                    }));
                }
            } else {
                if !record.read().has_past() {
                    return;
                }
                record.write().jump(0);
            }
        }
        Event::End => {
            if !record.read().has_future() {
                return;
            }
            if !online() {
                let mut record = record.write();
                let len = record.moves().len();
                record.jump(len);
            }
        }
        Event::Resign => {
            confirm(Confirm::Resign { online: online() });
        }
        Event::Draw => {
            if let Some(player) = player.get() {
                let requests = requests.read();
                if let Some(req @ Request::Draw) = requests[player.opposite()] {
                    confirm(Confirm::Requested(player, req));
                } else if requests[player].is_none() {
                    confirm(Confirm::RequestDraw);
                }
            } else {
                record.write().make_move(Move::Draw);
            }
        }
    };

    let on_game_menu_return = move |ret_val: GameMenuRetVal| match ret_val {
        GameMenuRetVal::Resume => {}
        GameMenuRetVal::MainMenu => {
            if online() {
                confirm(Confirm::MainMenu);
            } else {
                set_game_id("");
            }
        }
        GameMenuRetVal::Auth => {
            show_dialog(Dialog::from(AuthDialog));
        }
        GameMenuRetVal::Undo => on_event(Event::Undo),
        GameMenuRetVal::Redo => on_event(Event::Redo),
        GameMenuRetVal::Home => on_event(Event::Home),
        GameMenuRetVal::End => on_event(Event::End),
        GameMenuRetVal::ClaimWin => {
            if win_claim.get().is_none() {
                win_claim.set(Some(WinClaim::PendingPoint));
                if online() {
                    confirm(Confirm::BeginClaim);
                }
            } else {
                win_claim.set(None);
            }
        }
        GameMenuRetVal::Resign => on_event(Event::Resign),
        GameMenuRetVal::Submit => on_event(Event::Submit),
        GameMenuRetVal::Draw => on_event(Event::Draw),
    };

    let on_dialog_return = move |id: u32, ret_val: RetVal| {
        // We can't simply `pop`, because if we right click as soon as we close
        // the game menu, `show_dialog` can get called before `on_dialog_return`.
        // This is similar to how a `pointerover` event gets fired before a
        // `close` event when a dialog is closed with a pointer (see comments at
        // `on_hover` in `game_menu.rs`).
        let mut entries = dialog_entries.write();
        let i = (0..entries.len()).rfind(|&i| entries[i].id == id).unwrap();
        let dialog = entries.remove(i).dialog;
        drop(entries);

        match ret_val {
            RetVal::MainMenu(ret_val) => match ret_val {
                MainMenuRetVal::Local => set_game_id("local"),
                MainMenuRetVal::Online => {
                    show_dialog(Dialog::from(OnlineMenuDialog));
                }
            },
            RetVal::OnlineMenu(ret_val) => match ret_val {
                OnlineMenuRetVal::Cancel => {
                    show_dialog(Dialog::from(MainMenuDialog));
                }
                OnlineMenuRetVal::Start(options) => connect(ClientMessage::Start(options)),
                OnlineMenuRetVal::Join(game_id) => set_game_id(&game_id),
            },
            RetVal::Auth(ret_val) => match ret_val {
                AuthRetVal::ViewOnly => {}
                AuthRetVal::Submit(passcode) => {
                    let GameKind::Online(id) = game_kind.get() else {
                        return;
                    };

                    match argon2id::hash(passcode.as_bytes(), id.0) {
                        Ok(hash) => send(ClientMessage::Authenticate(hash)),
                        Err(err) => {
                            confirm(Confirm::Error(format!("Failed to hash passcode: {err}")));
                        }
                    }
                }
            },
            RetVal::GameMenu(ret_val) => on_game_menu_return(ret_val),
            RetVal::Confirm(ret_val) => {
                let Dialog::Confirm(ConfirmDialog(confirm)) = dialog else {
                    unreachable!();
                };

                if ret_val == ConfirmRetVal::Cancel {
                    if let Confirm::ConnClosed(_) | Confirm::Error(_) = confirm {
                        set_game_id("");
                    }
                    return;
                }

                match confirm {
                    Confirm::MainMenu => set_game_id(""),
                    Confirm::Submit(p1, p2) => send(ClientMessage::Place(p1, p2)),
                    Confirm::Pass(None) => send(ClientMessage::Pass),
                    Confirm::Pass(Some(p)) => send(ClientMessage::Place(p, None)),
                    Confirm::BeginClaim => {}
                    Confirm::Claim(tentatives, p, dir) => {
                        if !tentatives.is_empty() {
                            send(ClientMessage::Place(
                                tentatives[0],
                                tentatives.get(1).copied(),
                            ));
                        }
                        send(ClientMessage::ClaimWin(p, dir));
                    }
                    Confirm::RequestDraw => send(ClientMessage::Request(Request::Draw)),
                    Confirm::RequestRetract => send(ClientMessage::Request(Request::Retract)),
                    Confirm::Requested(..) => {
                        send(match ret_val {
                            ConfirmRetVal::Confirm => ClientMessage::AcceptRequest,
                            ConfirmRetVal::AltConfirm => ClientMessage::DeclineRequest,
                            ConfirmRetVal::Cancel => unreachable!(),
                        });
                    }
                    Confirm::RequestAccepted | Confirm::RequestDeclined => {}
                    Confirm::Resign { online } => {
                        if online {
                            send(ClientMessage::Resign);
                        } else {
                            let resigned_stone = match ret_val {
                                ConfirmRetVal::Confirm => Stone::White,
                                ConfirmRetVal::AltConfirm => Stone::Black,
                                ConfirmRetVal::Cancel => unreachable!(),
                            };
                            record.write().make_move(Move::Resign(resigned_stone));
                        }
                    }
                    Confirm::ConnClosed(_) => {
                        let init_msg = ws_state.read().as_ref().map(|s| s.init_msg);
                        if let Some(init_msg) = init_msg {
                            connect(init_msg);
                        }
                    }
                    Confirm::Error(_) => set_game_id(""),
                }
            }
            RetVal::Reset(ret_val) => match ret_val {
                ResetRetVal::Cancel => {}
                ResetRetVal::Confirm(options) => {
                    send(ClientMessage::Request(Request::Reset(options)));
                }
            },
        }
    };

    let on_hash_change = move || {
        set_game_id(location_hash().as_deref().unwrap_or_default());
    };
    on_hash_change();

    let handle_hashchange = window_event_listener(ev::hashchange, move |_| on_hash_change());

    let handle_storage = window_event_listener(ev::storage, move |ev| {
        if game_kind.get() == GameKind::Local
            && ev.key().as_deref() == Some(STORAGE_KEY_RECORD)
            && let Some(rec) = ev.new_value()
            && let Ok(rec) = BASE64_STD.decode(rec)
            && let Some(rec) = Record::decode(&mut &rec[..])
        {
            record.set(rec);
        }
    });

    on_cleanup(move || {
        handle_hashchange.remove();
        handle_storage.remove();
    });

    view! {
        <game_view::GameView
            record=record
            stone=stone
            disabled=move || !dialog_entries.read().is_empty()
            pending=move || online() && options.get().is_none()
            replaying=move || game_kind.get() == GameKind::Record
            on_event=on_event
            tentatives_pos=tentatives_pos
            win_claim=win_claim
        />
        <For each=move || dialog_entries.get() key=|entry| entry.id let(DialogEntry { id, dialog })>
            {dialog.show(id, on_dialog_return)}
        </For>
    }
}
