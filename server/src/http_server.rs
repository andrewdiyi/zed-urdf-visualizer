use crate::analyzer::find_symbol_line_col;
use crate::state::{default_remote_port, is_ssh_session, write_session_info, SharedState};
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path as AxumPath, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures_util::{SinkExt, StreamExt};
use percent_encoding::percent_decode_str;
use rust_embed::RustEmbed;
use serde::Deserialize;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use tokio::net::TcpListener;
use tower_http::cors::CorsLayer;

#[derive(RustEmbed)]
#[folder = "dist/"]
struct WebAssets;

#[derive(Debug, Deserialize)]
struct OpenPreviewReq {
    pub file_path: Option<String>,
    #[serde(default)]
    pub browser: bool,
}

pub async fn start_http_server(state: Arc<SharedState>, preferred_port: u16) -> anyhow::Result<u16> {
    let settings = state.settings.read().await.clone();
    let is_ssh = is_ssh_session();
    let bind_host = if is_ssh && settings.remote_allow_lan {
        "0.0.0.0"
    } else {
        "127.0.0.1"
    };

    let listener = if preferred_port != 0 {
        TcpListener::bind((bind_host, preferred_port)).await?
    } else if is_ssh {
        // In SSH sessions, bind deterministic port 19876 (falling back to 19877..19885)
        // so `~/.ssh/config` LocalForward 19876 127.0.0.1:19876 works automatically!
        let base = if settings.remote_port != 0 {
            settings.remote_port
        } else {
            default_remote_port()
        };
        let mut bound = None;
        for candidate in base..base.saturating_add(10) {
            if let Ok(l) = TcpListener::bind((bind_host, candidate)).await {
                bound = Some(l);
                break;
            }
        }
        match bound {
            Some(l) => l,
            None => TcpListener::bind((bind_host, 0)).await?,
        }
    } else {
        TcpListener::bind(("127.0.0.1", 0)).await?
    };

    let port = listener.local_addr()?.port();
    *state.http_port.write().await = port;
    write_session_info(port);

    let app = Router::new()
        .route("/ws", get(ws_handler))
        .route("/api/state", get(get_state_handler))
        .route("/api/open-preview", post(open_preview_handler))
        .route("/pkg/:pkg/*rel", get(serve_package_asset))
        .route("/fs/*abs_path", get(serve_fs_asset))
        .fallback(get(serve_embedded_ui))
        .layer(CorsLayer::permissive())
        .with_state(state);

    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });

    Ok(port)
}

async fn get_state_handler(State(state): State<Arc<SharedState>>) -> Response {
    let payload = state.latest_payload.read().await.clone();
    Json(payload).into_response()
}

async fn open_preview_handler(
    State(state): State<Arc<SharedState>>,
    Json(req): Json<OpenPreviewReq>,
) -> Response {
    if let Some(fp) = req.file_path {
        let pb = PathBuf::from(&fp);
        if pb.exists() {
            let existing = state.docs.read().await.get(&pb).map(|d| d.text.clone());
            let content = existing.or_else(|| std::fs::read_to_string(&pb).ok());
            if let Some(text) = content {
                let _ = state.update_document(pb, text).await;
            }
        }
    }

    let res = if req.browser {
        state.open_in_browser().await
    } else {
        state.spawn_native_window().await
    };

    match res {
        Ok(()) => Json(serde_json::json!({ "ok": true })).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "ok": false, "error": e.to_string() })),
        )
            .into_response(),
    }
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<SharedState>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_ws_client(socket, state))
}

async fn handle_ws_client(socket: WebSocket, state: Arc<SharedState>) {
    let (mut sender, mut receiver) = socket.split();
    let mut rx = state.ws_tx.subscribe();

    // Immediately push current URDF model state on connect
    if let Some(initial) = state.latest_payload.read().await.clone() {
        if let Ok(json) = serde_json::to_string(&initial) {
            let _ = sender.send(Message::Text(json)).await;
        }
    }

    let send_task = tokio::spawn(async move {
        while let Ok(msg) = rx.recv().await {
            if sender.send(Message::Text(msg)).await.is_err() {
                break;
            }
        }
    });

    while let Some(Ok(msg)) = receiver.next().await {
        if let Message::Text(txt) = msg {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&txt) {
                match val.get("type").and_then(|t| t.as_str()) {
                    Some("set_xacro_arg") => {
                        if let (Some(k), Some(v)) = (
                            val.get("name").and_then(|s| s.as_str()),
                            val.get("value").and_then(|s| s.as_str()),
                        ) {
                            state
                                .set_xacro_arg_and_reprocess(k.to_string(), v.to_string())
                                .await;
                        }
                    }
                    Some("jump_to_symbol") => {
                        if let (Some(fp), Some(kind), Some(name)) = (
                            val.get("filePath").and_then(|s| s.as_str()),
                            val.get("kind").and_then(|s| s.as_str()),
                            val.get("name").and_then(|s| s.as_str()),
                        ) {
                            let pb = PathBuf::from(fp);
                            if let Some((line, col)) = find_symbol_line_col(&pb, kind, name) {
                                open_in_zed_editor(&pb, line, col);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    send_task.abort();
}

fn open_in_zed_editor(file: &PathBuf, line: u32, col: u32) {
    let target = format!("{}:{line}:{col}", file.display());
    if Command::new("zed").arg(&target).spawn().is_ok() {
        return;
    }
    #[cfg(target_os = "macos")]
    {
        let mac_cli = "/Applications/Zed.app/Contents/MacOS/cli";
        if std::path::Path::new(mac_cli).exists() {
            let _ = Command::new(mac_cli).arg(&target).spawn();
        }
    }
}

async fn serve_package_asset(
    AxumPath((pkg, rel)): AxumPath<(String, String)>,
    State(state): State<Arc<SharedState>>,
) -> Response {
    let pkg_decoded = percent_decode_str(&pkg).decode_utf8_lossy().to_string();
    let rel_decoded = percent_decode_str(&rel).decode_utf8_lossy().to_string();
    let registry = state.registry.read().await;
    let Some(pkg_root) = registry.resolve_package(&pkg_decoded) else {
        return (
            StatusCode::NOT_FOUND,
            format!("ROS package '{pkg_decoded}' not found"),
        )
            .into_response();
    };
    let full_path = pkg_root.join(rel_decoded.trim_start_matches('/'));
    if !registry.is_path_allowed(&full_path) {
        return (StatusCode::FORBIDDEN, "Access outside package root denied").into_response();
    }
    serve_disk_file(&full_path).await
}

async fn serve_fs_asset(
    AxumPath(abs_path): AxumPath<String>,
    State(state): State<Arc<SharedState>>,
) -> Response {
    let decoded = percent_decode_str(&abs_path).decode_utf8_lossy().to_string();
    let full_path = if cfg!(windows) {
        PathBuf::from(&decoded)
    } else {
        PathBuf::from(format!("/{}", decoded.trim_start_matches('/')))
    };
    let registry = state.registry.read().await;
    if !registry.is_path_allowed(&full_path) {
        return (StatusCode::FORBIDDEN, "Path outside workspace roots").into_response();
    }
    serve_disk_file(&full_path).await
}

async fn serve_disk_file(path: &PathBuf) -> Response {
    match tokio::fs::read(path).await {
        Ok(bytes) => {
            let mime = guess_mesh_or_asset_mime(path);
            ([(header::CONTENT_TYPE, mime)], bytes).into_response()
        }
        Err(_) => (StatusCode::NOT_FOUND, "Asset file not found").into_response(),
    }
}

fn guess_mesh_or_asset_mime(path: &PathBuf) -> String {
    match path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "stl" => "model/stl".to_string(),
        "dae" => "model/vnd.collada+xml".to_string(),
        "obj" => "model/obj".to_string(),
        "gltf" => "model/gltf+json".to_string(),
        "glb" => "model/gltf-binary".to_string(),
        _ => mime_guess::from_path(path)
            .first_or_octet_stream()
            .to_string(),
    }
}

async fn serve_embedded_ui(uri: axum::http::Uri) -> Response {
    let raw_path = uri.path().trim_start_matches('/');
    let target = if raw_path.is_empty() {
        "index.html"
    } else {
        raw_path
    };

    if let Some(content) = WebAssets::get(target) {
        let mime = mime_guess::from_path(target).first_or_octet_stream();
        return ([(header::CONTENT_TYPE, mime.as_ref())], content.data).into_response();
    }
    if let Some(index) = WebAssets::get("index.html") {
        return ([(header::CONTENT_TYPE, "text/html; charset=utf-8")], index.data).into_response();
    }
    (StatusCode::NOT_FOUND, "Embedded WebUI missing").into_response()
}
