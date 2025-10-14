use anyhow::Result;
use axum::{
    http::StatusCode,
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    Router,
};
use rust_embed::RustEmbed;
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tower_http::cors::CorsLayer;

mod api;

use api::*;

#[derive(RustEmbed)]
#[folder = "../frontend/out"]
struct FrontendAssets;

#[tokio::main]
async fn main() -> Result<()> {
    println!("Starting agentdev UI server...");

    let app = Router::new()
        // API routes
        .route("/api/worktrees", get(get_worktrees))
        .route("/api/worktrees/:worktree_id", get(get_worktree))
        .route(
            "/api/worktrees/:worktree_id/git",
            get(get_worktree_git_details),
        )
        .route(
            "/api/worktrees/:worktree_id/processes",
            get(get_worktree_processes),
        )
        .route(
            "/api/worktrees/:worktree_id/commands",
            post(post_worktree_command),
        )
        // Static file serving (fallback to index.html for SPA)
        .fallback(serve_frontend)
        .layer(CorsLayer::permissive());

    let port = std::env::var("PORT")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .or_else(|| {
            std::env::var("AGENTDEV_BACKEND_PORT")
                .ok()
                .and_then(|value| value.parse::<u16>().ok())
        })
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
