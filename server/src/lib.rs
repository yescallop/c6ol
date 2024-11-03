//! The server library for [Connect6 Online](https://github.com/yescallop/c6ol).

mod manager;
mod server;
mod shutdown;
mod ws;

pub use server::run;
