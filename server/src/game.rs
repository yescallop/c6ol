//! Game manager.

use crate::{argon2id, db::DbManager, macros::exec};
use c6ol_core::{
    game::{Move, Player, PlayerSlots, Record},
    protocol::{
        ClientMessage, GameId, GameOptions, Passcode, PasscodeHash, Request, ServerMessage,
    },
};
use std::collections::HashMap;
use tokio::{
    sync::{broadcast, mpsc, oneshot},
    task::{self, JoinSet},
};

const CHANNEL_CAPACITY_MANAGE_CMD: usize = 64;
const CHANNEL_CAPACITY_GAME_CMD: usize = 8;
const CHANNEL_CAPACITY_GAME_MSG: usize = 8;

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
        exec!(self.cmd_tx, GameCommand::Subscribe,)
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
        self.player = exec!(self.cmd_tx, GameCommand::Authenticate, passcode);
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
        exec!(self.cmd_tx, GameCommand::Play(player, msg));
    }
}

enum GameManageCommand {
    Create(oneshot::Sender<Game>, GameOptions),
    Find(oneshot::Sender<Option<Game>>, GameId),
}

/// Creates a game manager.
///
/// Returns a command handle to it and a future to run it.
pub fn manager(db_manager: DbManager) -> (GameManager, impl Future<Output = ()>) {
    let (cmd_tx, cmd_rx) = mpsc::channel(CHANNEL_CAPACITY_MANAGE_CMD);
    (GameManager { cmd_tx }, manage_games(db_manager, cmd_rx))
}

/// A command handle to a game manager.
#[derive(Clone)]
pub struct GameManager {
    cmd_tx: mpsc::Sender<GameManageCommand>,
}

impl GameManager {
    /// Creates a new game with the given options.
    pub async fn create(&self, options: GameOptions) -> Game {
        exec!(self.cmd_tx, GameManageCommand::Create, options)
    }

    /// Searches for a game with the given ID.
    pub async fn find(&self, id: GameId) -> Option<Game> {
        exec!(self.cmd_tx, GameManageCommand::Find, id)
    }
}

async fn manage_games(db_manager: DbManager, mut cmd_rx: mpsc::Receiver<GameManageCommand>) {
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
                    GameManageCommand::Create(resp_tx, options) => {
                        let (id, state) = db_manager.create(options).await;

                        let (game_cmd_tx, game_cmd_rx) = mpsc::channel(CHANNEL_CAPACITY_GAME_CMD);
                        game_cmd_txs.insert(id, game_cmd_tx.downgrade());

                        let task_id = game_tasks.spawn(manage_game(id, state, game_cmd_rx)).id();
                        game_ids_by_task_id.insert(task_id, id);

                        _ = resp_tx.send(Game::new(id, game_cmd_tx));

                        tracing::info!("game started: {id}");
                    },
                    GameManageCommand::Find(resp_tx, id) => {
                        if let Some(tx) = game_cmd_txs.get(&id) {
                            // There is a chance that all senders have been dropped
                            // but the game task has not finished yet.
                            if let Some(tx) = tx.upgrade() {
                                _ = resp_tx.send(Some(Game::new(id, tx)));
                                continue;
                            }
                        }

                        if let Some(state) = db_manager.load(id).await {
                            let (game_cmd_tx, game_cmd_rx) = mpsc::channel(CHANNEL_CAPACITY_GAME_CMD);
                            game_cmd_txs.insert(id, game_cmd_tx.downgrade());

                            let task_id = game_tasks.spawn(manage_game(id, state, game_cmd_rx)).id();
                            game_ids_by_task_id.insert(task_id, id);

                            _ = resp_tx.send(Some(Game::new(id, game_cmd_tx)));

                            tracing::info!("game loaded: {id}");
                        } else {
                            _ = resp_tx.send(None);
                        }
                    }
                }
            }
            // When `join_next` returns `None`, `select!` will disable
            // this branch and still wait on the other branch.
            Some(res) = game_tasks.join_next_with_id() => {
                let (task_id, state) = match res {
                    Ok((id, state)) => (id, Some(state)),
                    Err(err) => {
                        tracing::error!("game task panicked: {err}");
                        (err.id(), None)
                    },
                };

                let id = game_ids_by_task_id.remove(&task_id).unwrap();
                if let Some(tx) = game_cmd_txs.get(&id) {
                    // There is a chance that the same game is loaded again
                    // before we even remove the weak sender.
                    if tx.strong_count() == 0 {
                        game_cmd_txs.remove(&id);
                    }
                }

                if let Some(state) = state {
                    db_manager.save(id, state).await;
                    tracing::info!("game saved: {id}");
                }
            }
        }
    }

    // Wait for all game tasks to finish.
    while game_tasks.join_next().await.is_some() {}

    tracing::info!("game manager stopped");
}

#[derive(Default)]
pub struct GameState {
    pub options: GameOptions,
    pub passcode_hashes: PlayerSlots<Option<PasscodeHash>>,
    pub requests: PlayerSlots<Option<Request>>,
    pub record: Record,
}

impl GameState {
    fn subscribe(&self, msg_tx: &broadcast::Sender<ServerMessage>) -> GameSubscription {
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
            msg_rx: msg_tx.subscribe(),
        }
    }

    fn authenticate(&mut self, hash: PasscodeHash) -> Option<Player> {
        if let Some(hash_host) = self.passcode_hashes[Player::Host] {
            if hash == hash_host {
                Some(Player::Host)
            } else if let Some(hash_guest) = self.passcode_hashes[Player::Guest] {
                if hash == hash_guest {
                    Some(Player::Guest)
                } else {
                    // Wrong passcode.
                    None
                }
            } else {
                self.passcode_hashes[Player::Guest] = Some(hash);
                Some(Player::Guest)
            }
        } else {
            self.passcode_hashes[Player::Host] = Some(hash);
            Some(Player::Host)
        }
    }

    fn play(
        &mut self,
        player: Player,
        msg: ClientMessage,
        msg_tx: &broadcast::Sender<ServerMessage>,
    ) {
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
            Msg::Request(req) => {
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
                _ = msg_tx.send(ServerMessage::Request(player, req));
                return;
            }
            Msg::AcceptRequest => {
                let Some(req) = self.requests[player.opposite()] else {
                    // The opponent hasn't made a request.
                    return;
                };

                match req {
                    Request::Draw => Action::Move(Move::Draw),
                    Request::Retract => Action::Retract,
                    Request::Reset(options) => Action::Reset(options),
                }
            }
            Msg::DeclineRequest => {
                if self.requests[player.opposite()].take().is_some() {
                    // Inform the opponent of the decline.
                    _ = msg_tx.send(ServerMessage::DeclineRequest(player));
                }
                return;
            }
        };

        match action {
            Action::Move(mov) => {
                if !self.record.make_move(mov) {
                    // The move failed.
                    return;
                }
                _ = msg_tx.send(ServerMessage::Move(mov));
            }
            Action::Retract => {
                // We have checked that there is a previous move.
                self.record.undo_move();
                _ = msg_tx.send(ServerMessage::Retract);
            }
            Action::Reset(options) => {
                self.options = options;
                self.record.jump(0);

                _ = msg_tx.send(ServerMessage::Options(options));
                _ = msg_tx.send(ServerMessage::Record(Default::default()));
            }
        }

        // Clear the requests.
        self.requests.fill(None);

        if let ClientMessage::AcceptRequest = msg {
            // Inform the opponent of the acceptance.
            // This has to be the last message in order for the dialog not to be closed.
            _ = msg_tx.send(ServerMessage::AcceptRequest(player));
        }
    }
}

async fn manage_game(
    id: GameId,
    mut state: Box<GameState>,
    mut cmd_rx: mpsc::Receiver<GameCommand>,
) -> Box<GameState> {
    let (msg_tx, _) = broadcast::channel(CHANNEL_CAPACITY_GAME_MSG);

    while let Some(cmd) = cmd_rx.recv().await {
        match cmd {
            GameCommand::Subscribe(resp_tx) => {
                _ = resp_tx.send(state.subscribe(&msg_tx));
            }
            GameCommand::Authenticate(resp_tx, passcode) => {
                let hash = task::spawn_blocking(move || argon2id::hash(&passcode, id.0))
                    .await
                    .expect("hashing should not panic");
                _ = resp_tx.send(hash.and_then(|hash| state.authenticate(hash)));
            }
            GameCommand::Play(player, msg) => state.play(player, msg, &msg_tx),
        }
    }

    // All command senders are dropped.
    state
}
