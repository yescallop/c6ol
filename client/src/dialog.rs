use crate::{ANALYZE_PREFIX, Confirm, WinClaim};
use base64::prelude::*;
use c6ol_core::{
    game::{Move, Player, PlayerSlots, Record, RecordEncodeMethod, Stone},
    protocol::{GameOptions, Request},
};
use leptos::{
    either::{Either, EitherOf3, EitherOf6},
    html,
    prelude::*,
};
use serde::{Deserialize, Serialize};

trait DialogImpl {
    type RetVal;

    fn class(&self) -> Option<&'static str> {
        None
    }

    fn contents(self) -> impl IntoView;
}

macro_rules! ret {
    ($($val:tt)+) => {
        ron::to_string(&Self::RetVal::$($val)+).unwrap()
    };
}

macro_rules! dialogs {
    (
        EitherType = $either_type:ty,
        $($name:ident => $either_variant:ident,)+
    ) => {
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

            impl Dialog {
                pub fn show(self, id: u32, on_return: impl Fn(u32, RetVal) + 'static) -> impl IntoView {
                    let dialog_ref = NodeRef::<html::Dialog>::new();

                    Effect::new(move || {
                        dialog_ref.get().unwrap().show_modal().unwrap();
                    });

                    let (ret_val_from_str, class, inner_view) = match self {
                        $(
                            Dialog::$name(dialog) => (
                                (|s| RetVal::$name(ron::from_str(s).unwrap_or_default()))
                                    as fn(&str) -> RetVal,
                                dialog.class(),
                                $either_type::$either_variant(dialog.contents()),
                            ),
                        )+
                    };

                    let on_close = move |_| {
                        let dialog = dialog_ref.get().unwrap();
                        on_return(id, ret_val_from_str(&dialog.return_value()));
                    };

                    view! {
                        <dialog node_ref=dialog_ref class=class on:close=on_close>
                            <form method="dialog">{inner_view}</form>
                        </dialog>
                    }
                }
            }
        }
    };
}

dialogs! {
    EitherType = EitherOf6,
    MainMenu => A,
    OnlineMenu => B,
    Auth => C,
    GameMenu => D,
    Confirm => E,
    Reset => F,
}

#[derive(Clone)]
pub struct MainMenuDialog;

#[derive(Debug, Default, Deserialize, Serialize)]
pub enum MainMenuRetVal {
    #[default]
    Offline,
    Online,
}

impl DialogImpl for MainMenuDialog {
    type RetVal = MainMenuRetVal;

    fn contents(self) -> impl IntoView {
        view! {
            <p class="title">"Main Menu"</p>
            <div class="menu-btn-group">
                <button>"Play Offline"</button>
                {
                    #[cfg(feature = "online")]
                    view! { <button value=ret!(Online)>"Play Online"</button> }
                }
                <a target="_blank" href="https://github.com/yescallop/c6ol">
                    <button type="button">"Source Code"</button>
                </a>
            </div>
        }
    }
}

#[derive(Clone)]
pub struct OnlineMenuDialog;

#[derive(Debug, Default, Deserialize, Serialize)]
pub enum OnlineMenuRetVal {
    #[default]
    Cancel,
    Start(GameOptions),
    Join(String),
}

impl DialogImpl for OnlineMenuDialog {
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
                        <button value=move || {
                            let options = GameOptions {
                                swapped: stone.get() == Stone::White,
                            };
                            ret!(Start(options))
                        }>"Start"</button>
                        <button formnovalidate>"Cancel"</button>
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
                        <button value=move || ret!(Join(game_id.get()))>"Join"</button>
                        <button formnovalidate>"Cancel"</button>
                    </div>
                };
                Either::Right(view)
            }
        };

        view! {
            <p class="title">"Play Online"</p>
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

#[derive(Debug, Default, Deserialize, Serialize)]
pub enum AuthRetVal {
    #[default]
    ViewOnly,
    Submit(String),
}

impl DialogImpl for AuthDialog {
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
                <button value=move || ret!(Submit(passcode.get()))>"Submit"</button>
                <button formnovalidate>"View Only"</button>
            </div>
        }
    }
}

#[derive(Clone)]
pub struct GameMenuDialog {
    pub game_id: ReadSignal<String>,
    pub stone: ReadSignal<Option<Stone>>,
    pub online: bool,
    pub player: ReadSignal<Option<Player>>,
    pub record: ReadSignal<Record>,
    pub win_claim: ReadSignal<Option<WinClaim>>,
    pub requests: ReadSignal<PlayerSlots<Option<Request>>>,
}

#[derive(Debug, Default, Deserialize, Serialize)]
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

impl DialogImpl for GameMenuDialog {
    type RetVal = GameMenuRetVal;

    fn class(&self) -> Option<&'static str> {
        Some("game-menu")
    }

    fn contents(self) -> impl IntoView {
        let Self {
            game_id,
            stone,
            online,
            player,
            record,
            win_claim,
            requests,
        } = self;

        let info_view = view! {
            {move || {
                let id = game_id.read();
                if id.is_empty() {
                    Either::Left("Pending")
                } else if *id == "local" {
                    Either::Left("Offline")
                } else if id.starts_with(ANALYZE_PREFIX) {
                    Either::Left("Analyzing")
                } else {
                    let href = format!("#{id}");
                    Either::Right(
                        view! {
                            <a href=href>{id.clone()}</a>
                            <br />
                            {move || match stone.get() {
                                Some(stone) => format!("Playing {stone:?}"),
                                None => "View Only".into(),
                            }}
                        },
                    )
                }
            }}
            <br />
            {move || {
                let record = record.read();
                if let Some(stone) = record.turn() {
                    return format!("{stone:?} to Play");
                }
                match record.prev_move().unwrap() {
                    Move::Draw => "Game Drawn".into(),
                    Move::Resign(stone) => format!("{stone:?} Resigned"),
                    Move::Win(p, _) => {
                        let stone = record.stone_at(p).unwrap();
                        format!("{stone:?} Won")
                    }
                    _ => unreachable!(),
                }
            }}
            <br />
            <a
                target="_blank"
                href=move || {
                    let mut buf = vec![];
                    record.read().encode(&mut buf, RecordEncodeMethod::Past);
                    format!("#{ANALYZE_PREFIX}{}", BASE64_STANDARD.encode(buf))
                }
            >
                "Analyze"
            </a>
        };

        let ctrl_view = move || {
            let alt_pushed = RwSignal::new(false);

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
                            value=ret!(Undo)
                            disabled=move || { no_past() || req_state!(Retract) >= Made }
                            class:prominent=move || req_state!(Retract) == CanAccept
                            class:pushed=move || req_state!(Retract) == Made
                        >
                            {if online { "Retract" } else { "Undo" }}
                        </button>
                        {(!online)
                            .then(|| {
                                view! {
                                    <button value=ret!(Redo) disabled=no_future>
                                        "Redo"
                                    </button>
                                }
                            })}
                    </div>
                    <div class="btn-group">
                        <button
                            class:pushed=move || win_claim.read().is_some()
                            value=ret!(ClaimWin)
                            disabled=ended
                        >
                            "Claim Win"
                        </button>
                        <button
                            value=ret!(Submit)
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
                            value=ret!(Home)
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
                                    <button value=ret!(End) disabled=no_future>
                                        "End"
                                    </button>
                                }
                            })}
                    </div>
                    <div class="btn-group">
                        <button
                            value=ret!(Draw)
                            disabled=move || { ended() || req_state!(Draw) >= Made }
                            class:prominent=move || req_state!(Draw) == CanAccept
                            class:pushed=move || req_state!(Draw) == Made
                        >
                            "Draw"
                        </button>
                        <button value=ret!(Resign) disabled=ended>
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
            if game_id.read().is_empty() {
                EitherOf3::A(())
            } else if online && player.get().is_none() {
                EitherOf3::B(view! { <button value=ret!(Auth)>"Authenticate"</button> })
            } else {
                EitherOf3::C(ctrl_view())
            }
        };

        view! {
            <p class="title">"Game Menu"</p>
            <p style="font-family: monospace;">{info_view}</p>
            <div class="menu-btn-group">
                <button value=ret!(MainMenu)>"Main Menu"</button>
                {maybe_auth_btn_or_ctrl_view}
                <button autofocus>"Resume"</button>
            </div>
        }
    }
}

#[derive(Clone)]
pub struct ConfirmDialog(pub Confirm);

#[derive(Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub enum ConfirmRetVal {
    #[default]
    Cancel,
    Confirm,
    AltConfirm,
}

impl DialogImpl for ConfirmDialog {
    type RetVal = ConfirmRetVal;

    fn class(&self) -> Option<&'static str> {
        match self.0 {
            Confirm::Submit(..) | Confirm::Pass(_) | Confirm::Claim(..) => Some("transparent"),
            _ => None,
        }
    }

    fn contents(self) -> impl IntoView {
        let mut title = None;
        let mut confirm = "Confirm";
        let mut cancel = Some("Cancel");
        let mut alt_confirm = None;

        let message = match self.0 {
            Confirm::MainMenu => "Back to main menu?",
            Confirm::Submit(_, None) => "Place one stone?",
            Confirm::Submit(_, Some(_)) => "Place two stones?",
            Confirm::Pass(None) => "Place no stone and pass?",
            Confirm::Pass(Some(_)) => "Place one stone and pass?",
            Confirm::BeginClaim => {
                (confirm, cancel) = ("Noted", None);
                "To claim a win, click on one end of a six-in-a-row and then on the other end."
            }
            Confirm::Claim(tentatives, ..) => match tentatives.len() {
                // TODO: Inform the user if they're claiming a win for the opponent?
                0 => "Claim a win?",
                1 => "Place one stone and claim a win?",
                _ => "Place two stones and claim a win?",
            },
            Confirm::RequestDraw => "Offer a draw?",
            Confirm::RequestRetract => "Request to retract the previous move?",
            Confirm::Requested(player, req) => {
                (confirm, cancel, alt_confirm) = ("Accept", Some("Ignore"), Some("Decline"));

                match req {
                    Request::Draw => "The opponent offers a draw.",
                    Request::Retract => "The opponent requests to retract the previous move.",
                    Request::Reset(options) => &format!(
                        "The opponent requests to reset the game. \
                         After the reset, you're to play {:?}.",
                        options.stone_of(player)
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
            Confirm::Resign => "Resign the game?",
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
                {alt_confirm.map(|s| view! { <button value=ret!(AltConfirm)>{s}</button> })}
                <button value=ret!(Confirm)>{confirm}</button>
            </div>
        }
    }
}

#[derive(Clone)]
pub struct ResetDialog {
    pub player: Player,
    pub old_options: GameOptions,
}

#[derive(Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub enum ResetRetVal {
    #[default]
    Cancel,
    Confirm(GameOptions),
}

impl DialogImpl for ResetDialog {
    type RetVal = ResetRetVal;

    fn class(&self) -> Option<&'static str> {
        None
    }

    fn contents(self) -> impl IntoView {
        let old_stone = self.old_options.stone_of(self.player);
        let new_stone = RwSignal::new(old_stone);

        view! {
            <p>
                "Request to reset the game?"<br />"To play: "
                <input
                    type="radio"
                    id="black"
                    name="stone"
                    checked=old_stone == Stone::Black
                    on:input=move |_| new_stone.set(Stone::Black)
                /> <label for="black">"Black"</label>
                <input
                    type="radio"
                    id="white"
                    name="stone"
                    checked=old_stone == Stone::White
                    on:input=move |_| new_stone.set(Stone::White)
                /> <label for="white">"White"</label>
            </p>
            <div class="btn-group">
                <button>"Cancel"</button>
                <button value=move || {
                    let swapped = self.old_options.swapped ^ (old_stone != new_stone.get());
                    ret!(Confirm(GameOptions { swapped }))
                }>"Confirm"</button>
            </div>
        }
    }
}
