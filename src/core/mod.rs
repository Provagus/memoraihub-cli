//! Core module - Business logic
//!
//! Contains the core data structures and logic for meh.

pub mod fact;
pub mod kb;
pub mod multi_storage;
pub mod notifications;
pub mod path;
pub mod pending_queue;
pub mod search;
pub mod storage;
pub mod trust;

pub use kb::{KbStats, KnowledgeBase, KnowledgeBaseBackend, LocalKb, RemoteKb};
pub use pending_queue::{PendingQueue, PendingWrite, PendingWriteType};
