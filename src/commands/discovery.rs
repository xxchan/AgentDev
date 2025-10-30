use agentdev::discovery::{
    DiscoveredWorktree, DiscoveryOptions, add_discovered_to_state, discover_worktrees,
};
use agentdev::state::WorktreeInfo;
use anyhow::Result;
use colored::Colorize;

pub fn handle_discovery(recursive: bool, json: bool) -> Result<()> {
    let options = DiscoveryOptions {
        recursive,
        ..DiscoveryOptions::default()
    };

    let discovered = discover_worktrees(options)?;
    let newly_added = add_discovered_to_state(&discovered)?;

    if json {
        println!("{}", serde_json::to_string_pretty(&discovered)?);
        return Ok(());
    }

    if discovered.is_empty() {
        println!("{} No unmanaged git worktrees found", "📭".yellow());
        print_persistence_summary(&newly_added);
        return Ok(());
    }

    println!("{} Discovered git worktrees (now managed):\n", "📂".cyan());

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
    print_persistence_summary(&newly_added);

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

fn print_persistence_summary(added: &[WorktreeInfo]) {
    if added.is_empty() {
        println!(
            "{} All discovered worktrees were already managed.",
            "ℹ️".blue()
        );
        return;
    }

    println!(
        "{} Registered {} worktree(s) with agentdev:",
        "✅".green(),
        added.len()
    );
    for info in added {
        println!(
            "  • {}/{} ({})",
            info.repo_name,
            info.name,
            info.path.display()
        );
    }
}
