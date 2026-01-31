//! `meh ls` and `meh tree` commands
//!
//! Browse the knowledge structure.
//!
//! # Usage
//! ```bash
//! meh ls                          # List root paths
//! meh ls @products/               # List children
//! meh ls @products/alpha/ --depth 2
//!
//! meh tree                        # Full tree
//! meh tree @products/alpha/       # Subtree
//! meh tree --depth 3              # Limit depth
//! ```
//!
//! # Architecture
//! See `../../plan/ANALYSIS_KNOWLEDGE_ORGANIZATION.md`

use anyhow::{bail, Result};
use clap::Args;
use std::collections::HashMap;
use std::path::PathBuf;

use crate::core::storage::Storage;

#[derive(Args, Debug)]
pub struct LsArgs {
    /// Path to list (default: root)
    #[arg(default_value = "@")]
    pub path: String,

    /// Depth of listing (1 = direct children only)
    #[arg(short, long, default_value = "1")]
    pub depth: usize,

    /// Show fact count for directories
    #[arg(long)]
    pub count: bool,

    /// Detail level (catalog = just names, index = with titles)
    #[arg(long, default_value = "catalog")]
    pub level: String,

    /// Include hidden/internal paths
    #[arg(long)]
    pub all: bool,
}

pub fn run_ls(args: LsArgs) -> Result<()> {
    // 1. Find .meh directory
    let meh_dir = find_meh_dir()?;
    let db_path = meh_dir.join("data.db");
    let storage = Storage::open(&db_path)?;

    // 2. Get children at the given path
    let prefix = args.path.trim_end_matches('/');
    let children = storage.list_children_all(prefix)?;

    if children.is_empty() {
        println!("No facts found under: {}", prefix);
        return Ok(());
    }

    println!("📂 {}/\n", prefix);

    for info in children {
        let display_path = info.path.trim_start_matches(prefix).trim_start_matches('/');
        if args.count {
            println!("  {}  ({} facts)", display_path, info.fact_count);
        } else {
            println!("  {}", display_path);
        }
    }

    Ok(())
}

#[derive(Args, Debug)]
pub struct TreeArgs {
    /// Path to show tree for (default: root)
    #[arg(default_value = "@")]
    pub path: String,

    /// Maximum depth
    #[arg(short, long, default_value = "3")]
    pub depth: usize,

    /// Show fact count
    #[arg(long)]
    pub count: bool,

    /// Show only directories (no leaf facts)
    #[arg(long)]
    pub dirs_only: bool,
}

pub fn run_tree(args: TreeArgs) -> Result<()> {
    // 1. Find .meh directory
    let meh_dir = find_meh_dir()?;
    let db_path = meh_dir.join("data.db");
    let storage = Storage::open(&db_path)?;

    // 2. Get all facts with prefix and build tree
    let prefix = args.path.trim_end_matches('/');
    let facts = storage.get_by_path_prefix(prefix)?;

    if facts.is_empty() {
        println!("No facts found under: {}", prefix);
        return Ok(());
    }

    // Build tree structure
    let mut tree = PathTree::new();
    for fact in &facts {
        tree.add_path(&fact.path);
    }

    // Print tree
    println!("📂 {}", if prefix.is_empty() { "@" } else { prefix });
    tree.print(prefix, args.depth, "", true);

    let total: usize = facts.len();
    println!("\n{} facts total", total);

    Ok(())
}

/// Simple tree structure for path visualization
struct PathTree {
    children: HashMap<String, PathTree>,
    is_leaf: bool,
}

impl PathTree {
    fn new() -> Self {
        Self {
            children: HashMap::new(),
            is_leaf: false,
        }
    }

    fn add_path(&mut self, path: &str) {
        let segments: Vec<&str> = path
            .trim_start_matches('@')
            .trim_start_matches('/')
            .split('/')
            .filter(|s| !s.is_empty())
            .collect();

        self.add_segments(&segments, 0);
    }

    fn add_segments(&mut self, segments: &[&str], idx: usize) {
        if idx >= segments.len() {
            self.is_leaf = true;
            return;
        }

        let segment = segments[idx].to_string();
        let child = self.children.entry(segment).or_insert_with(PathTree::new);
        child.add_segments(segments, idx + 1);
    }

    fn print(&self, _prefix: &str, max_depth: usize, indent: &str, is_last: bool) {
        self.print_level(max_depth, 0, indent, is_last);
    }

    fn print_level(&self, max_depth: usize, current_depth: usize, indent: &str, _is_last: bool) {
        if current_depth >= max_depth {
            return;
        }

        let mut keys: Vec<_> = self.children.keys().collect();
        keys.sort();

        for (i, key) in keys.iter().enumerate() {
            let is_last_child = i == keys.len() - 1;
            let child = &self.children[*key];

            let branch = if is_last_child {
                "└── "
            } else {
                "├── "
            };
            let suffix = if child.children.is_empty() { "" } else { "/" };
            println!("{}{}{}{}", indent, branch, key, suffix);

            let new_indent = format!("{}{}", indent, if is_last_child { "    " } else { "│   " });
            child.print_level(max_depth, current_depth + 1, &new_indent, is_last_child);
        }
    }
}

/// Find .meh directory by walking up from current directory
fn find_meh_dir() -> Result<PathBuf> {
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
