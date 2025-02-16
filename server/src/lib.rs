//! The server library for [Connect6 Online](https://github.com/yescallop/c6ol).

mod argon2id;
mod db;
mod game;
mod macros;
mod server;
mod shutdown;
mod ws;

pub use server::run;
