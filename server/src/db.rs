use crate::{game::GameState, macros::exec};
use anyhow::Context;
use c6ol_core::{
    game::{Player, Record, RecordEncodeMethod},
    protocol::{GameId, GameOptions, Request},
};
use chrono::Utc;
use rand::{distr::Alphanumeric, Rng};
use rusqlite::{Connection, Row};
use std::{array, path::PathBuf};
use tokio::{
    sync::{mpsc, oneshot},
    task,
};

const CHANNEL_CAPACITY: usize = 64;

enum Command {
    Create(oneshot::Sender<(GameId, Box<GameState>)>, GameOptions),
    Load(oneshot::Sender<Option<Box<GameState>>>, GameId),
    Save(GameId, Box<GameState>),
}

pub struct DbManager {
    cmd_tx: mpsc::Sender<Command>,
}

impl DbManager {
    pub async fn create(&self, options: GameOptions) -> (GameId, Box<GameState>) {
        exec!(self.cmd_tx, Command::Create, options)
    }

    pub async fn load(&self, id: GameId) -> Option<Box<GameState>> {
        exec!(self.cmd_tx, Command::Load, id)
    }

    pub async fn save(&self, id: GameId, state: Box<GameState>) {
        exec!(self.cmd_tx, Command::Save(id, state));
    }
}

pub fn manager(path: Option<PathBuf>) -> (DbManager, task::JoinHandle<()>) {
    let (cmd_tx, cmd_rx) = mpsc::channel(CHANNEL_CAPACITY);
    let handle = task::spawn_blocking(move || {
        tracing::info!("database manager started");
        match manage_db(path, cmd_rx) {
            Ok(()) => tracing::info!("database manager stopped"),
            Err(err) => tracing::error!("database manager returned error: {err}"),
        }
    });
    (DbManager { cmd_tx }, handle)
}

/// Generates a random alphanumeric game ID.
fn rand_game_id() -> GameId {
    let mut rng = rand::rng();
    array::from_fn(|_| rng.sample(Alphanumeric))
}

fn manage_db(path: Option<PathBuf>, mut cmd_rx: mpsc::Receiver<Command>) -> anyhow::Result<()> {
    let conn = match path {
        Some(path) => Connection::open(path)?,
        None => Connection::open_in_memory()?,
    };

    conn.execute(
        "CREATE TABLE IF NOT EXISTS game (
            id BLOB NOT NULL PRIMARY KEY,
            options BLOB NOT NULL,
            passcode_host BLOB,
            passcode_guest BLOB,
            request_host BLOB,
            request_guest BLOB,
            record BLOB NOT NULL,
            created_at INT NOT NULL,
            updated_at INT NOT NULL
        ) STRICT",
        (),
    )?;

    while let Some(cmd) = cmd_rx.blocking_recv() {
        match cmd {
            Command::Create(resp_tx, options) => {
                let state = Box::new(GameState {
                    options,
                    ..Default::default()
                });

                let options = state.options.encode_to_vec();
                let record = state.record.encode_to_vec(RecordEncodeMethod::Past);
                let timestamp = Utc::now().timestamp_millis();

                let mut stmt = conn.prepare(
                    "INSERT OR IGNORE INTO game
                        (id, options, record, created_at, updated_at)
                        VALUES (?1, ?2, ?3, ?4, ?4)",
                )?;
                let id = loop {
                    let id = rand_game_id();
                    let rows = stmt.execute((id, &options, &record, timestamp))?;
                    if rows > 0 {
                        break id;
                    }
                };
                _ = resp_tx.send((id, state));
            }
            Command::Load(resp_tx, id) => {
                let mut stmt = conn.prepare(
                    "SELECT options, passcode_host, passcode_guest,
                        request_host, request_guest, record FROM game WHERE id = ?1",
                )?;
                let resp = stmt.query([id])?.next()?.map(parse_row).transpose()?;
                _ = resp_tx.send(resp);
            }
            Command::Save(id, state) => {
                conn.execute(
                    "UPDATE game SET options = ?1,
                        passcode_host = ?2, passcode_guest = ?3,
                        request_host = ?4, request_guest = ?5,
                        record = ?6, updated_at = ?7 WHERE id = ?8",
                    (
                        state.options.encode_to_vec(),
                        &state.passcodes[Player::Host],
                        &state.passcodes[Player::Guest],
                        state.requests[Player::Host].map(Request::encode_to_vec),
                        state.requests[Player::Guest].map(Request::encode_to_vec),
                        state.record.encode_to_vec(RecordEncodeMethod::Past),
                        Utc::now().timestamp_millis(),
                        id,
                    ),
                )?;
            }
        }
    }

    Ok(())
}

fn parse_row(row: &Row<'_>) -> anyhow::Result<Box<GameState>> {
    let mut state = Box::new(GameState::default());

    state.options = GameOptions::decode(&mut row.get_ref("options")?.as_blob()?)
        .context("failed to decode options")?;

    for (player, pass_idx, req_idx) in [
        (Player::Host, "passcode_host", "request_host"),
        (Player::Guest, "passcode_guest", "request_guest"),
    ] {
        state.passcodes[player] = row.get(pass_idx)?;
        state.requests[player] = row
            .get_ref(req_idx)?
            .as_blob_or_null()?
            .map(|ref mut buf| Request::decode(buf).context("failed to decode request"))
            .transpose()?;
    }

    state.record = Record::decode(&mut row.get_ref("record")?.as_blob()?)
        .context("failed to decode record")?;

    Ok(state)
}
