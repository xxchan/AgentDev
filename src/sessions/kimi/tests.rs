use super::*;
use expect_test::expect;
use serde_json::{Value, json};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

fn fixture_path(file_name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/sessions/kimi")
        .join(file_name)
}

fn snapshot_events(events: &[SessionEvent]) -> String {
    serde_json::to_string_pretty(events).expect("serialize events")
}

fn load_fixture(file_name: &str) -> Vec<SessionEvent> {
    let file = File::open(fixture_path(file_name)).expect("open kimi fixture");
    let reader = BufReader::new(file);
    reader
        .lines()
        .map(|line| line.expect("read line"))
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| serde_json::from_str::<Value>(&line).ok())
        .filter_map(|value| KimiParsedEntry::parse(value))
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
        "role": "assistant",
        "tool_calls": [
            {
                "id": "call-1",
                "type": "function",
                "function": {
                    "name": "git_diff",
                    "arguments": "{\"paths\": [\"src/lib.rs\"], \"commit\": \"HEAD\"}"
                }
            }
        ]
    });

    let entry = KimiParsedEntry::parse(raw).expect("parse kimi entry");
    let event = entry.to_event(false).expect("convert kimi tool event");

    expect![[r#"
        {
          "actor": "assistant",
          "category": "tool_use",
          "label": "Tool Use · git_diff",
          "text": "Tool calls:\n[\n  {\n    \"function\": {\n      \"arguments\": \"{\\\"paths\\\": [\\\"src/lib.rs\\\"], \\\"commit\\\": \\\"HEAD\\\"}\",\n      \"name\": \"git_diff\"\n    },\n    \"id\": \"call-1\",\n    \"type\": \"function\"\n  }\n]",
          "data": {
            "role": "assistant",
            "timestamp": "2025-01-02T03:04:05Z",
            "tool_calls": [
              {
                "function": {
                  "arguments": "{\"paths\": [\"src/lib.rs\"], \"commit\": \"HEAD\"}",
                  "name": "git_diff"
                },
                "id": "call-1",
                "type": "function"
              }
            ]
          },
          "timestamp": "2025-01-02T03:04:05Z",
          "tool": {
            "phase": "use",
            "name": "git_diff",
            "identifier": "call-1",
            "input": {
              "commit": "HEAD",
              "paths": [
                "src/lib.rs"
              ]
            }
          }
        }"#]]
    .assert_eq(&serde_json::to_string_pretty(&event).expect("serialize event"));
}

#[test]
fn real_session_snapshot() {
    let events = load_fixture("5a769f2b-7e5f-4bec-bf2f-6d007cfd43bf.jsonl");
    assert_snapshot(
        "tests/fixtures/snapshots/kimi/real_session.json",
        &snapshot_events(&events),
    );
}
