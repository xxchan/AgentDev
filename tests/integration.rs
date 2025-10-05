use assert_cmd::Command;
use insta::{assert_json_snapshot, assert_snapshot};
use regex::Regex;
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

struct TestContext {
    temp_dir: TempDir,
    temp_dir_str: String, // Store temp dir path as string for easy replacement
    repo_dir: PathBuf,
    config_dir: PathBuf,
}

impl TestContext {
    fn new(repo_name: &str) -> Self {
        let temp_dir = TempDir::new().unwrap();
        // Get the canonical path to handle symlinks and /private prefix on macOS
        let temp_dir_str = temp_dir
            .path()
            .canonicalize()
            .unwrap_or_else(|_| temp_dir.path().to_path_buf())
            .to_string_lossy()
            .to_string();
        let repo_dir = temp_dir.path().join(repo_name);
        let config_dir = temp_dir.path().join(".config/xlaude");

        // Initialize test git repo
        Self::init_test_repo(&repo_dir);

        // Create config directory and default state with test agent
        fs::create_dir_all(&config_dir).unwrap();
        let default_state = json!({
            "worktrees": {},
            "editor": null,
            "agent": "true"
        });
        fs::write(
            config_dir.join("state.json"),
            serde_json::to_string_pretty(&default_state).unwrap(),
        )
        .unwrap();

        Self {
            temp_dir,
            temp_dir_str,
            repo_dir,
            config_dir,
        }
    }

    fn init_test_repo(path: &Path) {
        fs::create_dir_all(path).unwrap();

        // Initialize git repository
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(path)
            .output()
            .unwrap();

        // Configure git user for tests
        std::process::Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(path)
            .output()
            .unwrap();

        std::process::Command::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(path)
            .output()
            .unwrap();

        // Create initial commit
        fs::write(path.join("README.md"), "# Test Repo").unwrap();
        std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(path)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["commit", "--no-gpg-sign", "-m", "Initial commit"])
            .current_dir(path)
            .output()
            .unwrap();
    }

    fn xlaude(&self, args: &[&str]) -> Command {
        let mut cmd = Command::cargo_bin("agentdev").unwrap();
        cmd.current_dir(&self.repo_dir)
            .env("HOME", self.temp_dir.path())
            .env("XLAUDE_CONFIG_DIR", &self.config_dir)
            // Run in test mode to avoid auto-open prompts
            .env("XLAUDE_TEST_MODE", "1")
            // Disable color output for consistent snapshots
            .env("NO_COLOR", "1")
            // Enable non-interactive mode for testing
            .env("XLAUDE_NON_INTERACTIVE", "1");

        cmd.args(args);
        cmd
    }

    fn xlaude_in_dir(&self, dir: &Path, args: &[&str]) -> Command {
        let mut cmd = Command::cargo_bin("agentdev").unwrap();
        cmd.current_dir(dir)
            .env("HOME", self.temp_dir.path())
            .env("XLAUDE_CONFIG_DIR", &self.config_dir)
            .env("XLAUDE_TEST_MODE", "1")
            .env("NO_COLOR", "1")
            .env("XLAUDE_NON_INTERACTIVE", "1");

        cmd.args(args);
        cmd
    }

    fn read_state(&self) -> serde_json::Value {
        let state_path = self.config_dir.join("state.json");
        if state_path.exists() {
            let content = fs::read_to_string(state_path).unwrap();
            serde_json::from_str(&content).unwrap()
        } else {
            json!({ "worktrees": [] })
        }
    }

    fn write_state(&self, state: &serde_json::Value) {
        let state_path = self.config_dir.join("state.json");
        fs::write(state_path, serde_json::to_string_pretty(state).unwrap()).unwrap();
    }

    fn worktree_exists(&self, name: &str) -> bool {
        self.temp_dir
            .path()
            .join(format!("test-repo-{name}"))
            .exists()
    }

    fn redact_paths(&self, text: &str) -> String {
        // Replace the actual temp directory path with a placeholder
        text.replace(&self.temp_dir_str, "/tmp/TEST_DIR")
    }

    fn redact_output(&self, text: &str) -> String {
        // Redact both paths and timestamps
        let mut result = self.redact_paths(text);

        // Replace timestamps like "2024-01-01 12:34:56" with "[TIMESTAMP]"
        let re = Regex::new(r"\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}").unwrap();
        result = re.replace_all(&result, "[TIMESTAMP]").to_string();

        result
    }
}

// Create command tests
#[test]
fn test_create_with_name() {
    let ctx = TestContext::new("test-repo");

    // Execute command
    let output = ctx.xlaude(&["create", "feature-x"]).assert().success();

    // Snapshot test output with path redaction
    let stdout = String::from_utf8_lossy(&output.get_output().stdout);
    let redacted = ctx.redact_paths(&stdout);
    assert_snapshot!(redacted);

    // Snapshot test state with redactions for dynamic values
    let mut state = ctx.read_state();
    // Remove dynamic or config-only fields
    if let Some(obj) = state.as_object_mut() {
        obj.remove("agent");
    }
    // Manually redact dynamic values
    if let Some(worktrees) = state["worktrees"].as_object_mut() {
        for (_, worktree) in worktrees {
            worktree["created_at"] = json!("[TIMESTAMP]");
            if let Some(path) = worktree["path"].as_str() {
                worktree["path"] = json!(ctx.redact_paths(path));
            }
        }
    }
    assert_json_snapshot!(state);

    // Verify worktree was created
    assert!(ctx.worktree_exists("feature-x"));
}

#[test]
fn test_create_random_name() {
    let ctx = TestContext::new("test-repo");

    // Set fixed random seed for reproducibility
    let output = ctx
        .xlaude(&["create"])
        .env("XLAUDE_TEST_SEED", "42")
        .assert()
        .success();

    let stdout = String::from_utf8_lossy(&output.get_output().stdout);

    // Verify a worktree was created (name will vary based on random selection)
    assert!(stdout.contains("Creating worktree"));
    assert!(stdout.contains("Worktree created at"));
}

#[test]
fn test_create_on_wrong_branch() {
    let ctx = TestContext::new("test-repo");

    // Switch to non-main branch
    std::process::Command::new("git")
        .args(["checkout", "-b", "feature-branch"])
        .current_dir(&ctx.repo_dir)
        .output()
        .unwrap();

    let output = ctx.xlaude(&["create", "test"]).assert().failure();

    let stderr = String::from_utf8_lossy(&output.get_output().stderr);
    let redacted = ctx.redact_paths(&stderr);
    assert_snapshot!(redacted);
}

// List command tests
#[test]
fn test_list_empty() {
    let ctx = TestContext::new("test-repo");

    let output = ctx.xlaude(&["list"]).assert().success();

    let stdout = String::from_utf8_lossy(&output.get_output().stdout);
    let redacted = ctx.redact_paths(&stdout);
    assert_snapshot!(redacted);
}

#[test]
fn test_list_with_worktrees() {
    let ctx = TestContext::new("test-repo");

    // Create a few worktrees
    ctx.xlaude(&["create", "feature-a"]).assert().success();
    ctx.xlaude(&["create", "feature-b"]).assert().success();

    let output = ctx.xlaude(&["list"]).assert().success();

    // Manually redact timestamps and paths in output for consistent snapshots
    let stdout = String::from_utf8_lossy(&output.get_output().stdout);
    let redacted_stdout = ctx.redact_output(&stdout);
    assert_snapshot!(redacted_stdout);
}

// Delete command tests
#[test]
fn test_delete_clean_worktree() {
    let ctx = TestContext::new("test-repo");

    // Create worktree
    ctx.xlaude(&["create", "to-delete"]).assert().success();

    // Delete worktree (in non-interactive mode, clean worktree will be deleted automatically)
    let output = ctx.xlaude(&["delete", "to-delete"]).assert().success();

    let stdout = String::from_utf8_lossy(&output.get_output().stdout);
    let redacted = ctx.redact_paths(&stdout);
    assert_snapshot!(redacted);

    // Verify worktree was deleted
    assert!(!ctx.worktree_exists("to-delete"));

    // Verify state was updated
    let state = ctx.read_state();
    assert_eq!(state["worktrees"].as_object().unwrap().len(), 0);
}

#[test]
fn test_delete_with_changes() {
    let ctx = TestContext::new("test-repo");

    // Create worktree
    ctx.xlaude(&["create", "with-changes"]).assert().success();

    // Create uncommitted changes in worktree
    let worktree_path = ctx.temp_dir.path().join("test-repo-with-changes");
    fs::write(worktree_path.join("new-file.txt"), "content").unwrap();

    // Try to delete, in non-interactive mode it will be cancelled automatically
    let output = ctx.xlaude(&["delete", "with-changes"]).assert().success();

    // Check that output mentions uncommitted changes and cancellation
    let stdout = String::from_utf8_lossy(&output.get_output().stdout);
    assert!(stdout.contains("uncommitted changes"));
    assert!(stdout.contains("Cancelled"));

    // Verify worktree was not deleted
    assert!(worktree_path.exists());
}

#[test]
fn test_delete_current_worktree() {
    let ctx = TestContext::new("test-repo");

    // Create worktree
    ctx.xlaude(&["create", "current"]).assert().success();

    // Switch to the worktree directory
    let worktree_path = ctx.temp_dir.path().join("test-repo-current");

    // Delete from within the worktree (no name specified)
    let output = ctx
        .xlaude_in_dir(&worktree_path, &["delete"])
        .assert()
        .success();

    let stdout = String::from_utf8_lossy(&output.get_output().stdout);
    // Just check that the command succeeded and output something reasonable
    assert!(stdout.contains("Checking worktree") || stdout.contains("deleted"));
}

// Add command tests
#[test]
fn test_add_existing_worktree() {
    let ctx = TestContext::new("test-repo");

    // Manually create worktree
    std::process::Command::new("git")
        .args([
            "worktree",
            "add",
            "../test-repo-manual",
            "-b",
            "manual-branch",
        ])
        .current_dir(&ctx.repo_dir)
        .output()
        .unwrap();

    // Switch to manually created worktree
    let manual_worktree = ctx.temp_dir.path().join("test-repo-manual");

    let output = ctx
        .xlaude_in_dir(&manual_worktree, &["add", "manual"])
        .assert()
        .success();

    let stdout = String::from_utf8_lossy(&output.get_output().stdout);
    let redacted = ctx.redact_paths(&stdout);
    assert_snapshot!(redacted);

    // Verify state with manual redactions
    let mut state = ctx.read_state();
    if let Some(obj) = state.as_object_mut() {
        obj.remove("agent");
    }
    if let Some(worktrees) = state["worktrees"].as_object_mut() {
        for (_, worktree) in worktrees {
            worktree["created_at"] = json!("[TIMESTAMP]");
            if let Some(path) = worktree["path"].as_str() {
                worktree["path"] = json!(ctx.redact_paths(path));
            }
        }
    }
    assert_json_snapshot!(state);
}

#[test]
fn test_add_duplicate_path_is_rejected() {
    let ctx = TestContext::new("test-repo");

    // Manually create worktree
    std::process::Command::new("git")
        .args(["worktree", "add", "../test-repo-dup", "-b", "dup-branch"])
        .current_dir(&ctx.repo_dir)
        .output()
        .unwrap();

    let manual_worktree = ctx.temp_dir.path().join("test-repo-dup");

    ctx.xlaude_in_dir(&manual_worktree, &["add", "primary"])
        .assert()
        .success();

    let assert = ctx
        .xlaude_in_dir(&manual_worktree, &["add", "secondary"])
        .assert()
        .failure();
    let stderr = String::from_utf8_lossy(&assert.get_output().stderr);
    assert!(
        stderr.contains("already managed"),
        "expected duplicate path error, got: {}",
        stderr
    );
}

#[test]
fn test_add_without_name() {
    let ctx = TestContext::new("test-repo");

    // Manually create worktree
    std::process::Command::new("git")
        .args(["worktree", "add", "../test-repo-auto", "-b", "auto-branch"])
        .current_dir(&ctx.repo_dir)
        .output()
        .unwrap();

    let auto_worktree = ctx.temp_dir.path().join("test-repo-auto");

    // Add without specifying name (should use branch name)
    let output = ctx
        .xlaude_in_dir(&auto_worktree, &["add"])
        .assert()
        .success();

    let stdout = String::from_utf8_lossy(&output.get_output().stdout);
    assert!(stdout.contains("Adding worktree") || stdout.contains("added successfully"));

    // Verify it was added with branch name
    let state = ctx.read_state();
    let worktrees = state["worktrees"].as_object().unwrap();
    // Key format is now "repo_name/worktree_name"
    // The repo name is correctly detected from the main repository
    assert!(worktrees.contains_key("test-repo/auto-branch"));
}

// Clean command tests
#[test]
fn test_clean_invalid_worktrees() {
    let ctx = TestContext::new("test-repo");

    // Create a valid worktree
    ctx.xlaude(&["create", "valid"]).assert().success();

    // Manually corrupt state file by adding invalid worktree
    let state = ctx.read_state();
    // Convert to proper structure for xlaude state format
    let worktrees_obj = state["worktrees"].as_object().cloned().unwrap_or_default();
    let mut new_worktrees = serde_json::Map::new();

    // Copy existing worktrees
    for (k, v) in worktrees_obj {
        new_worktrees.insert(k, v);
    }

    // Add invalid worktree with new key format
    new_worktrees.insert(
        "test-repo/invalid".to_string(),
        json!({
            "name": "invalid",
            "branch": "invalid",
            "repo_name": "test-repo",
            "path": "/non/existent/path",
            "created_at": "2024-01-01T00:00:00Z"
        }),
    );

    let mut new_state = serde_json::Map::new();
    new_state.insert("worktrees".to_string(), json!(new_worktrees));
    ctx.write_state(&json!(new_state));

    // Run clean
    let output = ctx.xlaude(&["clean"]).assert().success();

    let stdout = String::from_utf8_lossy(&output.get_output().stdout);
    let redacted = ctx.redact_paths(&stdout);
    assert_snapshot!(redacted);

    // Verify state was cleaned
    let cleaned_state = ctx.read_state();
    let worktrees = cleaned_state["worktrees"].as_object().unwrap();
    assert_eq!(worktrees.len(), 1);
    // Key format is now "repo_name/worktree_name"
    assert!(worktrees.contains_key("test-repo/valid"));
}

#[test]
fn test_clean_with_no_invalid() {
    let ctx = TestContext::new("test-repo");

    // Create valid worktrees
    ctx.xlaude(&["create", "valid1"]).assert().success();
    ctx.xlaude(&["create", "valid2"]).assert().success();

    // Run clean (should find no invalid worktrees)
    let output = ctx.xlaude(&["clean"]).assert().success();

    let stdout = String::from_utf8_lossy(&output.get_output().stdout);
    assert!(
        stdout.contains("No invalid worktrees found") || stdout.contains("All worktrees are valid")
    );
}

// Open command tests (basic, since we can't actually launch Claude)
#[test]
fn test_open_specific_worktree() {
    let ctx = TestContext::new("test-repo");

    // Create worktree
    ctx.xlaude(&["create", "to-open"]).assert().success();

    // Mock claude command to verify it would be called
    let output = ctx.xlaude(&["open", "to-open"]).assert().success();

    let stdout = String::from_utf8_lossy(&output.get_output().stdout);
    assert!(stdout.contains("Opening worktree"));
}

#[test]
fn test_open_nonexistent_worktree() {
    let ctx = TestContext::new("test-repo");

    let output = ctx.xlaude(&["open", "nonexistent"]).assert().failure();

    let stderr = String::from_utf8_lossy(&output.get_output().stderr);
    assert!(stderr.contains("not found") || stderr.contains("No worktree"));
}

// Migration test
#[test]
fn test_v02_to_v03_migration() {
    let ctx = TestContext::new("test-repo");

    // Create old format state file (v0.2 format with keys as just worktree names)
    let old_state = json!({
        "worktrees": {
            "feature-old": {
                "name": "feature-old",
                "branch": "feature-old",
                "repo_name": "test-repo",
                "path": ctx.temp_dir.path().join("test-repo-feature-old"),
                "created_at": "2024-01-01T00:00:00Z"
            },
            "bugfix": {
                "name": "bugfix",
                "branch": "bugfix-branch",
                "repo_name": "another-repo",
                "path": ctx.temp_dir.path().join("another-repo-bugfix"),
                "created_at": "2024-01-02T00:00:00Z"
            }
        }
    });

    // Write old format state
    ctx.write_state(&old_state);

    // Run any xlaude command that loads state (list is simplest)
    let output = ctx.xlaude(&["list"]).assert().success();
    let stdout = String::from_utf8_lossy(&output.get_output().stdout);

    // Check migration message was shown
    let stderr = String::from_utf8_lossy(&output.get_output().stderr);
    assert!(
        stderr.contains("Migrating xlaude state")
            || stdout.contains("another-repo")
            || stdout.contains("test-repo")
    );

    // Read the migrated state
    let migrated_state = ctx.read_state();
    let worktrees = migrated_state["worktrees"].as_object().unwrap();

    // Verify new key format
    assert!(worktrees.contains_key("test-repo/feature-old"));
    assert!(worktrees.contains_key("another-repo/bugfix"));

    // Verify old keys are gone
    assert!(!worktrees.contains_key("feature-old"));
    assert!(!worktrees.contains_key("bugfix"));

    // Verify data integrity
    assert_eq!(worktrees["test-repo/feature-old"]["name"], "feature-old");
    assert_eq!(worktrees["another-repo/bugfix"]["name"], "bugfix");
}

#[test]
fn test_mixed_format_migration() {
    let ctx = TestContext::new("test-repo");

    // Create mixed format state file (some old, some new)
    let mixed_state = json!({
        "worktrees": {
            "old-style": {
                "name": "old-style",
                "branch": "old-branch",
                "repo_name": "repo-a",
                "path": ctx.temp_dir.path().join("repo-a-old-style"),
                "created_at": "2024-01-01T00:00:00Z"
            },
            "repo-b/new-style": {
                "name": "new-style",
                "branch": "new-branch",
                "repo_name": "repo-b",
                "path": ctx.temp_dir.path().join("repo-b-new-style"),
                "created_at": "2024-01-02T00:00:00Z"
            }
        }
    });

    // Write mixed format state
    ctx.write_state(&mixed_state);

    // Run any xlaude command that loads state
    ctx.xlaude(&["list"]).assert().success();

    // Read the migrated state
    let migrated_state = ctx.read_state();
    let worktrees = migrated_state["worktrees"].as_object().unwrap();

    // Verify both entries are in new format
    assert!(worktrees.contains_key("repo-a/old-style"));
    assert!(worktrees.contains_key("repo-b/new-style"));

    // Verify old key is gone
    assert!(!worktrees.contains_key("old-style"));

    // Verify no data loss
    assert_eq!(worktrees.len(), 2);
}

#[test]
fn test_open_current_worktree_already_managed() {
    let ctx = TestContext::new("test-repo");

    // Create a worktree
    ctx.xlaude(&["create", "feature-x"]).assert().success();

    // Navigate to the worktree directory
    let worktree_dir = ctx.temp_dir.path().join("test-repo-feature-x");

    // Open from within the worktree - should open directly since it's already managed
    ctx.xlaude_in_dir(&worktree_dir, &["open"])
        .assert()
        .success()
        .stdout(predicates::str::contains("Opening current worktree"));
}

#[test]
fn test_open_current_worktree_not_managed() {
    let ctx = TestContext::new("test-repo");

    // Create a worktree manually using git
    std::process::Command::new("git")
        .args([
            "worktree",
            "add",
            "-b",
            "manual-branch",
            "../test-repo-manual",
        ])
        .current_dir(&ctx.repo_dir)
        .output()
        .unwrap();

    let worktree_dir = ctx.temp_dir.path().join("test-repo-manual");

    // Try to open from within the unmanaged worktree
    // In non-interactive mode, it should just print info and exit
    ctx.xlaude_in_dir(&worktree_dir, &["open"])
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "Current directory is a worktree but not managed",
        ));
}

#[test]
fn test_open_from_base_branch() {
    let ctx = TestContext::new("test-repo");

    // Create a worktree for testing
    ctx.xlaude(&["create", "feature-y"]).assert().success();

    // Try to open from main branch (should fall through to normal behavior)
    ctx.xlaude(&["open"])
        .assert()
        .failure() // Will fail in non-interactive mode since it needs selection
        .stderr(predicates::str::contains(
            "Interactive selection not available in non-interactive mode",
        ));
}

#[test]
fn test_open_from_main_repo_not_worktree() {
    let ctx = TestContext::new("test-repo");

    // Create some worktrees first
    ctx.xlaude(&["create", "feature-a"]).assert().success();
    ctx.xlaude(&["create", "feature-b"]).assert().success();

    // Try to open from the main repo (not a worktree, on main branch)
    // Should fall through to selection mode
    ctx.xlaude(&["open"])
        .assert()
        .failure()
        .stderr(predicates::str::contains(
            "Interactive selection not available",
        ));
}

#[test]
fn test_open_from_non_git_directory() {
    let temp_dir = TempDir::new().unwrap();
    let non_git_dir = temp_dir.path().join("not-a-repo");
    let config_dir = temp_dir.path().join(".config/xlaude");
    fs::create_dir_all(&non_git_dir).unwrap();
    fs::create_dir_all(&config_dir).unwrap();

    // Create an empty state file
    let state = json!({ "worktrees": {} });
    fs::write(config_dir.join("state.json"), state.to_string()).unwrap();

    // Try to open from a non-git directory with empty worktrees
    let mut cmd = Command::cargo_bin("agentdev").unwrap();
    cmd.current_dir(&non_git_dir)
        .env("HOME", temp_dir.path())
        .env("XLAUDE_CONFIG_DIR", &config_dir)
        .env("XLAUDE_NON_INTERACTIVE", "1")
        .arg("open")
        .assert()
        .failure()
        .stderr(predicates::str::contains("No worktrees found"));
}

#[test]
fn test_rename_command() {
    let ctx = TestContext::new("test-repo");

    // Create a worktree first
    ctx.xlaude(&["create", "old-name"]).assert().success();

    // Rename the worktree
    ctx.xlaude(&["rename", "old-name", "new-name"])
        .assert()
        .success()
        .stdout(predicates::str::contains("Renamed worktree"))
        .stdout(predicates::str::contains("old-name"))
        .stdout(predicates::str::contains("new-name"));

    // Verify the rename in the list
    ctx.xlaude(&["list"])
        .assert()
        .success()
        .stdout(predicates::str::contains("• new-name")); // Check that the name is updated in the list

    // Try to rename non-existent worktree
    ctx.xlaude(&["rename", "non-existent", "some-name"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("not found"));

    // Try to rename to existing name
    ctx.xlaude(&["create", "another-name"]).assert().success();

    ctx.xlaude(&["rename", "new-name", "another-name"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("already exists"));
}

#[test]
fn test_create_duplicate_name() {
    let ctx = TestContext::new("test-repo");

    // Create a worktree with a specific name
    ctx.xlaude(&["create", "my-feature"]).assert().success();

    // Try to create another worktree with the same name - should fail
    ctx.xlaude(&["create", "my-feature"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("already exists"))
        .stderr(predicates::str::contains("my-feature"));

    // Verify only one worktree exists in the state
    let state = ctx.read_state();
    if let Some(worktrees) = state["worktrees"].as_object() {
        assert_eq!(worktrees.len(), 1, "Should have exactly one worktree");
    }
}

#[test]
fn test_create_existing_git_worktree() {
    let ctx = TestContext::new("test-repo");

    // Create a worktree manually using git (not tracked by xlaude)
    std::process::Command::new("git")
        .args([
            "worktree",
            "add",
            "-b",
            "existing-feature",
            "../test-repo-existing-feature",
        ])
        .current_dir(&ctx.repo_dir)
        .output()
        .unwrap();

    // Try to create a worktree with the same name through xlaude - should fail
    ctx.xlaude(&["create", "existing-feature"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("already exists"));

    // Verify xlaude state is still empty
    let state = ctx.read_state();
    if let Some(worktrees) = state["worktrees"].as_object() {
        assert_eq!(
            worktrees.len(),
            0,
            "Should have no worktrees in xlaude state"
        );
    }
}

#[test]
fn test_create_existing_directory() {
    let ctx = TestContext::new("test-repo");

    // Create a directory manually (not a git worktree)
    let existing_dir = ctx.temp_dir.path().join("test-repo-existing-dir");
    fs::create_dir(&existing_dir).unwrap();
    fs::write(existing_dir.join("file.txt"), "existing content").unwrap();

    // Try to create a worktree with the same name - should fail
    ctx.xlaude(&["create", "existing-dir"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("Directory"))
        .stderr(predicates::str::contains("already exists"));

    // Verify xlaude state is still empty
    let state = ctx.read_state();
    if let Some(worktrees) = state["worktrees"].as_object() {
        assert_eq!(
            worktrees.len(),
            0,
            "Should have no worktrees in xlaude state"
        );
    }
}

#[test]
fn test_create_with_submodules() {
    let ctx = TestContext::new("test-repo");

    // Add a fake submodule to the test repo
    let gitmodules_content = r#"[submodule "lib/helper"]
    path = lib/helper
    url = https://github.com/example/helper.git
"#;
    fs::write(ctx.repo_dir.join(".gitmodules"), gitmodules_content).unwrap();

    // Stage and commit the .gitmodules file
    std::process::Command::new("git")
        .args(["add", ".gitmodules"])
        .current_dir(&ctx.repo_dir)
        .output()
        .unwrap();
    std::process::Command::new("git")
        .args(["commit", "--no-gpg-sign", "-m", "Add submodule"])
        .current_dir(&ctx.repo_dir)
        .output()
        .unwrap();

    // Create a worktree
    let output = ctx.xlaude(&["create", "with-submodule"]).assert().success();

    // Snapshot test output with path redaction
    let stdout = String::from_utf8_lossy(&output.get_output().stdout);
    let redacted = ctx.redact_paths(&stdout);
    assert_snapshot!(redacted);

    // Verify worktree was created
    assert!(ctx.worktree_exists("with-submodule"));
}

#[test]
fn test_create_without_submodules() {
    let ctx = TestContext::new("test-repo");

    // Create a worktree in a repo without submodules
    let output = ctx.xlaude(&["create", "no-submodule"]).assert().success();

    // Snapshot test output with path redaction
    let stdout = String::from_utf8_lossy(&output.get_output().stdout);
    let redacted = ctx.redact_paths(&stdout);
    assert_snapshot!(redacted);

    // Verify worktree was created
    assert!(ctx.worktree_exists("no-submodule"));

    // Ensure no submodule update message appears
    assert!(!stdout.contains("Updated submodules"));
    assert!(!stdout.contains("Warning: Failed to update submodules"));
}

#[test]
fn test_create_with_slash_in_branch_name() {
    let ctx = TestContext::new("test-repo");

    // Create worktree with branch name containing slash
    let output = ctx.xlaude(&["create", "fix/bug"]).assert().success();

    // Snapshot test output with path redaction
    let stdout = String::from_utf8_lossy(&output.get_output().stdout);
    let redacted = ctx.redact_paths(&stdout);
    assert_snapshot!(redacted);

    // Verify worktree was created with sanitized directory name
    assert!(ctx.worktree_exists("fix-bug"));

    // Verify state file has correct information
    let state = ctx.read_state();
    let key = "test-repo/fix-bug".to_string();
    assert!(state["worktrees"].as_object().unwrap().contains_key(&key));

    let worktree_info = &state["worktrees"][&key];
    assert_eq!(worktree_info["name"], "fix-bug");
    assert_eq!(worktree_info["branch"], "fix/bug");

    // Verify branch was created with original name
    let branch_output = std::process::Command::new("git")
        .args(["branch", "--list", "fix/bug"])
        .current_dir(&ctx.repo_dir)
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&branch_output.stdout).contains("fix/bug"));
}

#[test]
fn test_delete_with_slash_in_branch_name() {
    let ctx = TestContext::new("test-repo");

    // Create worktree with branch name containing slash
    ctx.xlaude(&["create", "feature/awesome"])
        .assert()
        .success();

    // Verify worktree exists with sanitized name
    assert!(ctx.worktree_exists("feature-awesome"));

    // Delete the worktree from within it
    let worktree_dir = ctx
        .repo_dir
        .parent()
        .unwrap()
        .join("test-repo-feature-awesome");
    let output = ctx
        .xlaude_in_dir(&worktree_dir, &["delete"])
        .assert()
        .success();

    let stdout = String::from_utf8_lossy(&output.get_output().stdout);
    assert!(stdout.contains("Worktree 'feature-awesome' deleted successfully"));

    // Verify worktree is gone
    assert!(!ctx.worktree_exists("feature-awesome"));

    // Verify state is updated
    let state = ctx.read_state();
    let key = "test-repo/feature-awesome".to_string();
    assert!(!state["worktrees"].as_object().unwrap().contains_key(&key));
}
