use crate::{Confirm, ANALYZE_PREFIX};
use base64::prelude::*;
use c6ol_core::{
    game::{Move, Record, Stone},
    protocol::Request,
};
use leptos::{
    either::{Either, EitherOf5},
    html,
    prelude::*,
};
use serde::{Deserialize, Serialize};

trait DialogImpl {
    type RetVal;

    fn class(&self) -> Option<&'static str> {
        None
    }

    fn inner_view(self) -> impl IntoView;
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
                                $either_type::$either_variant(dialog.inner_view()),
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
    EitherType = EitherOf5,
    MainMenu => A,
    OnlineMenu => B,
    Join => C,
    GameMenu => D,
    Confirm => E,
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

    fn inner_view(self) -> impl IntoView {
        view! {
            <p class="title">"Main Menu"</p>
            <div class="menu-btn-group">
                <button>"Play Offline"</button>
                {
                    #[cfg(feature = "online")]
                    view! { <button value=ret!(Online)>"Play Online"</button> }
                }
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
    Start(String),
    Join(String),
}

impl DialogImpl for OnlineMenuDialog {
    type RetVal = OnlineMenuRetVal;

    fn inner_view(self) -> impl IntoView {
        let start_checked = RwSignal::new(true);
        let passcode = RwSignal::new(String::new());
        let game_id = RwSignal::new(String::new());

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
            {move || {
                if start_checked.get() {
                    Either::Left(
                        view! {
                            <label for="passcode">"Passcode: "</label>
                            <input
                                type="text"
                                id="passcode"
                                required
                                autocomplete="on"
                                placeholder="Yours, not shared"
                                bind:value=passcode
                            />
                        },
                    )
                } else {
                    Either::Right(
                        view! {
                            <label for="game-id">"Game ID: "</label>
                            <input
                                type="text"
                                id="game-id"
                                required
                                pattern="[0-9A-Za-z]{10}"
                                autocomplete="on"
                                placeholder="10 alphanumerics"
                                bind:value=game_id
                            />
                        },
                    )
                }
            }}
            <div class="btn-group reversed">
                <button value=move || {
                    if start_checked.get() {
                        ret!(Start(passcode.get()))
                    } else {
                        ret!(Join(game_id.get()))
                    }
                }>{move || if start_checked.get() { "Start" } else { "Join" }}</button>
                <button formnovalidate>"Cancel"</button>
            </div>
        }
    }
}

#[derive(Clone)]
pub struct JoinDialog;

#[derive(Debug, Default, Deserialize, Serialize)]
pub enum JoinRetVal {
    #[default]
    ViewOnly,
    Join(String),
}

impl DialogImpl for JoinDialog {
    type RetVal = JoinRetVal;

    fn inner_view(self) -> impl IntoView {
        let passcode = RwSignal::new(String::new());

        view! {
            <p class="title">"Join Game"</p>
            <label for="passcode">"Passcode: "</label>
            <input
                type="text"
                id="passcode"
                autocomplete="on"
                required
                placeholder="Yours, not shared"
                bind:value=passcode
            />
            <div class="btn-group reversed">
                <button value=move || ret!(Join(passcode.get()))>"Join"</button>
                <button formnovalidate>"View Only"</button>
            </div>
        }
    }
}

#[derive(Clone)]
pub struct GameMenuDialog {
    pub game_id: String,
    pub stone: Option<Stone>,
    pub online: bool,
    pub record: ReadSignal<Record>,
    pub requests: ReadSignal<[Option<Stone>; Request::VALUES.len()]>,
}

#[derive(Debug, Default, Deserialize, Serialize)]
pub enum GameMenuRetVal {
    #[default]
    Resume,
    MainMenu,
    Join,
    Undo,
    Redo,
    Home,
    End,
    ClaimWin,
    Resign,
    Pass,
    Draw,
}

impl DialogImpl for GameMenuDialog {
    type RetVal = GameMenuRetVal;

    fn class(&self) -> Option<&'static str> {
        Some("game-menu")
    }

    fn inner_view(self) -> impl IntoView {
        let Self {
            game_id,
            stone,
            online,
            record,
            requests,
        } = self;

        let info_view = view! {
            {if game_id == "local" {
                Either::Left("Offline")
            } else if game_id.starts_with(ANALYZE_PREFIX) {
                Either::Left("Analyzing")
            } else {
                let href = format!("#{game_id}");
                Either::Right(
                    view! {
                        <a href=href>{game_id}</a>
                        <br />
                        {if let Some(stone) = stone {
                            format!("Playing {stone:?}")
                        } else {
                            "View Only".into()
                        }}
                    },
                )
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
                    Move::Win(pos) => {
                        let stone = record.stone_at(pos).unwrap();
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
                    record.read().encode(&mut buf, false);
                    format!("#{ANALYZE_PREFIX}{}", BASE64_STANDARD.encode(buf))
                }
            >
                "Analyze"
            </a>
        };

        let join_btn_or_ctrl_view = if online && stone.is_none() {
            Either::Left(view! { <button value=ret!(Join)>"Join"</button> })
        } else {
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

            #[derive(Eq, PartialEq)]
            enum Side {
                Neither,
                User,
                Opponent,
            }

            use Request::*;
            use Side::*;

            let who_requested = move |req| match (stone, requests.read()[req as usize]) {
                (None, _) | (_, None) => Neither,
                (Some(a), Some(b)) if a == b => User,
                _ => Opponent,
            };

            let ctrl_view = move || {
                view! {
                    <div class="btn-group">
                        {alt_btn(false)}
                        <button
                            value=ret!(Undo)
                            disabled=move || no_past() || who_requested(Retract) == User
                            class:prominent=move || who_requested(Retract) == Opponent
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
                        <button value=ret!(ClaimWin) disabled=ended>
                            "Claim Win"
                        </button>
                        <button value=ret!(Resign) disabled=ended>
                            "Resign"
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
                            disabled=move || no_past() || who_requested(Reset) == User
                            class:prominent=move || who_requested(Reset) == Opponent
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
                            value=ret!(Pass)
                            disabled=move || ended() || record.read().turn() != stone
                        >
                            "Pass"
                        </button>
                        <button
                            value=ret!(Draw)
                            disabled=move || ended() || who_requested(Draw) == User
                            class:prominent=move || who_requested(Draw) == Opponent
                        >
                            "Draw"
                        </button>
                    </div>
                }
            };

            Either::Right(move || {
                if !alt_pushed.get() {
                    Either::Left(ctrl_view())
                } else {
                    Either::Right(alt_ctrl_view())
                }
            })
        };

        view! {
            <p class="title">"Game Menu"</p>
            <p style="font-family: monospace;">{info_view}</p>
            <div class="menu-btn-group">
                <button value=ret!(MainMenu)>"Main Menu"</button>
                {join_btn_or_ctrl_view}
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
}

impl DialogImpl for ConfirmDialog {
    type RetVal = ConfirmRetVal;

    fn class(&self) -> Option<&'static str> {
        match self.0 {
            Confirm::ConnClosed(_) | Confirm::Error(_) => None,
            _ => Some("transparent"),
        }
    }

    fn inner_view(self) -> impl IntoView {
        let mut title = None;
        let mut confirm = Some("Confirm");
        let mut cancel = "Cancel";

        let message = match &self.0 {
            Confirm::MainMenu => "Back to main menu?",
            Confirm::Submit(_, _) => "Submit the move?",
            Confirm::Pass => "Pass without placing stones?",
            Confirm::PlaceSingleStone(_) => "Place a single stone?",
            Confirm::Request(req) => match req {
                Request::Draw => "Offer a draw?",
                Request::Retract => "Request to retract the previous move?",
                Request::Reset => "Request to reset the game?",
            },
            Confirm::Accept(req) => {
                (confirm, cancel) = (Some("Accept"), "Ignore");
                match req {
                    Request::Draw => "The opponent offers a draw.",
                    Request::Retract => "The opponent requests to retract the previous move.",
                    Request::Reset => "The opponent requests to reset the game.",
                }
            }
            Confirm::Resign => "Resign the game?",
            Confirm::ConnClosed(reason) => {
                title = Some("Connection Closed");
                (confirm, cancel) = (Some("Retry"), "Menu");
                reason
            }
            Confirm::Error(message) => {
                title = Some("Error");
                (confirm, cancel) = (None, "Main Menu");
                message
            }
        };

        view! {
            {title.map(|s| view! { <p class="title">{s}</p> })}
            <p>{message.to_owned()}</p>
            <div class="btn-group">
                <button>{cancel}</button>
                {confirm.map(|s| view! { <button value=ret!(Confirm)>{s}</button> })}
            </div>
        }
    }
}
