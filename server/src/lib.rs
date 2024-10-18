//! The server for a web app for playing [Connect6] games online.
//!
//! [Connect6]: https://en.wikipedia.org/wiki/Connect6

#![warn(
    future_incompatible,
    missing_docs,
    nonstandard_style,
    rust_2018_idioms,
    clippy::checked_conversions,
    clippy::if_not_else,
    clippy::ignored_unit_patterns,
    clippy::map_unwrap_or,
    clippy::missing_errors_doc,
    // clippy::must_use_candidate,
    // clippy::redundant_closure_for_method_calls,
    clippy::redundant_else,
    clippy::semicolon_if_nothing_returned,
    clippy::single_match_else,
    clippy::use_self,
)]
#![forbid(unsafe_code)]

pub mod game;
pub mod manager;
pub mod protocol;
mod server;
mod shutdown;
mod ws;

pub use server::run;
