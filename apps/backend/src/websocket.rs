use axum::extract::ws::{Message, WebSocket};
use axum::{
    extract::{Path, State, WebSocketUpgrade},
    response::Response,
};
use futures_util::{SinkExt, StreamExt};
use tokio::time::{interval, Duration};

use crate::AppState;
use agentdev::tmux::TmuxManager;

/// WebSocket handler for attaching to tmux sessions
pub async fn websocket_handler(
    State(state): State<AppState>,
    Path((task_id, agent_id)): Path<(String, String)>,
    ws: WebSocketUpgrade,
) -> Response {
    println!(
        "WebSocket connection request for task {} agent {}",
        task_id, agent_id
    );

    ws.on_upgrade(move |socket| handle_websocket(socket, state, task_id, agent_id))
}

async fn handle_websocket(socket: WebSocket, state: AppState, task_id: String, agent_id: String) {
    println!(
        "WebSocket connected for task {} agent {}",
        task_id, agent_id
    );

    let agent_info = {
        let tasks_map = state.tasks.read().await;
        tasks_map
            .get(&task_id)
            .and_then(|task| task.agents.iter().find(|a| a.id == agent_id))
            .cloned()
    };

    let (mut sender, mut receiver) = socket.split();

    let Some(agent) = agent_info else {
        let _ = sender
            .send(Message::Text(
                "Error: Agent not found for this task".to_string(),
            ))
            .await;
        return;
    };

    let tmux_manager = TmuxManager::new();
    let mut session_full = agent.tmux_session.clone();

    if session_full.is_none() && tmux_manager.session_exists(&agent.name) {
        session_full = Some(tmux_manager.session_name(&agent.name));
    }

    let Some(session_name_full) = session_full else {
        let _ = sender
            .send(Message::Text(
                "Error: Agent tmux session not found".to_string(),
            ))
            .await;
        return;
    };

    // Extract session name (remove "agentdev_" prefix)
    let session_key = session_name_full
        .strip_prefix("agentdev_")
        .unwrap_or(&session_name_full)
        .to_string();

    // Check if session exists
    if !tmux_manager.session_exists(&session_key) {
        let _ = sender
            .send(Message::Text(
                "Error: Tmux session does not exist".to_string(),
            ))
            .await;
        return;
    }

    // Send initial connection message
    if let Err(e) = sender
        .send(Message::Text(format!(
            "Connected to tmux session: {}",
            session_key
        )))
        .await
    {
        println!("Failed to send initial message: {}", e);
        return;
    }

    // Use channel to communicate between capture task and main loop
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();

    // Spawn a task to capture tmux output periodically
    let session_name_capture = session_key.clone();
    let tmux_manager_capture = TmuxManager::new();
    let mut last_output = String::new();

    let capture_handle = tokio::spawn(async move {
        let mut interval = interval(Duration::from_millis(500)); // Capture every 500ms

        loop {
            interval.tick().await;

            // Capture tmux pane content
            match tmux_manager_capture.capture_pane(&session_name_capture, 1000) {
                Ok(output) => {
                    // Only send if output changed
                    if output != last_output {
                        if tx.send(format!("output:{}", output)).is_err() {
                            println!("Receiver dropped, stopping capture");
                            break;
                        }
                        last_output = output;
                    }
                }
                Err(e) => {
                    println!("Failed to capture tmux pane: {}", e);
                    // Session might have died, break the loop
                    break;
                }
            }
        }
    });

    // Handle incoming messages using tokio::select to handle both WebSocket and capture messages
    loop {
        tokio::select! {
            // Handle messages from tmux capture
            Some(output) = rx.recv() => {
                if let Err(e) = sender.send(Message::Text(output)).await {
                    println!("Failed to send tmux output: {}", e);
                    break;
                }
            }

            // Handle WebSocket messages (user input)
            msg = receiver.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        println!("Received input for session {}: {}", session_key, text);

                        // Handle different message types
                        if text.starts_with("input:") {
                            // User input to send to tmux
                            let input = &text[6..]; // Remove "input:" prefix
                            if let Err(e) = tmux_manager.send_text(&session_key, input) {
                                println!("Failed to send input to tmux: {}", e);
                                let _ = sender.send(Message::Text(format!("Error: {}", e))).await;
                            }
                        } else if text == "enter" {
                            // Send Enter key
                            if let Err(e) = tmux_manager.send_enter(&session_key) {
                                println!("Failed to send enter to tmux: {}", e);
                                let _ = sender.send(Message::Text(format!("Error: {}", e))).await;
                            }
                        } else if text == "clear" {
                            // Clear terminal
                            if let Err(e) = tmux_manager.send_text(&session_key, "clear") {
                                println!("Failed to clear tmux: {}", e);
                            } else if let Err(e) = tmux_manager.send_enter(&session_key) {
                                println!("Failed to send enter after clear: {}", e);
                            }
                        } else if text.starts_with("resize:") {
                            // Handle terminal resize (future enhancement)
                            println!("Resize request (not yet implemented): {}", text);
                        } else {
                            println!("Unknown message type: {}", text);
                        }
                    }
                    Some(Ok(Message::Binary(_))) => {
                        // Ignore binary messages
                    }
                    Some(Ok(Message::Close(_))) => {
                        println!("WebSocket closed for task {} agent {}", task_id, agent_id);
                        break;
                    }
                    Some(Err(e)) => {
                        println!("WebSocket error for task {} agent {}: {}", task_id, agent_id, e);
                        break;
                    }
                    None => {
                        println!("WebSocket stream ended for task {} agent {}", task_id, agent_id);
                        break;
                    }
                    _ => {}
                }
            }
        }
    }

    // Clean up capture task
    capture_handle.abort();
    println!(
        "WebSocket disconnected for task {} agent {}",
        task_id, agent_id
    );
}
