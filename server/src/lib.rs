//! The server for a web app for playing [Connect6] games online.
//!
//! [Connect6]: https://en.wikipedia.org/wiki/Connect6

#![warn(
    future_incompatible,
    missing_docs,
    nonstandard_style,
    rust_2018_idioms,
    clippy::use_self
)]
#![forbid(unsafe_code)]
#![feature(isqrt)]

pub mod game;
pub mod protocol;
