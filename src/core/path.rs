//! Path - Knowledge organization paths
//!
//! Paths are hierarchical locations for facts, similar to filesystem paths.
//!
//! # Examples
//! - `@products/alpha/api/timeout`
//! - `@users/kasia/preferences/coffee`
//! - `@repos/backend/conventions`
//!
//! # Architecture
//! See `../../plan/ANALYSIS_KNOWLEDGE_ORGANIZATION.md`
//!
//! # Key Points
//! - Path depth is UNLIMITED (not capped at 4)
//! - Paths can use wildcards in queries: `@products/*/api/timeout`
//! - Reserved prefixes are optional: `@products/`, `@users/`, etc.

use std::fmt;

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

/// A knowledge path
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Path {
    /// Path segments (e.g., ["@products", "alpha", "api", "timeout"])
    segments: Vec<String>,
    
    /// Original string representation
    raw: String,
}

impl Path {
    /// Parse a path string
    ///
    /// # Examples
    /// ```
    /// use meh::core::path::Path;
    ///
    /// let path = Path::parse("@products/alpha/api/timeout").unwrap();
    /// assert_eq!(path.segments().len(), 4);
    /// ```
    pub fn parse(s: &str) -> Result<Self> {
        let s = s.trim();
        
        if s.is_empty() {
            bail!("Path cannot be empty");
        }

        // Normalize: remove trailing slash, handle leading slash
        let normalized = s.trim_start_matches('/').trim_end_matches('/');
        
        if normalized.is_empty() {
            // Root path
            return Ok(Self {
                segments: vec![],
                raw: "/".to_string(),
            });
        }

        // Split into segments
        let segments: Vec<String> = normalized
            .split('/')
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect();

        // Validate segments
        for segment in &segments {
            Self::validate_segment(segment)?;
        }

        Ok(Self {
            raw: normalized.to_string(),
            segments,
        })
    }

    /// Validate a path segment
    fn validate_segment(segment: &str) -> Result<()> {
        if segment.is_empty() {
            bail!("Path segment cannot be empty");
        }

        // Allow: alphanumeric, dash, underscore, @ (for reserved prefixes)
        let valid = segment.chars().all(|c| {
            c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '@'
        });

        if !valid {
            bail!("Invalid characters in path segment: {}", segment);
        }

        // Segment starting with @ must be at root level
        // (This is handled by convention, not enforced here)

        Ok(())
    }

    /// Get path segments
    pub fn segments(&self) -> &[String] {
        &self.segments
    }

    /// Get path depth (number of segments)
    pub fn depth(&self) -> usize {
        self.segments.len()
    }

    /// Check if this is the root path
    pub fn is_root(&self) -> bool {
        self.segments.is_empty()
    }

    /// Get parent path
    pub fn parent(&self) -> Option<Path> {
        if self.segments.is_empty() {
            return None;
        }

        let parent_segments = self.segments[..self.segments.len() - 1].to_vec();
        let raw = if parent_segments.is_empty() {
            "/".to_string()
        } else {
            parent_segments.join("/")
        };

        Some(Path {
            segments: parent_segments,
            raw,
        })
    }

    /// Get the last segment (leaf name)
    pub fn name(&self) -> Option<&str> {
        self.segments.last().map(|s| s.as_str())
    }

    /// Check if this path starts with another path (is a descendant)
    pub fn starts_with(&self, prefix: &Path) -> bool {
        if prefix.segments.len() > self.segments.len() {
            return false;
        }

        self.segments
            .iter()
            .zip(prefix.segments.iter())
            .all(|(a, b)| a == b)
    }

    /// Join with another path or segment
    pub fn join(&self, other: &str) -> Result<Path> {
        let mut new_segments = self.segments.clone();
        
        for segment in other.split('/').filter(|s| !s.is_empty()) {
            Self::validate_segment(segment)?;
            new_segments.push(segment.to_string());
        }

        let raw = new_segments.join("/");
        Ok(Path {
            segments: new_segments,
            raw,
        })
    }

    /// Check if path matches a pattern with wildcards
    ///
    /// Patterns:
    /// - `*` matches any single segment
    /// - `**` matches any number of segments
    ///
    /// # Examples
    /// - `@products/*/api/timeout` matches `@products/alpha/api/timeout`
    /// - `@products/**/timeout` matches `@products/alpha/api/v2/timeout`
    pub fn matches_pattern(&self, pattern: &str) -> bool {
        let pattern_segments: Vec<&str> = pattern
            .trim_start_matches('/')
            .trim_end_matches('/')
            .split('/')
            .filter(|s| !s.is_empty())
            .collect();

        Self::match_segments(&self.segments, &pattern_segments)
    }

    fn match_segments(path: &[String], pattern: &[&str]) -> bool {
        match (path.first(), pattern.first()) {
            // Both empty = match
            (None, None) => true,
            
            // Pattern empty but path has more = no match
            (Some(_), None) => false,
            
            // Path empty but pattern has more
            (None, Some(&"**")) => Self::match_segments(path, &pattern[1..]),
            (None, Some(_)) => false,
            
            // ** matches zero or more segments
            (Some(_), Some(&"**")) => {
                // Try matching ** with zero segments
                if Self::match_segments(path, &pattern[1..]) {
                    return true;
                }
                // Try matching ** with one segment and continue
                Self::match_segments(&path[1..], pattern)
            }
            
            // * matches exactly one segment
            (Some(_), Some(&"*")) => {
                Self::match_segments(&path[1..], &pattern[1..])
            }
            
            // Literal match
            (Some(p), Some(pat)) => {
                if p == *pat {
                    Self::match_segments(&path[1..], &pattern[1..])
                } else {
                    false
                }
            }
        }
    }

    /// Check if this is a reserved path prefix
    pub fn is_reserved_prefix(&self) -> bool {
        matches!(
            self.segments.first().map(|s| s.as_str()),
            Some("@products" | "@users" | "@repos" | "@teams" | "@topics" | "@meta")
        )
    }
}

impl fmt::Display for Path {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.segments.is_empty() {
            write!(f, "/")
        } else {
            write!(f, "{}", self.raw)
        }
    }
}

impl TryFrom<&str> for Path {
    type Error = anyhow::Error;

    fn try_from(s: &str) -> Result<Self> {
        Path::parse(s)
    }
}

impl TryFrom<String> for Path {
    type Error = anyhow::Error;

    fn try_from(s: String) -> Result<Self> {
        Path::parse(&s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple() {
        let path = Path::parse("@products/alpha/api/timeout").unwrap();
        assert_eq!(path.segments().len(), 4);
        assert_eq!(path.segments()[0], "@products");
        assert_eq!(path.segments()[3], "timeout");
    }

    #[test]
    fn test_parse_with_slashes() {
        let path1 = Path::parse("/products/alpha/").unwrap();
        let path2 = Path::parse("products/alpha").unwrap();
        assert_eq!(path1.segments(), path2.segments());
    }

    #[test]
    fn test_root() {
        let path = Path::parse("/").unwrap();
        assert!(path.is_root());
        assert_eq!(path.depth(), 0);
    }

    #[test]
    fn test_parent() {
        let path = Path::parse("@products/alpha/api").unwrap();
        let parent = path.parent().unwrap();
        assert_eq!(parent.to_string(), "@products/alpha");
    }

    #[test]
    fn test_starts_with() {
        let path = Path::parse("@products/alpha/api/timeout").unwrap();
        let prefix = Path::parse("@products/alpha").unwrap();
        assert!(path.starts_with(&prefix));
    }

    #[test]
    fn test_join() {
        let path = Path::parse("@products/alpha").unwrap();
        let joined = path.join("api/timeout").unwrap();
        assert_eq!(joined.to_string(), "@products/alpha/api/timeout");
    }

    #[test]
    fn test_wildcard_single() {
        let path = Path::parse("@products/alpha/api/timeout").unwrap();
        assert!(path.matches_pattern("@products/*/api/timeout"));
        assert!(!path.matches_pattern("@products/*/database/timeout"));
    }

    #[test]
    fn test_wildcard_double() {
        let path = Path::parse("@products/alpha/api/v2/timeout").unwrap();
        assert!(path.matches_pattern("@products/**/timeout"));
        assert!(path.matches_pattern("@products/alpha/**/timeout"));
    }

    #[test]
    fn test_deep_path() {
        // Path depth is unlimited
        let path = Path::parse(
            "@products/alpha/api/v2/commands/users/create/validation/email/format"
        ).unwrap();
        assert_eq!(path.depth(), 10);
    }

    // === Additional tests ===

    #[test]
    fn test_empty_path_error() {
        assert!(Path::parse("").is_err());
        assert!(Path::parse("   ").is_err());
    }

    #[test]
    fn test_invalid_characters() {
        // Spaces not allowed
        assert!(Path::parse("@products/my path").is_err());
        // Special chars not allowed
        assert!(Path::parse("@products/path!name").is_err());
        assert!(Path::parse("@products/path#name").is_err());
    }

    #[test]
    fn test_valid_characters() {
        // Underscore and dash are allowed
        assert!(Path::parse("@my_product/api-v2").is_ok());
        // @ is allowed (for reserved prefixes)
        assert!(Path::parse("@meh/todo").is_ok());
    }

    #[test]
    fn test_name() {
        let path = Path::parse("@products/alpha/timeout").unwrap();
        assert_eq!(path.name(), Some("timeout"));
        
        let root = Path::parse("/").unwrap();
        assert_eq!(root.name(), None);
    }

    #[test]
    fn test_parent_chain() {
        let path = Path::parse("@a/b/c/d").unwrap();
        let p1 = path.parent().unwrap();
        assert_eq!(p1.to_string(), "@a/b/c");
        let p2 = p1.parent().unwrap();
        assert_eq!(p2.to_string(), "@a/b");
        let p3 = p2.parent().unwrap();
        assert_eq!(p3.to_string(), "@a");
        let p4 = p3.parent().unwrap();
        assert!(p4.is_root());
        assert!(p4.parent().is_none());
    }

    #[test]
    fn test_starts_with_self() {
        let path = Path::parse("@products/alpha").unwrap();
        assert!(path.starts_with(&path));
    }

    #[test]
    fn test_starts_with_longer_prefix() {
        let path = Path::parse("@products").unwrap();
        let longer = Path::parse("@products/alpha/beta").unwrap();
        assert!(!path.starts_with(&longer));
    }

    #[test]
    fn test_wildcard_at_start() {
        let path = Path::parse("@products/alpha/api").unwrap();
        assert!(path.matches_pattern("*/alpha/api"));
        assert!(path.matches_pattern("**/api"));
    }

    #[test]
    fn test_wildcard_double_zero_segments() {
        // ** can match zero segments
        let path = Path::parse("@products/timeout").unwrap();
        assert!(path.matches_pattern("@products/**/timeout"));
    }

    #[test]
    fn test_display() {
        let path = Path::parse("@meh/todo/item").unwrap();
        assert_eq!(format!("{}", path), "@meh/todo/item");
        
        let root = Path::parse("/").unwrap();
        assert_eq!(format!("{}", root), "/");
    }

    #[test]
    fn test_try_from_str() {
        let path: Path = "@test/path".try_into().unwrap();
        assert_eq!(path.depth(), 2);
    }

    #[test]
    fn test_try_from_string() {
        let path: Path = String::from("@test/path").try_into().unwrap();
        assert_eq!(path.depth(), 2);
    }

    #[test]
    fn test_reserved_prefix() {
        let reserved = Path::parse("@products/alpha").unwrap();
        assert!(reserved.is_reserved_prefix());
        
        let not_reserved = Path::parse("@meh/todo").unwrap();
        assert!(!not_reserved.is_reserved_prefix());
        
        let custom = Path::parse("@custom/path").unwrap();
        assert!(!custom.is_reserved_prefix());
    }

    #[test]
    fn test_join_empty() {
        let path = Path::parse("@products").unwrap();
        let joined = path.join("").unwrap();
        assert_eq!(joined.to_string(), "@products");
    }

    #[test]
    fn test_multiple_consecutive_slashes() {
        let path = Path::parse("@products//alpha///beta").unwrap();
        assert_eq!(path.segments().len(), 3);
        assert_eq!(path.segments(), &["@products", "alpha", "beta"]);
    }
}
