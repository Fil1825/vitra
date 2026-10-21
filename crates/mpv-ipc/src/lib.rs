//! Async client for [mpv's JSON IPC](https://mpv.io/manual/stable/#json-ipc).
//!
//! mpv runs as a separate process; this crate starts it with an IPC connection (a socket pair
//! on Unix, a named pipe on Windows), sends commands with request ids and delivers the events
//! mpv pushes (property changes, file start/end, ...). It knows nothing about Tauri, so it can be
//! tested against an in-memory fake (see the tests) and, opt-in, against a real mpv.

mod client;
mod error;
mod event;
mod process;

pub use client::{Client, Events};
pub use error::Error;
pub use event::{EndReason, Event};
pub use process::{MpvProcess, SpawnOptions};
