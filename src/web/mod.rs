use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

use anyhow::Result;
use axum::{
    Router,
    response::IntoResponse,
    routing::{get, post},
};
use tokio::net::TcpListener;
use tower::{ServiceBuilder, make::Shared};
use tower_http::{cors::CorsLayer, normalize_path::NormalizePathLayer};

pub mod api;
mod frontend;

use api::*;

/// Configuration options for launching the embedded UI server.
#[derive(Clone, Copy, Debug)]
pub struct ServerOptions {
    /// Override the port used for the HTTP server. When `None`, fall back to
    /// the `PORT` or `AGENTDEV_BACKEND_PORT` environment variables, then 3000.
    pub port: Option<u16>,
    /// Override the host interface the HTTP server binds to. When `None`, fall
    /// back to `AGENTDEV_BACKEND_HOST`, then `HOST`, and finally 127.0.0.1.
    pub host: Option<IpAddr>,
    /// Whether to attempt opening the default browser after the server starts.
    pub auto_open_browser: bool,
}

impl Default for ServerOptions {
    fn default() -> Self {
        Self {
            port: None,
            host: None,
            auto_open_browser: false,
        }
    }
}

impl ServerOptions {
    /// Construct options using environment defaults (PORT/AGENTDEV_BACKEND_PORT,
    /// AGENTDEV_BACKEND_HOST/HOST).
    pub fn from_env() -> Self {
        let mut options = Self::default();
        options.port = std::env::var("PORT")
            .ok()
            .and_then(|value| value.parse::<u16>().ok())
            .or_else(|| {
                std::env::var("AGENTDEV_BACKEND_PORT")
                    .ok()
                    .and_then(|value| value.parse::<u16>().ok())
            });
        options.host = host_from_env();
        if let Ok(value) = std::env::var("AGENTDEV_AUTO_OPEN_BROWSER") {
            let normalized = value.trim().to_ascii_lowercase();
            options.auto_open_browser =
                matches!(normalized.as_str(), "1" | "true" | "yes" | "y" | "on");
        }
        options
    }

    /// Return a copy of the options with the port overridden.
    pub fn with_port(mut self, port: u16) -> Self {
        self.port = Some(port);
        self
    }

    /// Return a copy of the options with the host overridden.
    pub fn with_host(mut self, host: IpAddr) -> Self {
        self.host = Some(host);
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

    let router = build_router();
    let service = ServiceBuilder::new()
        .layer(NormalizePathLayer::trim_trailing_slash())
        .service(router);
    let app = Shared::new(service);

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
    let host = options
        .host
        .or_else(host_from_env)
        .unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST));
    let addr = SocketAddr::from((host, port));
    let listener = TcpListener::bind(addr).await?;

    println!(
        "🚀 AgentDev UI server running on http://{}:{port}",
        format_host_for_display(host)
    );

    if options.auto_open_browser {
        if let Err(e) = open_browser(host, port) {
            println!(
                "Failed to open browser: {e}. Please manually visit http://{}:{port}",
                format_host_for_display(host)
            );
        }
    }

    axum::serve(listener, app).await?;

    Ok(())
}

fn build_router() -> Router {
    Router::new()
        // API routes
        .route(
            "/api/sessions/:provider/:session_id",
            get(get_session_detail),
        )
        .route("/api/sessions", get(get_sessions))
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
        .route(
            "/api/worktrees/:worktree_id/merge",
            post(post_worktree_merge),
        )
        .route(
            "/api/worktrees/:worktree_id/delete",
            post(post_worktree_delete),
        )
        // Static file serving (fallback to index.html for SPA)
        .fallback(serve_frontend)
        .layer(CorsLayer::permissive())
}

async fn serve_frontend(uri: axum::http::Uri) -> impl IntoResponse {
    frontend::serve(uri)
}

fn host_from_env() -> Option<IpAddr> {
    parse_env_ip("AGENTDEV_BACKEND_HOST").or_else(|| parse_env_ip("HOST"))
}

fn parse_env_ip(key: &str) -> Option<IpAddr> {
    std::env::var(key)
        .ok()
        .and_then(|value| value.parse::<IpAddr>().ok())
}

fn format_host_for_display(host: IpAddr) -> String {
    match host {
        IpAddr::V6(addr) => format!("[{addr}]"),
        IpAddr::V4(addr) => addr.to_string(),
    }
}

fn format_host_for_browser(host: IpAddr) -> String {
    match host {
        IpAddr::V4(addr) if addr.is_unspecified() => Ipv4Addr::LOCALHOST.to_string(),
        IpAddr::V6(addr) if addr.is_unspecified() => format!("[{}]", Ipv6Addr::LOCALHOST),
        IpAddr::V6(addr) => format!("[{addr}]"),
        IpAddr::V4(addr) => addr.to_string(),
    }
}

fn open_browser(host: IpAddr, port: u16) -> Result<()> {
    let url_host = format_host_for_browser(host);
    let url = format!("http://{url_host}:{port}");

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
        let app = ServiceBuilder::new()
            .layer(NormalizePathLayer::trim_trailing_slash())
            .service(build_router());

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
            .clone()
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
        let app = ServiceBuilder::new()
            .layer(NormalizePathLayer::trim_trailing_slash())
            .service(build_router());
        let response = app
            .clone()
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

    #[tokio::test]
    async fn normalize_layer_handles_trailing_slash_on_simple_route() {
        async fn handler() -> &'static str {
            "ok"
        }

        let router = Router::new()
            .route("/foo", axum::routing::get(handler))
            .fallback(|| async { (StatusCode::NOT_FOUND, "missing") })
            .layer(CorsLayer::permissive());

        let app = ServiceBuilder::new()
            .layer(NormalizePathLayer::trim_trailing_slash())
            .service(router);

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/foo/")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("normalized route request");

        assert_eq!(response.status(), StatusCode::OK);
    }
}
