use agentdev::discovery::{DiscoveredWorktree, DiscoveryOptions, discover_worktrees};
use anyhow::Result;
use colored::Colorize;

pub fn handle_discovery(recursive: bool, json: bool) -> Result<()> {
    let options = DiscoveryOptions {
        recursive,
        ..DiscoveryOptions::default()
    };

    let discovered = discover_worktrees(options)?;

    if json {
        println!("{}", serde_json::to_string_pretty(&discovered)?);
        return Ok(());
    }

    if discovered.is_empty() {
        println!("{} No unmanaged git worktrees found", "📭".yellow());
        return Ok(());
    }

    println!("{} Discovered unmanaged git worktrees:\n", "📂".cyan());

    let mut current_repo: Option<&str> = None;
    for entry in &discovered {
        if current_repo != Some(entry.repo.as_str()) {
            if current_repo.is_some() {
                println!();
            }
            println!("  {} {}", "📦".blue(), entry.repo.bold());
            current_repo = Some(entry.repo.as_str());
        }

        print_entry(entry);
    }

    println!();
    println!(
        "{} Use {} to add a worktree under management.",
        "💡".cyan(),
        "agentdev worktree add".bold()
    );

    Ok(())
}

fn print_entry(entry: &DiscoveredWorktree) {
    println!("    • {}", entry.path.cyan());
    if let Some(branch) = &entry.branch {
        println!("      Branch: {}", branch);
    }
    if let Some(head) = &entry.head {
        println!("      HEAD: {}", head);
    }
    if entry.bare {
        println!("      {}", "Bare worktree".dimmed());
    }
    if let Some(locked) = &entry.locked {
        println!("      Locked: {}", locked);
    }
    if let Some(prunable) = &entry.prunable {
        println!("      Prunable: {}", prunable);
    }
}
