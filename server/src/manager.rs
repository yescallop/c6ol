//! Game manager.

use c6ol_core::{
    game::{Move, Player, PlayerSlots, Record},
    protocol::{ClientMessage, GameId, GameOptions, Passcode, Request, ServerMessage},
};
use rand::{distributions::Alphanumeric, Rng};
use std::{array, collections::HashMap, future::Future};
use tokio::{
    sync::{broadcast, mpsc, oneshot},
    task::JoinSet,
};

const CHANNEL_CAPACITY_MANAGE_CMD: usize = 64;
const CHANNEL_CAPACITY_GAME_CMD: usize = 8;
const CHANNEL_CAPACITY_GAME_MSG: usize = 8;

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
    Authenticate(oneshot::Sender<Option<Player>>, Passcode),
    Play(Player, ClientMessage),
}

/// A command handle to a game.
pub struct Game {
    id: GameId,
    cmd_tx: mpsc::Sender<GameCommand>,
    player: Option<Player>,
}

impl Game {
    fn new(id: GameId, cmd_tx: mpsc::Sender<GameCommand>) -> Self {
        Self {
            id,
            cmd_tx,
            player: None,
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
    /// Returns the assigned player, or `None` if authentication failed.
    ///
    /// # Panics
    ///
    /// Panics if the handle is already authenticated.
    pub async fn authenticate(&mut self, passcode: Passcode) -> Option<Player> {
        assert!(self.player.is_none(), "already authenticated");
        self.player = execute!(self.cmd_tx, GameCommand::Authenticate, passcode);
        self.player
    }

    /// Returns the assigned player, or `None` if the handle is unauthenticated.
    pub fn player(&self) -> Option<Player> {
        self.player
    }

    /// Attempts to play the game by making the action described in the message.
    ///
    /// # Panics
    ///
    /// Panics if the handle is unauthenticated.
    pub async fn play(&self, msg: ClientMessage) {
        let player = self.player.expect("unauthenticated");
        execute!(self.cmd_tx, GameCommand::Play(player, msg));
    }
}

enum ManageCommand {
    New(oneshot::Sender<Game>, GameOptions),
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
    let (cmd_tx, cmd_rx) = mpsc::channel(CHANNEL_CAPACITY_MANAGE_CMD);
    (GameManager { cmd_tx }, manage_games(cmd_rx))
}

/// A command handle to a game manager.
#[derive(Clone)]
pub struct GameManager {
    cmd_tx: mpsc::Sender<ManageCommand>,
}

impl GameManager {
    /// Creates a new game with the given options.
    pub async fn new_game(&self, options: GameOptions) -> Game {
        execute!(self.cmd_tx, ManageCommand::New, options)
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
    let mut game_ids_by_task_id = HashMap::new();

    loop {
        tokio::select! {
            opt = cmd_rx.recv() => {
                let Some(cmd) = opt else {
                    // All command senders are dropped.
                    break;
                };
                match cmd {
                    ManageCommand::New(resp_tx, options) => loop {
                        let id = rand_game_id();
                        if game_cmd_txs.contains_key(&id) {
                            continue;
                        }

                        let (game_cmd_tx, game_cmd_rx) = mpsc::channel(CHANNEL_CAPACITY_GAME_CMD);
                        game_cmd_txs.insert(id, game_cmd_tx.downgrade());

                        let task_id = game_tasks.spawn(start_game(id, options, game_cmd_rx)).id();
                        game_ids_by_task_id.insert(task_id, id);

                        _ = resp_tx.send(Game::new(id, game_cmd_tx));
                        break;
                    },
                    ManageCommand::Find(resp_tx, id) => {
                        // There is a chance that all senders have been dropped
                        // but the game task has not finished yet.
                        let resp = game_cmd_txs
                            .get(&id)
                            .and_then(|tx| tx.upgrade().map(|tx| Game::new(id, tx)));
                        _ = resp_tx.send(resp);
                    }
                }
            }
            // When `join_next` returns `None`, `select!` will disable
            // this branch and still wait on the other branch.
            Some(res) = game_tasks.join_next_with_id() => {
                let task_id = match res {
                    Ok((id, ())) => id,
                    Err(err) => {
                        tracing::error!("game task panicked: {err}");
                        err.id()
                    },
                };
                let game_id = game_ids_by_task_id.remove(&task_id).unwrap();
                game_cmd_txs.remove(&game_id);
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
    options: GameOptions,
    passcodes: PlayerSlots<Option<Passcode>>,
    requests: PlayerSlots<Option<Request>>,
}

impl GameState {
    fn new() -> Self {
        Self {
            msg_tx: broadcast::channel(CHANNEL_CAPACITY_GAME_MSG).0,
            record: Record::new(),
            options: Default::default(),
            passcodes: Default::default(),
            requests: Default::default(),
        }
    }

    fn subscribe(&self) -> GameSubscription {
        GameSubscription {
            init_msgs: [
                ServerMessage::Options(self.options),
                ServerMessage::Record(Box::new(self.record.clone())),
            ]
            .into_iter()
            .chain([Player::Host, Player::Guest].iter().filter_map(|&player| {
                self.requests[player].map(|req| ServerMessage::Request(player, req))
            }))
            .collect(),
            msg_rx: self.msg_tx.subscribe(),
        }
    }

    fn authenticate(&mut self, passcode: Passcode) -> Option<Player> {
        if let Some(passcode_host) = &self.passcodes[Player::Host] {
            if passcode == *passcode_host {
                Some(Player::Host)
            } else if let Some(passcode_guest) = &self.passcodes[Player::Guest] {
                if passcode == *passcode_guest {
                    Some(Player::Guest)
                } else {
                    // Wrong passcode.
                    None
                }
            } else {
                self.passcodes[Player::Guest] = Some(passcode);
                Some(Player::Guest)
            }
        } else {
            self.passcodes[Player::Host] = Some(passcode);
            Some(Player::Host)
        }
    }

    fn play(&mut self, player: Player, msg: ClientMessage) {
        use ClientMessage as Msg;

        enum Action {
            Move(Move),
            Retract,
            Reset(GameOptions),
        }

        let stone = self.options.stone_of(player);

        let action = match msg {
            Msg::Start(..) | Msg::Join(_) | Msg::Authenticate(_) => return,
            Msg::Place(p1, p2) => {
                if self.record.turn() != Some(stone) {
                    // Not their turn.
                    return;
                }
                Action::Move(Move::Place(p1, p2))
            }
            Msg::Pass => {
                if self.record.turn() != Some(stone) {
                    // Not their turn.
                    return;
                }
                Action::Move(Move::Pass)
            }
            Msg::ClaimWin(p, dir) => Action::Move(Move::Win(p, dir)),
            Msg::Resign => Action::Move(Move::Resign(stone)),
            Msg::Request(req) => 'a: {
                match req {
                    Request::Accept => {
                        let Some(req) = self.requests[player.opposite()] else {
                            // The opponent hasn't made a request.
                            return;
                        };

                        break 'a match req {
                            Request::Accept | Request::Decline => unreachable!(),
                            Request::Draw => Action::Move(Move::Draw),
                            Request::Retract => Action::Retract,
                            Request::Reset(options) => Action::Reset(options),
                        };
                    }
                    Request::Decline => {
                        if self.requests[player.opposite()].take().is_some() {
                            // Inform the opponent of the decline.
                            _ = self.msg_tx.send(ServerMessage::Request(player, req));
                        }
                        return;
                    }
                    _ => {}
                }

                let player_req = &mut self.requests[player];
                if player_req.is_some() {
                    // Duplicate request.
                    return;
                }

                if req == Request::Retract && !self.record.has_past() {
                    // No moves in the past.
                    return;
                }

                *player_req = Some(req);
                _ = self.msg_tx.send(ServerMessage::Request(player, req));
                return;
            }
        };

        match action {
            Action::Move(mov) => {
                if !self.record.make_move(mov) {
                    // The move failed.
                    return;
                }
                _ = self.msg_tx.send(ServerMessage::Move(mov));
            }
            Action::Retract => {
                // We have checked that there is a previous move.
                self.record.undo_move();
                _ = self.msg_tx.send(ServerMessage::Retract);
            }
            Action::Reset(options) => {
                self.options = options;
                self.record.jump(0);

                _ = self.msg_tx.send(ServerMessage::Options(options));
                _ = self.msg_tx.send(ServerMessage::Record(Default::default()));
            }
        }

        // Clear the requests.
        self.requests.fill(None);

        if let ClientMessage::Request(_) = msg {
            // Inform the opponent of the acceptance.
            // This has to be the last message in order for the dialog not to be closed.
            _ = self
                .msg_tx
                .send(ServerMessage::Request(player, Request::Accept));
        }
    }
}

async fn start_game(id: GameId, options: GameOptions, mut cmd_rx: mpsc::Receiver<GameCommand>) {
    tracing::debug!("game started: {}", id.escape_ascii());

    let mut state = GameState::new();
    state.options = options;

    while let Some(cmd) = cmd_rx.recv().await {
        match cmd {
            GameCommand::Subscribe(resp_tx) => {
                _ = resp_tx.send(state.subscribe());
            }
            GameCommand::Authenticate(resp_tx, pass) => {
                _ = resp_tx.send(state.authenticate(pass));
            }
            GameCommand::Play(stone, msg) => state.play(stone, msg),
        }
    }

    // All command senders are dropped.
    tracing::debug!("game ended: {}", id.escape_ascii());
}
