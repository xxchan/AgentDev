use super::*;
use expect_test::expect;
use serde_json::{Value, json};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

fn fixture_path(file_name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/sessions/codex")
        .join(file_name)
}

fn snapshot_events(events: &[SessionEvent]) -> String {
    serde_json::to_string_pretty(events).expect("serialize events")
}

fn load_fixture(file_name: &str) -> Vec<SessionEvent> {
    let file = File::open(fixture_path(file_name)).expect("open codex fixture");
    let reader = BufReader::new(file);
    reader
        .lines()
        .map(|line| line.expect("read line"))
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| serde_json::from_str::<Value>(&line).ok())
        .filter_map(|value| CodexParsedEntry::parse(value))
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
        "type": "event_msg",
        "timestamp": "2025-01-02T03:04:05Z",
        "payload": {
            "type": "tool_use",
            "role": "assistant",
            "name": "shell",
            "input": {
                "command": "git status",
                "timeout": 30
            },
            "cwd": "/tmp/repo"
        }
    });

    let entry = CodexParsedEntry::parse(raw).expect("parse codex entry");
    let event = entry.to_event(false).expect("convert codex tool event");

    expect![[r#"
        {
          "actor": "assistant",
          "category": "tool_use",
          "label": "Tool Use · shell",
          "text": "{\n  \"cwd\": \"/tmp/repo\",\n  \"input\": {\n    \"command\": \"git status\",\n    \"timeout\": 30\n  },\n  \"name\": \"shell\",\n  \"role\": \"assistant\",\n  \"type\": \"tool_use\"\n}",
          "data": {
            "content": null,
            "cwd": "/tmp/repo",
            "id": null,
            "input": {
              "command": "git status",
              "timeout": 30
            },
            "instructions": null,
            "message": null,
            "name": "shell",
            "originator": null,
            "role": "assistant",
            "type": "tool_use",
            "working_dir": "/tmp/repo"
          },
          "timestamp": "2025-01-02T03:04:05Z",
          "tool": {
            "phase": "use",
            "name": "shell",
            "input": {
              "command": "git status",
              "timeout": 30
            },
            "working_dir": "/tmp/repo"
          }
        }"#]]
    .assert_eq(&serde_json::to_string_pretty(&event).expect("serialize event"));
}

#[test]
fn real_session_tool_events_snapshot() {
    let events =
        load_fixture("rollout-2025-10-17T20-38-29-0199f22d-c547-7eb3-99cd-bfef8fb5430c.jsonl");
    assert_snapshot(
        "tests/fixtures/snapshots/codex/real_session_tool_events.json",
        &snapshot_events(&events),
    );
}

#[test]
fn skips_duplicate_user_response_items() {
    let events = load_fixture("user_message_duplicate.jsonl");
    assert_eq!(events.len(), 2, "should only emit one user and one assistant event");

    let user_event = &events[0];
    assert_eq!(user_event.actor.as_deref(), Some("user"));
    assert_eq!(user_event.category, "user_message");
    assert_eq!(user_event.text.as_deref(), Some("commit changes"));

    let assistant_event = &events[1];
    assert_eq!(assistant_event.actor.as_deref(), Some("assistant"));
    assert_eq!(assistant_event.category, "response_item");
    assert_eq!(
        assistant_event.text.as_deref(),
        Some("I committed the changes.")
    );
}
