use anyhow::Result;
use axum::{
    http::StatusCode,
    response::{Html, IntoResponse, Response},
    routing::{delete, get},
    Router,
};
use rust_embed::RustEmbed;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::{net::TcpListener, sync::RwLock};
use tower_http::cors::CorsLayer;

mod api;
mod websocket;

use api::*;
use websocket::*;

#[derive(RustEmbed)]
#[folder = "../frontend/out"]
struct FrontendAssets;

#[derive(Clone)]
pub struct AppState {
    pub tasks: Arc<RwLock<HashMap<String, Task>>>,
}

impl AppState {
    fn new() -> Result<Self> {
        let tasks = load_tasks_from_state()?;
        let task_count = tasks.len();
        if task_count > 0 {
            println!("Restored {} task(s) from previous state", task_count);
        }
        Ok(Self {
            tasks: Arc::new(RwLock::new(tasks)),
        })
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    println!("Starting agentdev UI server...");

    let state = AppState::new()?;

    let app = Router::new()
        // API routes
        .route("/api/worktrees", get(get_worktrees))
        .route("/api/worktrees/:worktree_id", get(get_worktree))
        .route("/api/tasks", get(get_tasks).post(create_task))
        .route("/api/tasks/:task_id", delete(delete_task))
        .route(
            "/api/tasks/:task_id/agents/:agent_id/diff",
            get(get_agent_diff),
        )
        // WebSocket routes
        .route(
            "/ws/tasks/:task_id/agents/:agent_id/attach",
            get(websocket_handler),
        )
        // Static file serving (fallback to index.html for SPA)
        .fallback(serve_frontend)
        .layer(CorsLayer::permissive())
        .with_state(state);

    let port = std::env::var("PORT")
        .unwrap_or_else(|_| "3000".to_string())
        .parse::<u16>()
        .unwrap_or(3000);

    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = TcpListener::bind(addr).await?;

    println!("🚀 AgentDev UI server running on http://localhost:{}", port);

    // Auto-open browser
    if let Err(e) = open_browser(port) {
        println!(
            "Failed to open browser: {}. Please manually visit http://localhost:{}",
            e, port
        );
    }

    axum::serve(listener, app).await?;

    Ok(())
}

async fn serve_frontend(uri: axum::http::Uri) -> impl IntoResponse {
    let path = uri.path().trim_start_matches('/');

    if path.is_empty() || !path.contains('.') {
        // Serve index.html for SPA routing
        serve_file("index.html")
    } else {
        // Serve specific file
        serve_file(path)
    }
}

fn serve_file(path: &str) -> Response {
    match FrontendAssets::get(path) {
        Some(content) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            let body = content.data;
            Response::builder()
                .header("content-type", mime.as_ref())
                .body(axum::body::Body::from(body))
                .unwrap()
        }
        None => {
            // Try to serve index.html as fallback for SPA
            match FrontendAssets::get("index.html") {
                Some(content) => {
                    Html(String::from_utf8_lossy(&content.data).to_string()).into_response()
                }
                None => (StatusCode::NOT_FOUND, "404 Not Found").into_response(),
            }
        }
    }
}

fn open_browser(port: u16) -> Result<()> {
    let url = format!("http://localhost:{}", port);

    #[cfg(target_os = "macos")]
    std::process::Command::new("open").arg(&url).spawn()?;

    #[cfg(target_os = "linux")]
    std::process::Command::new("xdg-open").arg(&url).spawn()?;

    #[cfg(target_os = "windows")]
    std::process::Command::new("cmd")
        .args(&["/c", "start", &url])
        .spawn()?;

    Ok(())
}
