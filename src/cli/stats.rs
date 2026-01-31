//! Stats command - Show database statistics

use clap::Args;

use crate::config::Config;
use crate::core::storage::Storage;

/// Stats command arguments
#[derive(Args, Debug)]
pub struct StatsArgs {
    /// Output as JSON
    #[arg(long)]
    pub json: bool,
}

/// Execute stats command
pub fn execute(args: StatsArgs) -> anyhow::Result<()> {
    let config = Config::load()?;
    let db_path = config.data_dir();
    let storage = Storage::open(&db_path)?;

    let stats = storage.stats()?;

    if args.json {
        let json = serde_json::json!({
            "total_facts": stats.total_facts,
            "active_facts": stats.active_facts,
            "deprecated_facts": stats.deprecated_facts,
            "superseded_facts": stats.total_facts - stats.active_facts - stats.deprecated_facts,
        });
        println!("{}", serde_json::to_string_pretty(&json)?);
    } else {
        println!("📊 Knowledge Base Statistics\n");
        println!("  Total facts:      {}", stats.total_facts);
        println!(
            "  ├── Active:       {} ({}%)",
            stats.active_facts,
            if stats.total_facts > 0 {
                stats.active_facts * 100 / stats.total_facts
            } else {
                0
            }
        );
        println!(
            "  ├── Deprecated:   {} ({}%)",
            stats.deprecated_facts,
            if stats.total_facts > 0 {
                stats.deprecated_facts * 100 / stats.total_facts
            } else {
                0
            }
        );
        let superseded = stats.total_facts - stats.active_facts - stats.deprecated_facts;
        println!(
            "  └── Superseded:   {} ({}%)",
            superseded,
            if stats.total_facts > 0 {
                superseded * 100 / stats.total_facts
            } else {
                0
            }
        );

        // Get top paths
        if let Ok(paths) = storage.list_children_all("@") {
            if !paths.is_empty() {
                println!("\n📂 Top-level paths:");
                for path_info in paths.iter().take(10) {
                    println!("  {} ({} facts)", path_info.path, path_info.fact_count);
                }
            }
        }

        // Show database path
        println!("\n📁 Database: {}", db_path.display());
    }

    Ok(())
}
