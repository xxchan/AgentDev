use super::*;
use expect_test::expect;
use serde_json::{json, Value};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

fn fixture_path(file_name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/sessions/claude")
        .join(file_name)
}

fn snapshot_events(events: &[SessionEvent]) -> String {
    serde_json::to_string_pretty(events).expect("serialize events")
}

fn load_fixture(file_name: &str) -> Vec<SessionEvent> {
    let file = File::open(fixture_path(file_name)).expect("open claude fixture");
    let reader = BufReader::new(file);
    reader
        .lines()
        .map(|line| line.expect("read line"))
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| serde_json::from_str::<Value>(&line).ok())
        .filter_map(|value| ClaudeParsedEntry::parse(value))
        .filter_map(|entry| entry.to_event(false))
        .collect()
}

fn assert_snapshot(path: &str, content: &str) {
    let absolute = Path::new(env!("CARGO_MANIFEST_DIR")).join(path);
    if std::env::var("UPDATE_EXPECT").is_ok() {
        if let Some(parent) = absolute.parent() {
            std::fs::create_dir_all(parent).expect("create snapshot dir");
        }
        std::fs::write(&absolute, content).expect("write snapshot");
    } else {
        let expected = std::fs::read_to_string(&absolute).expect("read snapshot");
        assert_eq!(expected, content, "snapshot mismatch at {}", path);
    }
}

#[test]
fn tool_use_event_snapshot() {
    let raw = json!({
        "timestamp": "2025-01-02T03:04:05Z",
        "id": "event-1",
        "cwd": "/tmp/repo",
        "message": {
            "role": "assistant",
            "content": [
                {
                    "type": "tool_use",
                    "name": "git_diff",
                    "input": {
                        "paths": ["src/lib.rs"],
                        "commit": "HEAD~1"
                    }
                },
                {
                    "type": "text",
                    "text": "diff output truncated"
                }
            ]
        }
    });

    let entry = ClaudeParsedEntry::parse(raw).expect("parse claude entry");
    let event = entry.to_event(false).expect("convert claude tool event");

    expect![[r#"
        {
          "actor": "assistant",
          "category": "tool_use",
          "label": "Tool Use · git_diff",
          "text": "Tool call: git_diff\n{\n  \"commit\": \"HEAD~1\",\n  \"paths\": [\n    \"src/lib.rs\"\n  ]\n}\n\ndiff output truncated",
          "data": {
            "id": "event-1",
            "working_dir": "/tmp/repo"
          },
          "timestamp": "2025-01-02T03:04:05Z",
          "tool": {
            "phase": "use",
            "name": "git_diff",
            "input": {
              "commit": "HEAD~1",
              "paths": [
                "src/lib.rs"
              ]
            },
            "working_dir": "/tmp/repo"
          }
        }"#]]
    .assert_eq(&serde_json::to_string_pretty(&event).expect("serialize event"));
}

#[test]
fn real_session_parallel_tool_snapshot() {
    let events = load_fixture("c3bcc6a0-de15-480b-94ec-f0f0521abf18.jsonl");
    assert_snapshot(
        "tests/fixtures/snapshots/claude/real_session_parallel_tools.json",
        &snapshot_events(&events),
    );
}
