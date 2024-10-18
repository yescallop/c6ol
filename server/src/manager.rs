//! Game manager.

use crate::{
    game::{Move, Record, Stone},
    protocol::{ClientMessage, GameId, Passcode, ServerMessage},
};
use rand::{distributions::Alphanumeric, Rng};
use std::{array, collections::HashMap, future::Future, iter};
use tokio::sync::{broadcast, mpsc, oneshot};

/// A subscription to a game.
pub struct GameSubscription {
    /// The initial messages.
    pub init_msgs: Box<[ServerMessage]>,
    /// The receiver for future messages.
    pub msg_rx: broadcast::Receiver<ServerMessage>,
}

enum GameCommand {
    Subscribe(oneshot::Sender<GameSubscription>),
    Authenticate(Passcode, oneshot::Sender<Option<Stone>>),
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

    async fn exec<T>(&self, cmd: GameCommand, rx: oneshot::Receiver<T>) -> T {
        self.cmd_tx
            .send(cmd)
            .await
            .expect("command receiver should be alive");
        rx.await.expect("command should return")
    }

    /// Returns the game ID.
    pub fn id(&self) -> GameId {
        self.id
    }

    /// Subscribes to the game.
    pub async fn subscribe(&self) -> GameSubscription {
        let (tx, rx) = oneshot::channel();
        self.exec(GameCommand::Subscribe(tx), rx).await
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
        let (tx, rx) = oneshot::channel();
        self.stone = self.exec(GameCommand::Authenticate(passcode, tx), rx).await;
        self.stone
    }

    /// Returns the assigned stone, if authenticated.
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
        self.cmd_tx
            .send(GameCommand::Play(stone, msg))
            .await
            .expect("command receiver should be alive");
    }
}

enum ManageCommand {
    New(oneshot::Sender<Game>),
    Find(GameId, oneshot::Sender<Option<Game>>),
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
    async fn exec<T>(&self, cmd: ManageCommand, rx: oneshot::Receiver<T>) -> T {
        self.cmd_tx
            .send(cmd)
            .await
            .expect("command receiver should be alive");
        rx.await.expect("command should return")
    }

    /// Creates a new game.
    pub async fn new_game(&self) -> Game {
        let (tx, rx) = oneshot::channel();
        self.exec(ManageCommand::New(tx), rx).await
    }

    /// Searches for a game with the given ID.
    pub async fn find_game(&self, id: GameId) -> Option<Game> {
        let (tx, rx) = oneshot::channel();
        self.exec(ManageCommand::Find(id, tx), rx).await
    }
}

async fn manage_games(mut cmd_rx: mpsc::Receiver<ManageCommand>) {
    tracing::debug!("game manager started");

    let mut games = HashMap::new();
    let (cleanup_tx, mut cleanup_rx) = mpsc::channel(100);

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
                        if games.contains_key(&id) {
                            continue;
                        }

                        let (game_cmd_tx, game_cmd_rx) = mpsc::channel(100);
                        games.insert(id, game_cmd_tx.downgrade());
                        tokio::spawn(host_game(id, game_cmd_rx, cleanup_tx.clone()));

                        let _ = resp_tx.send(Game::new(id, game_cmd_tx));
                        break;
                    },
                    ManageCommand::Find(id, resp_tx) => {
                        // There is a chance that we have not yet received a cleanup message.
                        let resp = games
                            .get(&id)
                            .and_then(|tx| tx.upgrade().map(|tx| Game::new(id, tx)));
                        let _ = resp_tx.send(resp);
                    }
                }
            }
            opt = cleanup_rx.recv() => {
                let id = opt.expect("at least one sender should be alive");
                games.remove(&id);
            }
        }
    }

    // Wait for all game futures to complete.
    drop(cleanup_tx);
    while cleanup_rx.recv().await.is_some() {}

    tracing::debug!("game manager stopped");
}

struct GameState {
    msg_tx: broadcast::Sender<ServerMessage>,
    rec: Record,
    pass_black: Option<Passcode>,
    pass_white: Option<Passcode>,
    req_draw: Option<Stone>,
    req_retract: Option<Stone>,
}

impl GameState {
    fn new() -> Self {
        Self {
            msg_tx: broadcast::channel(100).0,
            rec: Record::new(),
            pass_black: None,
            pass_white: None,
            req_draw: None,
            req_retract: None,
        }
    }

    fn subscribe(&self) -> GameSubscription {
        GameSubscription {
            init_msgs: iter::once(ServerMessage::Record(Box::new(self.rec.clone())))
                .chain(self.req_draw.map(ServerMessage::RequestDraw))
                .chain(self.req_retract.map(ServerMessage::RequestRetract))
                .collect(),
            msg_rx: self.msg_tx.subscribe(),
        }
    }

    fn authenticate(&mut self, pass: Passcode) -> Option<Stone> {
        if let Some(pass_black) = &self.pass_black {
            if pass == *pass_black {
                Some(Stone::Black)
            } else if let Some(pass_white) = &self.pass_white {
                if pass == *pass_white {
                    Some(Stone::White)
                } else {
                    None
                }
            } else {
                self.pass_white = Some(pass);
                Some(Stone::White)
            }
        } else {
            self.pass_black = Some(pass);
            Some(Stone::Black)
        }
    }

    fn play(&mut self, stone: Stone, msg: ClientMessage) {
        use ClientMessage as Msg;

        enum Action {
            Move(Move),
            Retract,
        }

        let action = match msg {
            Msg::Start(_) | Msg::Join(_) => return,
            Msg::Place(fst, snd) => {
                if self.rec.turn() != stone {
                    return;
                }
                Action::Move(Move::Stone(fst, snd))
            }
            Msg::Pass => {
                if self.rec.turn() != stone {
                    return;
                }
                Action::Move(Move::Pass)
            }
            Msg::ClaimWin(pos) => Action::Move(Move::Win(pos)),
            Msg::Resign => Action::Move(Move::Resign(stone)),
            Msg::RequestDraw => {
                if self.req_draw == Some(stone) {
                    // Duplicate request.
                    return;
                }
                if self.req_draw.is_none() {
                    // No request present, make one.
                    self.req_draw = Some(stone);
                    let _ = self.msg_tx.send(ServerMessage::RequestDraw(stone));
                    return;
                }
                Action::Move(Move::Draw)
            }
            Msg::RequestRetract => {
                if !self.rec.has_past() || self.req_retract == Some(stone) {
                    // No move in the past or duplicate request.
                    return;
                }
                if self.req_retract.is_none() {
                    // No request present, make one.
                    self.req_retract = Some(stone);
                    let _ = self.msg_tx.send(ServerMessage::RequestRetract(stone));
                    return;
                }
                Action::Retract
            }
        };

        let msg = match action {
            Action::Move(mov) => {
                if !self.rec.make_move(mov) {
                    return;
                }
                ServerMessage::Move(mov)
            }
            Action::Retract => {
                self.rec.undo_move();
                ServerMessage::Retract
            }
        };

        self.req_draw = None;
        self.req_retract = None;
        let _ = self.msg_tx.send(msg);
    }
}

async fn host_game(
    id: GameId,
    mut cmd_rx: mpsc::Receiver<GameCommand>,
    cleanup_tx: mpsc::Sender<GameId>,
) {
    tracing::debug!("game started: {}", id.escape_ascii());
    let mut state = GameState::new();
    while let Some(cmd) = cmd_rx.recv().await {
        match cmd {
            GameCommand::Subscribe(resp_tx) => {
                let _ = resp_tx.send(state.subscribe());
            }
            GameCommand::Authenticate(pass, resp_tx) => {
                let _ = resp_tx.send(state.authenticate(pass));
            }
            GameCommand::Play(stone, msg) => state.play(stone, msg),
        }
    }
    // All command senders are dropped.
    tracing::debug!("game ended: {}", id.escape_ascii());
    let _ = cleanup_tx.send(id).await;
}
