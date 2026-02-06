//! CLI utility functions
//!
//! Common helper functions shared across CLI commands.
//! This module reduces code duplication by centralizing:
//! - Repository discovery (find_meh_dir)
//! - Fact lookup by ID or path (find_fact)

use anyhow::{bail, Result};
use std::path::PathBuf;
use ulid::Ulid;

use crate::core::fact::Fact;
use crate::core::storage::Storage;

/// Find .meh directory by walking up from current directory
///
/// Traverses parent directories until a `.meh` folder is found,
/// similar to how git finds `.git`.
///
/// # Errors
/// Returns an error if no `.meh` directory is found in any parent.
pub fn find_meh_dir() -> Result<PathBuf> {
    let mut current = std::env::current_dir()?;

    loop {
        let meh_dir = current.join(".meh");
        if meh_dir.exists() {
            return Ok(meh_dir);
        }

        if !current.pop() {
            bail!("Not a meh repository (or any parent directory). Run 'meh init' first.");
        }
    }
}

/// Get the database path from the .meh directory
///
/// Convenience function that finds .meh and returns the data.db path.
pub fn get_db_path() -> Result<PathBuf> {
    Ok(find_meh_dir()?.join("data.db"))
}

/// Open storage from the current repository
///
/// Convenience function that finds .meh, opens storage, and returns it.
pub fn open_storage() -> Result<Storage> {
    Storage::open(&get_db_path()?)
}

/// Find a fact by ID or path
///
/// Accepts either:
/// - `meh-01HQ3K2JN5` - a meh ID (ULID format)
/// - `@products/alpha/api` - a path
///
/// # Errors
/// Returns an error if:
/// - The ID format is invalid
/// - No fact is found with the given ID or path
pub fn find_fact(storage: &Storage, target: &str) -> Result<Fact> {
    if target.starts_with("meh-") {
        // ID lookup
        let id_str = target.trim_start_matches("meh-");
        let id =
            Ulid::from_string(id_str).map_err(|_| anyhow::anyhow!("Invalid meh ID: {}", target))?;
        storage
            .get_by_id(&id)?
            .ok_or_else(|| anyhow::anyhow!("Fact not found: {}", target))
    } else {
        // Path lookup - return first (latest) match
        let facts = storage.get_by_path(target)?;
        facts
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("Fact not found: {}", target))
    }
}
