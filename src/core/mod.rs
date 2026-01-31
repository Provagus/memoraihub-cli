//! Core module - Business logic
//!
//! Contains the core data structures and logic for meh.

pub mod fact;
pub mod kb;
pub mod path;
pub mod storage;
pub mod search;
pub mod trust;
pub mod multi_storage;
pub mod notifications;
pub mod pending_queue;

pub use kb::{KnowledgeBase, KnowledgeBaseBackend, LocalKb, RemoteKb, KbStats};
pub use pending_queue::{PendingQueue, PendingWrite, PendingWriteType};
