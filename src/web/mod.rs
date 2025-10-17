use std::net::SocketAddr;

use anyhow::Result;
use axum::{
    Router,
    response::IntoResponse,
    routing::{get, post},
};
use tokio::net::TcpListener;
use tower_http::cors::CorsLayer;

pub mod api;
mod frontend;

use api::*;

/// Configuration options for launching the embedded UI server.
#[derive(Clone, Copy, Debug)]
pub struct ServerOptions {
    /// Override the port used for the HTTP server. When `None`, fall back to
    /// the `PORT` or `AGENTDEV_BACKEND_PORT` environment variables, then 3000.
    pub port: Option<u16>,
    /// Whether to attempt opening the default browser after the server starts.
    pub auto_open_browser: bool,
}

impl Default for ServerOptions {
    fn default() -> Self {
        Self {
            port: None,
            auto_open_browser: true,
        }
    }
}

impl ServerOptions {
    /// Construct options using environment defaults (PORT/AGENTDEV_BACKEND_PORT).
    pub fn from_env() -> Self {
        let port = std::env::var("PORT")
            .ok()
            .and_then(|value| value.parse::<u16>().ok())
            .or_else(|| {
                std::env::var("AGENTDEV_BACKEND_PORT")
                    .ok()
                    .and_then(|value| value.parse::<u16>().ok())
            });
        Self {
            port,
            ..Default::default()
        }
    }

    /// Return a copy of the options with the port overridden.
    pub fn with_port(mut self, port: u16) -> Self {
        self.port = Some(port);
        self
    }

    /// Return a copy of the options with the browser auto-open flag updated.
    pub fn with_auto_open(mut self, enabled: bool) -> Self {
        self.auto_open_browser = enabled;
        self
    }
}

/// Run the UI server using a Tokio runtime owned by the caller thread.
pub fn run_blocking(options: ServerOptions) -> Result<()> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    runtime.block_on(run_async(options))
}

async fn run_async(options: ServerOptions) -> Result<()> {
    println!("Starting agentdev UI server...");

    let app = build_router();

    let port = options
        .port
        .or_else(|| {
            std::env::var("PORT")
                .ok()
                .and_then(|value| value.parse::<u16>().ok())
        })
        .or_else(|| {
            std::env::var("AGENTDEV_BACKEND_PORT")
                .ok()
                .and_then(|value| value.parse::<u16>().ok())
        })
        .unwrap_or(3000);

    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = TcpListener::bind(addr).await?;

    println!("🚀 AgentDev UI server running on http://localhost:{port}");

    if options.auto_open_browser {
        if let Err(e) = open_browser(port) {
            println!("Failed to open browser: {e}. Please manually visit http://localhost:{port}");
        }
    }

    axum::serve(listener, app).await?;

    Ok(())
}

fn build_router() -> Router {
    Router::new()
        // API routes
        .route("/api/sessions", get(get_sessions))
        .route("/api/sessions/", get(get_sessions))
        .route("/api/worktrees", get(get_worktrees))
        .route("/api/worktrees/", get(get_worktrees))
        .route("/api/worktrees/:worktree_id", get(get_worktree))
        .route("/api/worktrees/:worktree_id/", get(get_worktree))
        .route(
            "/api/worktrees/:worktree_id/git",
            get(get_worktree_git_details),
        )
        .route(
            "/api/worktrees/:worktree_id/git/",
            get(get_worktree_git_details),
        )
        .route(
            "/api/worktrees/:worktree_id/processes",
            get(get_worktree_processes),
        )
        .route(
            "/api/worktrees/:worktree_id/processes/",
            get(get_worktree_processes),
        )
        .route(
            "/api/worktrees/:worktree_id/commands",
            post(post_worktree_command),
        )
        .route(
            "/api/worktrees/:worktree_id/commands/",
            post(post_worktree_command),
        )
        // Static file serving (fallback to index.html for SPA)
        .fallback(serve_frontend)
        .layer(CorsLayer::permissive())
}

async fn serve_frontend(uri: axum::http::Uri) -> impl IntoResponse {
    frontend::serve(uri)
}

fn open_browser(port: u16) -> Result<()> {
    let url = format!("http://localhost:{port}");

    #[cfg(target_os = "macos")]
    std::process::Command::new("open").arg(&url).spawn()?;

    #[cfg(target_os = "linux")]
    std::process::Command::new("xdg-open").arg(&url).spawn()?;

    #[cfg(target_os = "windows")]
    std::process::Command::new("cmd")
        .args(["/c", "start", &url])
        .spawn()?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::{Method, Request, StatusCode},
    };
    use tempfile::TempDir;
    use tower::ServiceExt;

    struct EnvGuard {
        key: &'static str,
        original: Option<std::ffi::OsString>,
    }

    impl EnvGuard {
        fn set<K>(key: &'static str, value: K) -> Self
        where
            K: AsRef<std::ffi::OsStr>,
        {
            let original = std::env::var_os(key);
            // SAFETY: Setting environment variables is process-global. Tests use this helper
            // to isolate environment-dependent paths and restore the previous value on drop.
            unsafe {
                std::env::set_var(key, value);
            }
            Self { key, original }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            if let Some(ref value) = self.original {
                unsafe {
                    std::env::set_var(self.key, value);
                }
            } else {
                unsafe {
                    std::env::remove_var(self.key);
                }
            }
        }
    }

    fn setup_test_env() -> (TempDir, EnvGuard, EnvGuard) {
        let temp = TempDir::new().expect("create temp dir");
        let home_guard = EnvGuard::set("HOME", temp.path());
        let config_dir = temp.path().join(".config/xlaude");
        if let Err(err) = std::fs::create_dir_all(&config_dir) {
            panic!("failed to create config dir for test: {err}");
        }
        let config_guard = EnvGuard::set("XLAUDE_CONFIG_DIR", &config_dir);
        (temp, home_guard, config_guard)
    }

    #[tokio::test]
    async fn worktrees_endpoint_accepts_trailing_slash() {
        let (_temp, _home_guard, _config_guard) = setup_test_env();
        let app = build_router();

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/api/worktrees")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("worktrees request without slash");
        assert_eq!(response.status(), StatusCode::OK);
        let content_type = response
            .headers()
            .get(axum::http::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default();
        assert!(
            content_type.starts_with("application/json"),
            "expected JSON content-type, got {content_type}"
        );

        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/api/worktrees/")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("worktrees request with trailing slash");
        assert_eq!(response.status(), StatusCode::OK);
        let content_type = response
            .headers()
            .get(axum::http::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default();
        assert!(
            content_type.starts_with("application/json"),
            "expected JSON content-type, got {content_type}"
        );
    }

    #[tokio::test]
    async fn worktree_detail_trailing_slash_returns_not_found_json() {
        let (_temp, _home_guard, _config_guard) = setup_test_env();
        let response = build_router()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/api/worktrees/nonexistent/")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("worktree detail request with trailing slash");

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let content_type = response
            .headers()
            .get(axum::http::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default();
        assert_eq!(
            content_type, "text/plain; charset=utf-8",
            "expected plain text error response, got {content_type}"
        );
    }
}
