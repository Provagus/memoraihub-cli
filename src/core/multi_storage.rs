//! Multi-Storage - Federated storage across multiple databases
//!
//! Allows reading from multiple sources (local, cache, remote)
//! while writing only to local.

use std::path::Path;

use anyhow::{Context, Result};

use super::fact::Fact;
use super::storage::{PathInfo, Storage};

/// Storage source identifier
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StorageSource {
    /// Local writable storage
    Local,
    /// Cached remote data (read-only)
    Cache(String),
}

/// A storage instance with its source
struct SourcedStorage {
    #[allow(dead_code)] // Will be used for write restriction checks
    source: StorageSource,
    storage: Storage,
    #[allow(dead_code)] // Will be used for write restriction checks
    readonly: bool,
}

/// Multi-storage manager
///
/// Reads from multiple databases, writes to local only.
pub struct MultiStorage {
    /// Local storage (always first, writable)
    local: Storage,
    /// Cached/remote storages (read-only)
    caches: Vec<SourcedStorage>,
}

impl MultiStorage {
    /// Create with just local storage
    pub fn new(local_path: &Path) -> Result<Self> {
        let local = Storage::open(local_path).context("Failed to open local storage")?;

        Ok(Self {
            local,
            caches: Vec::new(),
        })
    }

    /// Add a cache storage (read-only)
    pub fn add_cache(&mut self, name: impl Into<String>, path: &Path) -> Result<()> {
        let storage = Storage::open(path).context("Failed to open cache storage")?;

        self.caches.push(SourcedStorage {
            source: StorageSource::Cache(name.into()),
            storage,
            readonly: true,
        });

        Ok(())
    }

    /// Load all caches from a directory
    pub fn load_caches_from_dir(&mut self, cache_dir: &Path) -> Result<usize> {
        if !cache_dir.exists() {
            return Ok(0);
        }

        let mut count = 0;
        for entry in std::fs::read_dir(cache_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.extension().map_or(false, |ext| ext == "db") {
                let name = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("unknown")
                    .to_string();

                if let Ok(()) = self.add_cache(&name, &path) {
                    count += 1;
                }
            }
        }

        Ok(count)
    }

    /// Get reference to local storage (for writes)
    pub fn local(&self) -> &Storage {
        &self.local
    }

    /// Get mutable reference to local storage (for writes)
    pub fn local_mut(&mut self) -> &mut Storage {
        &mut self.local
    }

    /// Insert a fact (always to local)
    pub fn insert(&self, fact: &Fact) -> Result<()> {
        self.local.insert(fact)
    }

    /// Get by ID - search local first, then caches
    pub fn get_by_id(&self, id: &ulid::Ulid) -> Result<Option<Fact>> {
        // Try local first
        if let Some(fact) = self.local.get_by_id(id)? {
            return Ok(Some(fact));
        }

        // Try caches
        for cache in &self.caches {
            if let Some(fact) = cache.storage.get_by_id(id)? {
                return Ok(Some(fact));
            }
        }

        Ok(None)
    }

    /// Get by path - merge results from all sources
    pub fn get_by_path(&self, path: &str) -> Result<Vec<Fact>> {
        let mut results = self.local.get_by_path(path)?;

        for cache in &self.caches {
            let cache_results = cache.storage.get_by_path(path)?;
            results.extend(cache_results);
        }

        // Sort by created_at desc, dedupe by id
        results.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        results.dedup_by(|a, b| a.id == b.id);

        Ok(results)
    }

    /// Search across all storages
    pub fn search(&self, query: &str, limit: i64) -> Result<Vec<Fact>> {
        let mut results = self.local.search(query, limit)?;

        for cache in &self.caches {
            let cache_results = cache.storage.search(query, limit)?;
            results.extend(cache_results);
        }

        // Sort by relevance (trust_score as proxy), dedupe
        results.sort_by(|a, b| b.trust_score.partial_cmp(&a.trust_score).unwrap());
        results.dedup_by(|a, b| a.id == b.id);
        results.truncate(limit as usize);

        Ok(results)
    }

    /// List children - merge from all sources
    pub fn list_children(
        &self,
        parent: &str,
        limit: i64,
        cursor: Option<&str>,
    ) -> Result<(Vec<PathInfo>, bool)> {
        // For now, just use local - merging path counts is complex
        // TODO: Merge path counts from caches
        self.local.list_children(parent, limit, cursor)
    }

    /// Get stats from all storages
    pub fn stats(&self) -> Result<MultiStorageStats> {
        let local_stats = self.local.stats()?;

        let mut cache_stats = Vec::new();
        for cache in &self.caches {
            let name = match &cache.source {
                StorageSource::Local => "local".to_string(),
                StorageSource::Cache(n) => n.clone(),
            };
            let stats = cache.storage.stats()?;
            cache_stats.push((name, stats));
        }

        Ok(MultiStorageStats {
            local: local_stats,
            caches: cache_stats,
        })
    }

    /// Mark as superseded (local only)
    pub fn mark_superseded(&self, id: &ulid::Ulid) -> Result<()> {
        self.local.mark_superseded(id)
    }

    /// Mark as deprecated (local only)
    pub fn mark_deprecated(&self, id: &ulid::Ulid) -> Result<()> {
        self.local.mark_deprecated(id)
    }
}

/// Stats for multi-storage
pub struct MultiStorageStats {
    pub local: super::storage::StorageStats,
    pub caches: Vec<(String, super::storage::StorageStats)>,
}

impl MultiStorageStats {
    pub fn total_facts(&self) -> i64 {
        let mut total = self.local.total;
        for (_, stats) in &self.caches {
            total += stats.total;
        }
        total
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_multi_storage_local_only() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("local.db");

        let storage = MultiStorage::new(&db_path).unwrap();

        let fact = Fact::new("@test/path", "Test", "Content");
        storage.insert(&fact).unwrap();

        let found = storage.get_by_id(&fact.id).unwrap();
        assert!(found.is_some());
    }

    #[test]
    fn test_multi_storage_with_cache() {
        let dir = tempdir().unwrap();
        let local_path = dir.path().join("local.db");
        let cache_path = dir.path().join("cache.db");

        // Create cache with a fact
        {
            let cache = Storage::open(&cache_path).unwrap();
            let fact = Fact::new("@cached/path", "Cached", "From cache");
            cache.insert(&fact).unwrap();
        }

        // Create multi-storage
        let mut storage = MultiStorage::new(&local_path).unwrap();
        storage.add_cache("test-cache", &cache_path).unwrap();

        // Should find cached fact
        let results = storage.search("cached", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Cached");
    }
}
