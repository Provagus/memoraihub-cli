//! Remote search command handler
//!
//! Searches facts on a remote server instead of local database.

use anyhow::Result;
use colored::Colorize;

use crate::config::Config;
use crate::remote::{RemoteClient, RemoteFact};

/// Options for remote search
pub struct RemoteSearchOptions<'a> {
    pub server_url: &'a str,
    pub kb_slug: &'a str,
    pub query: &'a str,
    pub limit: Option<usize>,
    pub path_filter: Option<&'a str>,
    pub format: &'a str,
}

/// Execute remote search
pub async fn remote_search(opts: RemoteSearchOptions<'_>) -> Result<()> {
    let config = Config::load()?;
    let client = RemoteClient::from_url_with_config(opts.server_url, &config)?;

    let results = client
        .search(opts.kb_slug, opts.query, opts.limit, opts.path_filter)
        .await?;

    match opts.format {
        "json" => print_json(&results),
        "compact" => print_compact(&results),
        _ => print_pretty(&results, opts.kb_slug),
    }

    Ok(())
}

fn print_pretty(facts: &[RemoteFact], kb_slug: &str) {
    if facts.is_empty() {
        println!("No results found in '{}'.", kb_slug);
        return;
    }

    println!("{} results in {}", facts.len(), kb_slug.cyan());
    println!("{}", "─".repeat(60));

    for fact in facts {
        println!();
        println!("{} {}", "##".dimmed(), fact.path.cyan().bold());
        println!("   {}", fact.title);

        if let Some(summary) = &fact.summary {
            println!("   {}", summary.dimmed());
        }

        println!(
            "   {} {} | trust: {:.2}",
            "ID:".dimmed(),
            fact.id.dimmed(),
            fact.trust_score
        );
    }
}

fn print_compact(facts: &[RemoteFact]) {
    for fact in facts {
        println!("{} | {}", fact.path, fact.title);
    }
}

fn print_json(facts: &[RemoteFact]) {
    match serde_json::to_string_pretty(facts) {
        Ok(json) => println!("{}", json),
        Err(e) => eprintln!("Failed to serialize: {}", e),
    }
}

/// Options for remote add
pub struct RemoteAddOptions<'a> {
    pub server_url: &'a str,
    pub kb_slug: &'a str,
    pub path: &'a str,
    pub title: &'a str,
    pub content: &'a str,
    pub tags: Option<Vec<String>>,
}

/// Execute remote add
pub async fn remote_add(opts: RemoteAddOptions<'_>) -> Result<()> {
    let config = Config::load()?;
    let client = RemoteClient::from_url_with_config(opts.server_url, &config)?;

    let req = crate::remote::CreateFactRequest {
        path: opts.path.to_string(),
        title: opts.title.to_string(),
        content: opts.content.to_string(),
        tags: opts.tags,
    };

    let fact = client.create_fact(opts.kb_slug, req).await?;

    println!("{} Created fact: {}", "✓".green(), fact.id);
    println!("  Path: {}", fact.path.cyan());
    println!("  KB:   {}", opts.kb_slug);

    Ok(())
}

/// Options for remote show
pub struct RemoteShowOptions<'a> {
    pub server_url: &'a str,
    pub kb_slug: &'a str,
    pub fact_id: &'a str,
    pub format: &'a str,
}

/// Execute remote show
pub async fn remote_show(opts: RemoteShowOptions<'_>) -> Result<()> {
    let config = Config::load()?;
    let client = RemoteClient::from_url_with_config(opts.server_url, &config)?;

    let fact = client.get_fact(opts.kb_slug, opts.fact_id).await?;

    match opts.format {
        "json" => {
            let json = serde_json::to_string_pretty(&fact)?;
            println!("{}", json);
        }
        _ => {
            println!("{}", "Fact".bold());
            println!("{}", "═".repeat(40));
            println!("ID:      {}", fact.id);
            println!("Path:    {}", fact.path.cyan());
            println!("Title:   {}", fact.title);
            if let Some(status) = &fact.status {
                println!("Status:  {}", status);
            }
            println!("Trust:   {:.2}", fact.trust_score);
            if let Some(created_at) = &fact.created_at {
                println!("Created: {}", created_at);
            }

            if !fact.tags.is_empty() {
                println!("Tags:    {}", fact.tags.join(", "));
            }

            if let Some(content) = &fact.content {
                println!();
                println!("{}", "Content".bold());
                println!("{}", "─".repeat(40));
                println!("{}", content);
            }
        }
    }

    Ok(())
}

/// Options for remote browse
pub struct RemoteBrowseOptions<'a> {
    pub server_url: &'a str,
    pub kb_slug: &'a str,
    pub path: Option<&'a str>,
    pub depth: Option<usize>,
}

/// Execute remote browse (ls)
pub async fn remote_browse(opts: RemoteBrowseOptions<'_>) -> Result<()> {
    let config = Config::load()?;
    let client = RemoteClient::from_url_with_config(opts.server_url, &config)?;

    let nodes = client.browse(opts.kb_slug, opts.path, opts.depth).await?;

    if nodes.is_empty() {
        println!("No paths found.");
        return Ok(());
    }

    for node in &nodes {
        let suffix = if node.has_children { "/" } else { "" };
        let count = if node.fact_count > 0 {
            format!(" ({})", node.fact_count).dimmed().to_string()
        } else {
            String::new()
        };

        println!("{}{}{}", node.path.cyan(), suffix, count);
    }

    Ok(())
}
