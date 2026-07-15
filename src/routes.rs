use crate::{rooms::handle_socket, state::*, web_assets::get_html_page};
use axum::{
    extract::{Path, Query, State, ws::WebSocketUpgrade},
    http::header,
    response::{Html, IntoResponse, Redirect},
};
use std::collections::HashMap;
use uuid::Uuid;
pub(crate) async fn new_room(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Redirect, (axum::http::StatusCode, &'static str)> {
    if let Some(ref allowed_url) = state.allowed_url {
        let host = headers
            .get("host")
            .and_then(|v| v.to_str().ok())
            .map(|h| h.split(':').next().unwrap_or(h));
        match host {
            Some(h) if h == allowed_url => {}
            _ => return Err((axum::http::StatusCode::FORBIDDEN, "Forbidden")),
        }
    }
    if let Some(ref required_pass) = state.room_creation_password {
        match params.get("password") {
            Some(p) if p == required_pass => {}
            _ => return Err((axum::http::StatusCode::UNAUTHORIZED, "Unauthorized")),
        }
    }

    let room_id = if let Some(custom_name) = params.get("name") {
        if custom_name.is_empty() {
            Uuid::new_v4().to_string()
        } else {
            // Validate custom room name: alphanumeric, hyphens, underscores only, max length
            let trimmed = custom_name.trim();
            if trimmed.len() > MAX_ROOM_ID_LEN
                || trimmed.is_empty()
                || !trimmed
                    .chars()
                    .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
            {
                return Err((
                    axum::http::StatusCode::BAD_REQUEST,
                    "Invalid room name: use only letters, numbers, hyphens, and underscores (max 64 characters)",
                ));
            }
            trimmed.to_string()
        }
    } else {
        Uuid::new_v4().to_string()
    };

    Ok(Redirect::to(&format!("/{}", room_id)))
}

pub(crate) async fn redirect_room_trailing_slash(Path(room_id): Path<String>) -> Redirect {
    Redirect::to(&format!("/{}", room_id))
}

pub(crate) async fn redirect_channel_trailing_slash(
    Path((room_id, channel_id)): Path<(String, String)>,
) -> Redirect {
    Redirect::to(&format!("/{}/{}", room_id, channel_id))
}

pub(crate) async fn redirect_new_trailing_slash() -> Redirect {
    Redirect::to("/new")
}

pub(crate) async fn redirect_ws_trailing_slash(
    Path((room_id, channel_id)): Path<(String, String)>,
) -> Redirect {
    Redirect::to(&format!("/ws/{}/{}", room_id, channel_id))
}

pub(crate) async fn index(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
) -> axum::response::Response {
    if let Some(ref allowed_url) = state.allowed_url {
        let host = headers
            .get("host")
            .and_then(|v| v.to_str().ok())
            .map(|h| h.split(':').next().unwrap_or(h));
        match host {
            Some(h) if h == allowed_url => {}
            _ => return (axum::http::StatusCode::FORBIDDEN, "Forbidden").into_response(),
        }
    }

    let html = get_html_page();

    let csp = "default-src 'self'; script-src 'self' 'unsafe-inline' 'wasm-unsafe-eval'; script-src-elem 'self' 'unsafe-inline'; worker-src 'self' blob:; style-src 'self' 'unsafe-inline'; font-src 'self'; img-src 'self' data: https: blob:; connect-src 'self' wss: ws:; media-src 'self' blob:; object-src 'none'; frame-ancestors 'none';".to_string();

    (
        [(
            header::CONTENT_SECURITY_POLICY,
            axum::http::HeaderValue::from_str(&csp).unwrap(),
        )],
        Html(html),
    )
        .into_response()
}

pub(crate) async fn ws_handler(
    Path((room_id, channel_id)): Path<(String, String)>,
    Query(_): Query<HashMap<String, String>>,
    ws: WebSocketUpgrade,
    headers: axum::http::HeaderMap,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let mut channel_id = channel_id
        .chars()
        .take(MAX_CHANNEL_ID_LEN)
        .collect::<String>();
    if channel_id.eq_ignore_ascii_case("general") {
        channel_id = "General".to_string();
    }
    if room_id.len() > MAX_ROOM_ID_LEN {
        return (axum::http::StatusCode::BAD_REQUEST, "Room ID too long").into_response();
    }
    if let Some(ref allowed_url) = state.allowed_url {
        let host = headers
            .get("host")
            .and_then(|v| v.to_str().ok())
            .map(|h| h.split(':').next().unwrap_or(h));
        match host {
            Some(h) if h == allowed_url => {}
            _ => return (axum::http::StatusCode::FORBIDDEN, "Forbidden").into_response(),
        }
    }
    if let (Some(origin), Some(host)) = (headers.get("origin"), headers.get("host")) {
        if let (Ok(origin_str), Ok(host_str)) = (origin.to_str(), host.to_str()) {
            // Prevent bypass: "evil-example.com" must not match "example.com"
            let origin_host = origin_str
                .strip_prefix("https://")
                .or_else(|| origin_str.strip_prefix("http://"))
                .unwrap_or(origin_str)
                .split('/')
                .next()
                .unwrap_or(origin_str)
                .split(':')
                .next()
                .unwrap_or(origin_str);
            let host_base = host_str.split(':').next().unwrap_or(host_str);
            if origin_host != host_base && !origin_host.ends_with(&format!(".{}", host_base)) {
                return (axum::http::StatusCode::FORBIDDEN, "Forbidden Origin").into_response();
            }
        }
    }

    let mut client_ip = String::new();
    if let Some(real_ip) = headers.get("X-Real-IP") {
        client_ip = real_ip.to_str().unwrap_or("").to_string();
    } else if let Some(forwarded_for) = headers.get("X-Forwarded-For") {
        client_ip = forwarded_for
            .to_str()
            .unwrap_or("")
            .split(',')
            .next()
            .unwrap_or("")
            .trim()
            .to_string();
    }

    ws.max_message_size(32 * 1024 * 1024)
        .on_upgrade(move |socket| handle_socket(socket, room_id, channel_id, state, client_ip))
}
