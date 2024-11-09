//! Game manager.

use c6ol_core::{
    game::{Move, Record, Stone},
    protocol::{ClientMessage, GameId, Passcode, Request, ServerMessage},
};
use rand::{distributions::Alphanumeric, Rng};
use std::{array, collections::HashMap, future::Future, iter};
use tokio::{
    sync::{broadcast, mpsc, oneshot},
    task::JoinSet,
};

/// Convenience macro for command execution.
macro_rules! execute {
    ($cmd_tx:expr, $variant:path, $($args:expr),*) => {{
        let (tx, rx) = oneshot::channel();
        $cmd_tx.send($variant(tx, $($args),*)).await.expect("receiver should be alive");
        rx.await.expect("command should return")
    }};
    ($cmd_tx:expr, $cmd:expr) => {
        $cmd_tx.send($cmd).await.expect("receiver should be alive")
    };
}

/// A subscription to a game.
pub struct GameSubscription {
    /// The initial messages.
    pub init_msgs: Box<[ServerMessage]>,
    /// The receiver for future messages.
    pub msg_rx: broadcast::Receiver<ServerMessage>,
}

enum GameCommand {
    Subscribe(oneshot::Sender<GameSubscription>),
    Authenticate(oneshot::Sender<Option<Stone>>, Passcode),
    Play(Stone, ClientMessage),
}

/// A command handle to a game.
pub struct Game {
    id: GameId,
    cmd_tx: mpsc::Sender<GameCommand>,
    stone: Option<Stone>,
}

impl Game {
    fn new(id: GameId, cmd_tx: mpsc::Sender<GameCommand>) -> Self {
        Self {
            id,
            cmd_tx,
            stone: None,
        }
    }

    /// Returns the game ID.
    pub fn id(&self) -> GameId {
        self.id
    }

    /// Subscribes to the game.
    pub async fn subscribe(&self) -> GameSubscription {
        execute!(self.cmd_tx, GameCommand::Subscribe,)
    }

    /// Attempts to authenticate with the given passcode.
    ///
    /// Returns the assigned stone, or `None` if authentication failed.
    ///
    /// # Panics
    ///
    /// Panics if the handle is already authenticated.
    pub async fn authenticate(&mut self, passcode: Passcode) -> Option<Stone> {
        assert!(self.stone.is_none(), "already authenticated");
        self.stone = execute!(self.cmd_tx, GameCommand::Authenticate, passcode);
        self.stone
    }

    /// Returns the assigned stone, or `None` if the handle is unauthenticated.
    pub fn stone(&self) -> Option<Stone> {
        self.stone
    }

    /// Attempts to play the game by making the action described in the message.
    ///
    /// # Panics
    ///
    /// Panics if the handle is unauthenticated.
    pub async fn play(&self, msg: ClientMessage) {
        let stone = self.stone.expect("unauthenticated");
        execute!(self.cmd_tx, GameCommand::Play(stone, msg));
    }
}

enum ManageCommand {
    New(oneshot::Sender<Game>),
    Find(oneshot::Sender<Option<Game>>, GameId),
}

/// Generates a random alphanumeric game ID.
fn rand_game_id() -> GameId {
    let mut rng = rand::thread_rng();
    array::from_fn(|_| rng.sample(Alphanumeric))
}

/// Creates a game manager.
///
/// Returns a command handle to it and a future to run it.
pub fn create() -> (GameManager, impl Future<Output = ()>) {
    let (cmd_tx, cmd_rx) = mpsc::channel::<ManageCommand>(100);
    (GameManager { cmd_tx }, manage_games(cmd_rx))
}

/// A command handle to a game manager.
#[derive(Clone)]
pub struct GameManager {
    cmd_tx: mpsc::Sender<ManageCommand>,
}

impl GameManager {
    /// Creates a new game.
    pub async fn new_game(&self) -> Game {
        execute!(self.cmd_tx, ManageCommand::New,)
    }

    /// Searches for a game with the given ID.
    pub async fn find_game(&self, id: GameId) -> Option<Game> {
        execute!(self.cmd_tx, ManageCommand::Find, id)
    }
}

async fn manage_games(mut cmd_rx: mpsc::Receiver<ManageCommand>) {
    tracing::info!("game manager started");

    let mut game_cmd_txs = HashMap::new();
    let mut game_tasks = JoinSet::new();

    loop {
        tokio::select! {
            opt = cmd_rx.recv() => {
                let Some(cmd) = opt else {
                    // All command senders are dropped.
                    break;
                };
                match cmd {
                    ManageCommand::New(resp_tx) => loop {
                        let id = rand_game_id();
                        if game_cmd_txs.contains_key(&id) {
                            continue;
                        }

                        let (game_cmd_tx, game_cmd_rx) = mpsc::channel(100);
                        game_cmd_txs.insert(id, game_cmd_tx.downgrade());
                        game_tasks.spawn(host_game(id, game_cmd_rx));

                        let _ = resp_tx.send(Game::new(id, game_cmd_tx));
                        break;
                    },
                    ManageCommand::Find(resp_tx, id) => {
                        // There is a chance that all senders have been dropped
                        // but the game task has not finished yet.
                        let resp = game_cmd_txs
                            .get(&id)
                            .and_then(|tx| tx.upgrade().map(|tx| Game::new(id, tx)));
                        let _ = resp_tx.send(resp);
                    }
                }
            }
            // When `join_next` returns `None`, `select!` will disable
            // this branch and still wait on the other branch.
            Some(res) = game_tasks.join_next() => {
                let id = res.expect("game task should not panic");
                game_cmd_txs.remove(&id);
            }
        }
    }

    // Wait for all game tasks to finish.
    while game_tasks.join_next().await.is_some() {}

    tracing::info!("game manager stopped");
}

struct GameState {
    msg_tx: broadcast::Sender<ServerMessage>,
    record: Record,
    passcode_black: Option<Passcode>,
    passcode_white: Option<Passcode>,
    requests: [Option<Stone>; Request::VALUES.len()],
}

impl GameState {
    fn new() -> Self {
        Self {
            msg_tx: broadcast::channel(100).0,
            record: Record::new(),
            passcode_black: None,
            passcode_white: None,
            requests: [None; Request::VALUES.len()],
        }
    }

    fn subscribe(&self) -> GameSubscription {
        GameSubscription {
            init_msgs: iter::once(ServerMessage::Record(Box::new(self.record.clone())))
                .chain(Request::VALUES.into_iter().filter_map(|req| {
                    self.requests[req as usize].map(|stone| ServerMessage::Request(req, stone))
                }))
                .collect(),
            msg_rx: self.msg_tx.subscribe(),
        }
    }

    fn authenticate(&mut self, passcode: Passcode) -> Option<Stone> {
        if let Some(passcode_black) = &self.passcode_black {
            if passcode == *passcode_black {
                Some(Stone::Black)
            } else if let Some(passcode_white) = &self.passcode_white {
                if passcode == *passcode_white {
                    Some(Stone::White)
                } else {
                    None
                }
            } else {
                self.passcode_white = Some(passcode);
                Some(Stone::White)
            }
        } else {
            self.passcode_black = Some(passcode);
            Some(Stone::Black)
        }
    }

    fn play(&mut self, stone: Stone, msg: ClientMessage) {
        use ClientMessage as Msg;

        enum Action {
            Move(Move),
            Retract,
            Reset,
        }

        let action = match msg {
            Msg::Start(_) | Msg::Join(_) => return,
            Msg::Place(fst, snd) => {
                if self.record.turn() != Some(stone) {
                    // Not their turn.
                    return;
                }
                Action::Move(Move::Stone(fst, snd))
            }
            Msg::Pass => {
                if self.record.turn() != Some(stone) {
                    // Not their turn.
                    return;
                }
                Action::Move(Move::Pass)
            }
            Msg::ClaimWin(pos) => Action::Move(Move::Win(pos)),
            Msg::Resign => Action::Move(Move::Resign(stone)),
            Msg::Request(req) => {
                let req_stone = &mut self.requests[req as usize];
                if *req_stone == Some(stone) {
                    // Duplicate request.
                    return;
                }

                if matches!(req, Request::Retract | Request::Reset) && !self.record.has_past() {
                    // No moves in the past.
                    return;
                }

                if req_stone.is_none() {
                    // No request present, make one.
                    *req_stone = Some(stone);
                    let _ = self.msg_tx.send(ServerMessage::Request(req, stone));
                    return;
                }

                match req {
                    Request::Draw => Action::Move(Move::Draw),
                    Request::Retract => Action::Retract,
                    Request::Reset => Action::Reset,
                }
            }
        };

        let msg = match action {
            Action::Move(mov) => {
                if !self.record.make_move(mov) {
                    // The move failed.
                    return;
                }
                ServerMessage::Move(mov)
            }
            Action::Retract => {
                // We have checked that there is a previous move.
                self.record.undo_move();
                ServerMessage::Retract
            }
            Action::Reset => {
                // We have checked that there is a previous move.
                self.record.jump(0);
                ServerMessage::Record(Box::new(Record::new()))
            }
        };

        // Clear the requests.
        self.requests.fill(None);
        let _ = self.msg_tx.send(msg);
    }
}

async fn host_game(id: GameId, mut cmd_rx: mpsc::Receiver<GameCommand>) -> GameId {
    tracing::debug!("game started: {}", id.escape_ascii());

    let mut state = GameState::new();
    while let Some(cmd) = cmd_rx.recv().await {
        match cmd {
            GameCommand::Subscribe(resp_tx) => {
                let _ = resp_tx.send(state.subscribe());
            }
            GameCommand::Authenticate(resp_tx, pass) => {
                let _ = resp_tx.send(state.authenticate(pass));
            }
            GameCommand::Play(stone, msg) => state.play(stone, msg),
        }
    }

    // All command senders are dropped.
    tracing::debug!("game ended: {}", id.escape_ascii());
    id
}
