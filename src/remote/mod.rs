//! Remote server client module
//!
//! Provides HTTP client for interacting with memoraihub-server.

mod client;
mod types;

pub use client::RemoteClient;
pub use types::*;
