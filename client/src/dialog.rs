use crate::{AppState, BASE64_URL, Confirm, GameKind, RECORD_PREFIX, Submit, WinClaim};
use base64::Engine;
use c6ol_core::{
    game::{Move, RecordEncodingScheme, Stone},
    protocol::{GameOptions, Player, Request},
};
use leptos::{
    either::{Either, EitherOf3, EitherOf6},
    html,
    prelude::*,
};
use std::sync::Arc;

trait DialogView {
    type RetVal;

    fn class(&self) -> Option<&'static str> {
        None
    }

    fn contents(self) -> impl IntoView;
}

macro_rules! ret {
    ($($val:tt)+) => {
        use_context::<StoredValue<RetVal>>()
            .unwrap()
            .set_value(RetVal::from(Self::RetVal::$($val)+))
    };
}

macro_rules! dialogs {
    ($either_type:ty {
        $($either_variant:ident => $name:ident,)+
    }) => {
        paste::paste! {
            #[derive(Clone)]
            pub enum Dialog {
                $(
                    $name([<$name Dialog>]),
                )+
            }

            $(
                impl From<[<$name Dialog>]> for Dialog {
                    fn from(dialog: [<$name Dialog>]) -> Self {
                        Self::$name(dialog)
                    }
                }
            )+

            #[derive(Debug)]
            pub enum RetVal {
                $(
                    $name([<$name RetVal>]),
                )+
            }

            $(
                impl From<[<$name RetVal>]> for RetVal {
                    fn from(ret_val: [<$name RetVal>]) -> Self {
                        Self::$name(ret_val)
                    }
                }
            )+

            impl Dialog {
                pub fn show(self, id: u32, on_return: impl Fn(u32, RetVal) + 'static) -> impl IntoView {
                    let dialog_ref = NodeRef::<html::Dialog>::new();

                    Effect::new(move || {
                        dialog_ref.get().unwrap().show_modal().unwrap();
                    });

                    let default_ret_val = match self {
                        $(
                            Dialog::$name(_) => RetVal::$name(Default::default()),
                        )+
                    };

                    let ret_val = StoredValue::new(default_ret_val);
                    provide_context(ret_val);

                    let (class, contents) = match self {
                        $(
                            Dialog::$name(dialog) => (
                                dialog.class(),
                                $either_type::$either_variant(dialog.contents()),
                            ),
                        )+
                    };

                    let on_close = move |_| {
                        on_return(id, ret_val.into_inner().unwrap());
                    };

                    view! {
                        <dialog node_ref=dialog_ref class=class on:close=on_close>
                            <form method="dialog">{contents}</form>
                        </dialog>
                    }
                }
            }
        }
    };
}

dialogs!(EitherOf6 {
    A => MainMenu,
    B => OnlineMenu,
    C => Auth,
    D => GameMenu,
    E => Confirm,
    F => Reset,
});

#[derive(Clone)]
pub struct MainMenuDialog;

#[derive(Debug, Default)]
pub enum MainMenuRetVal {
    #[default]
    Local,
    Online,
}

impl DialogView for MainMenuDialog {
    type RetVal = MainMenuRetVal;

    fn contents(self) -> impl IntoView {
        view! {
            <p class="title">"Main Menu"</p>
            <div class="menu-btn-group">
                <button>"Local Play"</button>
                <button on:click=move |_| ret!(Online)>"Online Play"</button>
                <a target="_blank" href="https://github.com/yescallop/c6ol">
                    <button type="button">"Source Code"</button>
                </a>
            </div>
        }
    }
}

#[derive(Clone)]
pub struct OnlineMenuDialog;

#[derive(Debug, Default)]
pub enum OnlineMenuRetVal {
    #[default]
    Cancel,
    Start(GameOptions),
    Join(String),
}

impl DialogView for OnlineMenuDialog {
    type RetVal = OnlineMenuRetVal;

    fn contents(self) -> impl IntoView {
        let start_checked = RwSignal::new(true);

        let start_or_join = move || {
            if start_checked.get() {
                let stone = RwSignal::new(Stone::Black);

                let view = view! {
                    <table>
                        <tr>
                            <td style="text-align: right;">"To Play: "</td>
                            <td style="text-align: center;">
                                <input
                                    type="radio"
                                    id="black"
                                    name="stone"
                                    checked
                                    on:input=move |_| stone.set(Stone::Black)
                                />
                                <label for="black">"Black"</label>
                                <input
                                    type="radio"
                                    id="white"
                                    name="stone"
                                    on:input=move |_| stone.set(Stone::White)
                                />
                                <label for="white">"White"</label>
                            </td>
                        </tr>
                    // TODO: More options.
                    </table>
                    <div class="btn-group reversed">
                        <button on:click=move |_| {
                            let options = GameOptions {
                                swapped: stone.get() == Stone::White,
                            };
                            ret!(Start(options));
                        }>"Start"</button>
                        <button>"Cancel"</button>
                    </div>
                };
                Either::Left(view)
            } else {
                let game_id = RwSignal::new(String::new());

                let view = view! {
                    <label for="game-id">"Game ID: "</label>
                    <input
                        type="text"
                        id="game-id"
                        required
                        pattern="[0-9A-Za-z]{11}"
                        placeholder="11 alphanumerics"
                        bind:value=game_id
                    />
                    <div class="btn-group reversed">
                        <button on:click=move |_| {
                            let id = game_id.get();
                            if id.len() == 11 && id.bytes().all(|b| b.is_ascii_alphanumeric()) {
                                ret!(Join(id));
                            }
                        }>"Join"</button>
                        <button formnovalidate>"Cancel"</button>
                    </div>
                };
                Either::Right(view)
            }
        };

        view! {
            <p class="title">"Online Play"</p>
            <div class="radio-group">
                <input
                    type="radio"
                    id="start"
                    name="action"
                    checked
                    on:input=move |_| start_checked.set(true)
                />
                <label for="start">"Start"</label>
                <input
                    type="radio"
                    id="join"
                    name="action"
                    on:input=move |_| start_checked.set(false)
                />
                <label for="join">"Join"</label>
            </div>
            {start_or_join}
        }
    }
}

#[derive(Clone)]
pub struct AuthDialog;

#[derive(Debug, Default)]
pub enum AuthRetVal {
    #[default]
    ViewOnly,
    Submit(String),
}

impl DialogView for AuthDialog {
    type RetVal = AuthRetVal;

    fn contents(self) -> impl IntoView {
        let passcode = RwSignal::new(String::new());

        view! {
            <p class="title">"Authenticate"</p>
            <label for="passcode">"Passcode: "</label>
            <input
                type="password"
                id="passcode"
                required
                placeholder="Yours, not shared"
                bind:value=passcode
            />
            <div class="btn-group reversed">
                <button on:click=move |_| {
                    let code = passcode.get();
                    if !code.is_empty() {
                        ret!(Submit(code));
                    }
                }>"Submit"</button>
                <button formnovalidate>"View Only"</button>
            </div>
        }
    }
}

#[derive(Clone)]
pub struct GameMenuDialog;

#[derive(Debug, Default)]
pub enum GameMenuRetVal {
    #[default]
    Resume,
    MainMenu,
    Auth,
    Undo,
    Redo,
    Home,
    End,
    ClaimWin,
    Resign,
    Submit,
    Draw,
}

impl DialogView for GameMenuDialog {
    type RetVal = GameMenuRetVal;

    fn class(&self) -> Option<&'static str> {
        Some("game-menu")
    }

    fn contents(self) -> impl IntoView {
        let AppState {
            game_kind,
            stone,
            player,
            record,
            win_claim,
            requests,
            ..
        } = *use_context::<Arc<AppState>>().unwrap();

        let online = game_kind.get().is_online();

        let alt_pushed = RwSignal::new(false);

        let info_view = view! {
            {move || {
                match game_kind.get() {
                    GameKind::Pending => Either::Left("Pending"),
                    GameKind::Local => Either::Left("Local"),
                    GameKind::Record => Either::Left("Record"),
                    GameKind::Online(id) => {
                        let href = format!("#{id}");
                        Either::Right(
                            view! {
                                <a href=href>{id.to_string()}</a>
                                <br />
                                {move || match stone.get() {
                                    Some(stone) => format!("Playing {stone}"),
                                    None => "View Only".into(),
                                }}
                            },
                        )
                    }
                }
            }}
            <br />
            {move || {
                let record = record.read();
                if let Some(stone) = record.turn() {
                    return format!("{stone} to Play");
                }
                match record.prev_move().unwrap() {
                    Move::Draw => "Game Drawn".into(),
                    Move::Resign(stone) => format!("{stone} Resigned"),
                    Move::Win(p, _) => {
                        let stone = record.stone_at(p).unwrap();
                        format!("{stone} Won")
                    }
                    _ => unreachable!(),
                }
            }}
            <br />
            {move || {
                let record = record.read();
                if record.has_future() {
                    format!("Move {} of {}", record.move_index(), record.moves().len())
                } else {
                    format!("Move {}", record.move_index())
                }
            }}
            <br />
            <a
                target="_blank"
                href=move || {
                    let mut buf = vec![];
                    record.read().encode(&mut buf, RecordEncodingScheme::past());
                    format!("#{RECORD_PREFIX}{}", BASE64_URL.encode(buf))
                }
            >
                "Export"
            </a>
        };

        let ctrl_view = move || {
            let alt_btn = move |pushed: bool| {
                view! {
                    <button
                        on:click=move |ev| {
                            ev.prevent_default();
                            alt_pushed.set(!pushed);
                        }
                        class:pushed=pushed
                    >
                        "Alt"
                    </button>
                }
            };

            let no_past = move || !record.read().has_past();
            let no_future = move || !record.read().has_future();
            let ended = move || record.read().is_ended();

            #[derive(Eq, Ord, PartialEq, PartialOrd)]
            enum RequestState {
                /// The user is offline or unauthenticated.
                Irrelevant,
                /// The opponent has made this request.
                CanAccept,
                /// The user can make this request.
                CanMake,
                /// The user has made this request.
                Made,
                /// The user has made another request.
                MadeAnother,
            }

            use Request::*;
            use RequestState::*;

            macro_rules! req_state {
                ($pat:pat) => {
                    if let Some(player) = player.get() {
                        let requests = requests.read();
                        if matches!(requests[player.opposite()], Some($pat)) {
                            CanAccept
                        } else if requests[player].is_none() {
                            CanMake
                        } else if matches!(requests[player], Some($pat)) {
                            Made
                        } else {
                            MadeAnother
                        }
                    } else {
                        Irrelevant
                    }
                };
            }

            let main_ctrl_view = move || {
                view! {
                    <div class="btn-group">
                        {alt_btn(false)}
                        <button
                            on:click=move |_| ret!(Undo)
                            disabled=move || { no_past() || req_state!(Retract) >= Made }
                            class:prominent=move || req_state!(Retract) == CanAccept
                            class:pushed=move || req_state!(Retract) == Made
                        >
                            {if online { "Retract" } else { "Undo" }}
                        </button>
                        {(!online)
                            .then(|| {
                                view! {
                                    <button on:click=move |_| ret!(Redo) disabled=no_future>
                                        "Redo"
                                    </button>
                                }
                            })}
                    </div>
                    <div class="btn-group">
                        <button
                            class:pushed=move || win_claim.read().is_some()
                            on:click=move |_| ret!(ClaimWin)
                            disabled=ended
                        >
                            "Claim Win"
                        </button>
                        <button
                            on:click=move |_| ret!(Submit)
                            disabled=move || {
                                ended()
                                    || (record.read().turn() != stone.get()
                                        && !matches!(win_claim.get(), Some(WinClaim::Ready(..))))
                            }
                        >
                            "Submit"
                        </button>
                    </div>
                }
            };

            let alt_ctrl_view = move || {
                view! {
                    <div class="btn-group">
                        {alt_btn(true)}
                        <button
                            on:click=move |_| ret!(Home)
                            disabled=move || {
                                (!online && no_past()) || req_state!(Reset { .. }) >= Made
                            }
                            class:prominent=move || req_state!(Reset { .. }) == CanAccept
                            class:pushed=move || req_state!(Reset { .. }) == Made
                        >
                            {if online { "Reset" } else { "Home" }}
                        </button>
                        {(!online)
                            .then(|| {
                                view! {
                                    <button on:click=move |_| ret!(End) disabled=no_future>
                                        "End"
                                    </button>
                                }
                            })}
                    </div>
                    <div class="btn-group">
                        <button
                            on:click=move |_| ret!(Draw)
                            disabled=move || { ended() || req_state!(Draw) >= Made }
                            class:prominent=move || req_state!(Draw) == CanAccept
                            class:pushed=move || req_state!(Draw) == Made
                        >
                            "Draw"
                        </button>
                        <button on:click=move |_| ret!(Resign) disabled=ended>
                            "Resign"
                        </button>
                    </div>
                }
            };

            move || {
                if !alt_pushed.get() {
                    Either::Left(main_ctrl_view())
                } else {
                    Either::Right(alt_ctrl_view())
                }
            }
        };

        let maybe_auth_btn_or_ctrl_view = move || {
            if game_kind.get() == GameKind::Pending {
                EitherOf3::A(())
            } else if online && player.get().is_none() {
                EitherOf3::B(view! { <button on:click=move |_| ret!(Auth)>"Authenticate"</button> })
            } else {
                EitherOf3::C(ctrl_view())
            }
        };

        view! {
            <p class="title">"Game Menu"</p>
            <p style="font-family: monospace;">{info_view}</p>
            <div class="menu-btn-group">
                <button on:click=move |_| ret!(MainMenu)>"Main Menu"</button>
                {maybe_auth_btn_or_ctrl_view}
                <button autofocus>"Resume"</button>
            </div>
        }
    }
}

#[derive(Clone)]
pub struct ConfirmDialog(pub Confirm);

#[derive(Debug, Default, Eq, PartialEq)]
pub enum ConfirmRetVal {
    #[default]
    Cancel,
    Confirm,
    AltConfirm,
}

impl DialogView for ConfirmDialog {
    type RetVal = ConfirmRetVal;

    fn class(&self) -> Option<&'static str> {
        match self.0 {
            Confirm::Submit(_) | Confirm::Claim => Some("transparent"),
            _ => None,
        }
    }

    fn contents(self) -> impl IntoView {
        let mut title = None;
        let mut confirm = "Confirm";
        let mut cancel = Some("Cancel");
        let mut alt_confirm = None;

        let state = use_context::<Arc<AppState>>().unwrap();

        let message = match self.0 {
            Confirm::MainMenu => "Back to main menu?",
            Confirm::Submit(submit) => match submit {
                Submit::Pass => "Place no stone and pass?",
                Submit::OneAndPass => "Place one stone and pass?",
                Submit::One => "Place one stone?",
                Submit::Two => "Place two stones?",
            },
            Confirm::BeginClaim => {
                (confirm, cancel) = ("Noted", None);
                "To claim a win, click on one end of a six-in-a-row and then on the other end."
            }
            Confirm::Claim => match state.tentatives.get().len() {
                // TODO: Inform the user if they're claiming a win for the opponent?
                0 => "Claim a win?",
                1 => "Place one stone and claim a win?",
                _ => "Place two stones and claim a win?",
            },
            Confirm::RequestDraw => "Offer a draw?",
            Confirm::RequestRetract => "Request to retract the previous move?",
            Confirm::Requested(req) => {
                (confirm, cancel, alt_confirm) = ("Accept", Some("Ignore"), Some("Decline"));

                let player = state.player.get().unwrap();
                let options = state.options.get().unwrap();
                match req {
                    Request::Draw => "The opponent offers a draw.",
                    Request::Retract => "The opponent requests to retract the previous move.",
                    Request::Reset(new_options) => &format!(
                        "The opponent requests to reset the game. \
                         Your stone will {} {}.",
                        if new_options.swapped == options.swapped {
                            "remain"
                        } else {
                            "switch to"
                        },
                        new_options.stone_of(player)
                    ),
                }
            }
            Confirm::RequestAccepted => {
                (confirm, cancel) = ("Noted", None);
                "The opponent accepted your request."
            }
            Confirm::RequestDeclined => {
                (confirm, cancel) = ("Noted", None);
                "The opponent declined your request."
            }
            Confirm::Resign => {
                if state.game_kind.get().is_online() {
                    "Resign the game?"
                } else {
                    (confirm, alt_confirm) = ("White", Some("Black"));
                    "Resign for which stone?"
                }
            }
            Confirm::ConnClosed(ref reason) => {
                title = Some("Connection Closed");
                (confirm, cancel) = ("Retry", Some("Menu"));
                reason
            }
            Confirm::Error(ref message) => {
                title = Some("Error");
                (confirm, cancel) = ("Main Menu", None);
                message
            }
        };

        view! {
            {title.map(|s| view! { <p class="title">{s}</p> })}
            <p>{message.to_owned()}</p>
            <div class="btn-group">
                {cancel.map(|s| view! { <button>{s}</button> })}
                {alt_confirm
                    .map(|s| {
                        view! { <button on:click=move |_| ret!(AltConfirm)>{s}</button> }
                    })} <button on:click=move |_| ret!(Confirm)>{confirm}</button>
            </div>
        }
    }
}

#[derive(Clone)]
pub struct ResetDialog {
    pub player: Player,
    pub old_options: GameOptions,
}

#[derive(Debug, Default, Eq, PartialEq)]
pub enum ResetRetVal {
    #[default]
    Cancel,
    Confirm(GameOptions),
}

impl DialogView for ResetDialog {
    type RetVal = ResetRetVal;

    fn class(&self) -> Option<&'static str> {
        None
    }

    fn contents(self) -> impl IntoView {
        let old_stone = self.old_options.stone_of(self.player);
        let new_stone = RwSignal::new(old_stone.opposite());

        view! {
            <p>
                "Request to reset the game?"<br />"Playing: "{old_stone.to_string()}<br />
                "To play: "
                <input
                    type="radio"
                    id="black"
                    name="stone"
                    checked=old_stone == Stone::White
                    on:input=move |_| new_stone.set(Stone::Black)
                /> <label for="black">"Black"</label>
                <input
                    type="radio"
                    id="white"
                    name="stone"
                    checked=old_stone == Stone::Black
                    on:input=move |_| new_stone.set(Stone::White)
                /> <label for="white">"White"</label>
            </p>
            <div class="btn-group">
                <button>"Cancel"</button>
                <button on:click=move |_| {
                    let swapped = self.old_options.swapped ^ (old_stone != new_stone.get());
                    ret!(Confirm(GameOptions { swapped }));
                }>"Confirm"</button>
            </div>
        }
    }
}
