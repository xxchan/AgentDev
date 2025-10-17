use axum::http::{StatusCode, Uri};
use axum::response::{IntoResponse, Response};

#[cfg(agentdev_ui_built)]
use axum::body::Body;
#[cfg(agentdev_ui_built)]
use rust_embed::{EmbeddedFile, RustEmbed};

#[cfg(agentdev_ui_built)]
#[derive(RustEmbed)]
#[folder = "$OUT_DIR/assets"]
struct Assets;

#[cfg(agentdev_ui_built)]
fn response_from_asset(path: &str, asset: EmbeddedFile) -> Response {
    let mime = mime_guess::from_path(path).first_or_octet_stream();
    Response::builder()
        .header("content-type", mime.as_ref())
        .body(Body::from(asset.data))
        .expect("embedded asset response")
}

pub(super) fn serve(uri: Uri) -> Response {
    #[cfg(agentdev_ui_built)]
    {
        let path = uri.path().trim_start_matches('/');
        let mut candidates: Vec<String> = Vec::new();
        let mut push_candidate = |value: String| {
            if value.is_empty() {
                return;
            }
            if !candidates.iter().any(|existing| existing == &value) {
                candidates.push(value);
            }
        };

        if path.is_empty() {
            push_candidate("index.html".to_string());
        } else {
            let without_trailing = path.trim_end_matches('/');
            if path.contains('.') {
                push_candidate(path.to_string());
                if without_trailing != path {
                    push_candidate(without_trailing.to_string());
                }
            } else if without_trailing.is_empty() {
                push_candidate("index.html".to_string());
            } else {
                push_candidate(format!("{}/index.html", without_trailing));
                push_candidate(without_trailing.to_string());
            }
        }

        for candidate in &candidates {
            if let Some(asset) = Assets::get(candidate) {
                return response_from_asset(candidate, asset);
            }
        }

        if let Some(asset) = Assets::get("index.html") {
            return response_from_asset("index.html", asset);
        }

        return (StatusCode::NOT_FOUND, "404 Not Found").into_response();
    }

    #[cfg(not(agentdev_ui_built))]
    {
        let _ = uri;
        let instructions = [
            "AgentDev UI assets were not embedded into this build.",
            "To ship a standalone binary, rebuild with the frontend bundle by running:",
            "",
            "    AGENTDEV_SKIP_UI_BUILD=0 cargo build --release",
            "",
            "During development you can skip embedding by setting AGENTDEV_SKIP_UI_BUILD=1",
            "and run `pnpm run dev` inside apps/frontend to serve assets separately.",
        ]
        .join("\n");
        (StatusCode::SERVICE_UNAVAILABLE, instructions).into_response()
    }
}
