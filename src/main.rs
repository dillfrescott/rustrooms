use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, State, Query,
    },
    http::header,
    response::{Html, IntoResponse, Redirect},
    routing::get,
    Router,
};
use futures::{sink::SinkExt, stream::StreamExt};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};
use tokio::sync::Mutex;
use uuid::Uuid;
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message as WsMessage};
use sha1::{Sha1, Digest};
use rand::Rng;

async fn rnnoise_js() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "application/javascript")],
        include_str!("rnnoise.js"),
    )
}

async fn rnnoise_processor_js() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "application/javascript")],
        include_str!("rnnoise_processor.js"),
    )
}

async fn manifest_json() -> impl IntoResponse {
    let manifest = r##"{
    "name": "RustRooms",
    "short_name": "RustRooms",
    "start_url": "/",
    "scope": "/",
    "display": "standalone",
    "background_color": "#09090b",
    "theme_color": "#09090b",
    "description": "Simple, secure, and fast video conferencing.",
    "icons": [
        {
            "src": "/icon.svg",
            "sizes": "any",
            "type": "image/svg+xml",
            "purpose": "any maskable"
        }
    ]
}"##;
    (
        [(header::CONTENT_TYPE, "application/manifest+json")],
        manifest,
    )
}

async fn service_worker_js() -> impl IntoResponse {
    let sw = r##"
const CACHE_NAME = 'rustrooms-v1';
const ASSETS = [
    '/icon.svg',
    '/rnnoise.js',
    '/rnnoise_processor.js',
    '/assets/tailwind.js',
    '/assets/croppie.min.js',
    '/assets/croppie.min.css',
    '/assets/inter.css',
    '/fonts/inter-cyrillic-ext.woff2',
    '/fonts/inter-cyrillic.woff2',
    '/fonts/inter-greek-ext.woff2',
    '/fonts/inter-greek.woff2',
    '/fonts/inter-vietnamese.woff2',
    '/fonts/inter-latin-ext.woff2',
    '/fonts/inter-latin.woff2'
];

self.addEventListener('install', (event) => {
    event.waitUntil(
        caches.open(CACHE_NAME).then((cache) => cache.addAll(ASSETS))
    );
});

self.addEventListener('fetch', (event) => {
    if (event.request.method !== 'GET') return;

    event.respondWith(
        (async () => {
            try {
                const networkResponse = await fetch(event.request);
                return networkResponse;
            } catch (error) {
                const cachedResponse = await caches.match(event.request);
                if (cachedResponse) {
                    return cachedResponse;
                }
                throw error;
            }
        })()
    );
});
"##;
    (
        [(header::CONTENT_TYPE, "application/javascript")],
        sw,
    )
}

async fn icon_svg() -> impl IntoResponse {
    let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512">
    <rect width="512" height="512" rx="128" ry="128" fill="#09090b"/>
    <circle cx="256" cy="256" r="180" fill="#6366f1" fill-opacity="0.15"/>
    <circle cx="256" cy="256" r="140" fill="#6366f1" fill-opacity="0.3"/>
    <circle cx="256" cy="256" r="100" fill="#6366f1"/>
    <path d="M256 196a60 60 0 1 0 0 120 60 60 0 0 0 0-120z" fill="#ffffff"/>
    <path d="M196 256a60 60 0 0 1 120 0" stroke="#ffffff" stroke-width="20" stroke-linecap="round"/>
</svg>"##;
    (
        [(header::CONTENT_TYPE, "image/svg+xml")],
        svg,
    )
}

macro_rules! asset_route {
    ($func:ident, $content_type:expr, $path:expr, str) => {
        async fn $func() -> impl IntoResponse {
            (
                [(header::CONTENT_TYPE, $content_type)],
                include_str!($path),
            )
        }
    };
    ($func:ident, $content_type:expr, $path:expr, bytes) => {
        async fn $func() -> impl IntoResponse {
            (
                [(header::CONTENT_TYPE, $content_type)],
                include_bytes!($path).as_slice(),
            )
        }
    };
}

asset_route!(tailwind_js, "application/javascript", "assets/tailwind.js", str);
asset_route!(croppie_js, "application/javascript", "assets/croppie.min.js", str);
asset_route!(croppie_css, "text/css", "assets/croppie.min.css", str);
asset_route!(inter_css, "text/css", "assets/inter.css", str);
asset_route!(inter_cyrillic_ext_woff2, "font/woff2", "assets/fonts/inter-cyrillic-ext.woff2", bytes);
asset_route!(inter_cyrillic_woff2, "font/woff2", "assets/fonts/inter-cyrillic.woff2", bytes);
asset_route!(inter_greek_ext_woff2, "font/woff2", "assets/fonts/inter-greek-ext.woff2", bytes);
asset_route!(inter_greek_woff2, "font/woff2", "assets/fonts/inter-greek.woff2", bytes);
asset_route!(inter_vietnamese_woff2, "font/woff2", "assets/fonts/inter-vietnamese.woff2", bytes);
asset_route!(inter_latin_ext_woff2, "font/woff2", "assets/fonts/inter-latin-ext.woff2", bytes);
asset_route!(inter_latin_woff2, "font/woff2", "assets/fonts/inter-latin.woff2", bytes);

fn get_html_page(turn_url: &str, turn_username: &str, turn_credential: &str) -> String {

    let html = r###"
<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0, maximum-scale=1.0, user-scalable=no, viewport-fit=cover">
    <title>Rust Rooms</title>
    <link rel="manifest" href="/manifest.json">
    <link rel="icon" type="image/svg+xml" href="/icon.svg">
    <meta name="theme-color" content="#09090b">
    <script src="/assets/tailwind.js"></script>
    <link href="/assets/inter.css" rel="stylesheet">
    <style>
        :root {

            --bg-primary: #000000;
            --bg-secondary: #050505;
            --bg-tertiary: #0a0a0a;
            --bg-elevated: #111111;
            --bg-elevated-strong: #161616;
            --border-subtle: #222222;
            --border-medium: #333333;
            --border-strong: #444444;

            --text-primary: #fafafa;
            --text-secondary: #a1a1aa;
            --text-muted: #71717a;

            --accent: #3b82f6;
            --accent-hover: #2563eb;
            --accent-glow: rgba(59, 130, 246, 0.25);
            --accent-blue: #3b82f6;
            --accent-dark-blue: #1d4ed8;

            --accent-green: #10b981;
            --accent-red: #ef4444;
            --accent-dark-red: #dc2626;
            --accent-yellow: #f59e0b;

            --success: #10b981;
            --success-glow: rgba(16, 185, 129, 0.2);
            --danger: #ef4444;
            --danger-glow: rgba(239, 68, 68, 0.2);
            --warning: #f59e0b;
            --warning-glow: rgba(245, 158, 11, 0.2);
        }

        html {
            height: 100%;
            overflow: hidden;
            overscroll-behavior: none;
        }

        body {
            background-color: var(--bg-primary);
            color: var(--text-primary);
            font-family: 'Inter', -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
            overflow: hidden;
            position: fixed;
            inset: 0;
            width: 100%;
            height: 100dvh;
            touch-action: pan-x pan-y;
        }

        * {
            -webkit-font-smoothing: antialiased;
            -moz-osx-font-smoothing: grayscale;
        }

        img, video, canvas {
            filter: none;
        }

        ::-webkit-scrollbar { width: 6px; }
        ::-webkit-scrollbar-track { background: transparent; }
        ::-webkit-scrollbar-thumb { background: var(--border-medium); border-radius: 3px; }
        ::-webkit-scrollbar-thumb:hover { background: rgba(255, 255, 255, 0.2); }

        .glass-panel {
            background: var(--bg-elevated);
            backdrop-filter: blur(32px) saturate(180%) brightness(115%);
            -webkit-backdrop-filter: blur(32px) saturate(180%) brightness(115%);
            border: 1px solid var(--border-subtle);
        }

        .video-container {
            position: relative;
            background: var(--bg-secondary);
            border-radius: 12px;
            overflow: hidden;
            border: 1px solid var(--border-subtle);
            transition: all 0.3s cubic-bezier(0.4, 0, 0.2, 1);
            display: flex;
            flex-direction: column;
            width: 100%;
            height: 100%;
        }

        .video-container::before {
            display: none;
        }

        .video-container:hover {
            border-color: var(--border-medium);
        }

        .video-container video {
            width: 100%;
            height: 100%;
            object-fit: contain;
            background: transparent;
        }

        .grid-expand {
            grid-auto-rows: minmax(0, 1fr);
        }

        .avatar-layer {
            position: absolute;
            inset: 0;
            display: flex;
            align-items: center;
            justify-content: center;
            background: var(--bg-secondary);
            z-index: 10;
        }

        .avatar-img {
            position: absolute;
            inset: 0;
            width: 100%;
            height: 100%;
            object-fit: cover;
            filter: blur(24px);
            opacity: 0.3;
            pointer-events: none;
            -webkit-user-drag: none;
            user-drag: none;
        }

        .avatar-center {
            position: relative;
            width: 108px;
            height: 108px;
            border-radius: 12px;
            overflow: hidden;
            border: 1px solid var(--border-medium);
            background: var(--bg-tertiary);
        }

        .avatar-center img {
            -webkit-user-drag: none;
            user-drag: none;
            pointer-events: none;
        }

        .video-container img {
            -webkit-user-drag: none;
            user-drag: none;
        }

        .avatar-layer {
            display: flex;
            align-items: center;
            justify-content: center;
            z-index: 20;
        }

        @media (min-width: 768px) {
            .avatar-center {
                width: 144px;
                height: 144px;
                border-width: 2px;
            }
        }

        .avatar-center img {
            width: 100%;
            height: 100%;
            object-fit: cover;
        }

        video.active + .avatar-layer {
            display: none !important;
        }

        .control-btn {
            padding: 0;
            border-radius: 10px;
            border: none;
            cursor: pointer;
            display: flex;
            align-items: center;
            justify-content: center;
            transition: all 0.2s cubic-bezier(0.4, 0, 0.2, 1);
            background: var(--bg-elevated);
            backdrop-filter: blur(20px) saturate(160%);
            -webkit-backdrop-filter: blur(20px) saturate(160%);
            color: var(--text-primary);
            width: 54px;
            height: 54px;
            border: 1px solid var(--border-subtle);
            overflow: hidden;
        }

        @media (min-width: 768px) {
            .control-btn {
                width: 58px;
                height: 58px;
                border-radius: 11px;
            }
        }

        .control-btn::before {
            content: '';
            position: absolute;
            inset: 0;
            background: rgba(255, 255, 255, 0.05);
            opacity: 0;
            transition: opacity 0.15s ease;
        }

        .control-btn:hover::before {
            opacity: 1;
        }

        .control-btn:hover {
            background: var(--bg-tertiary);
            border-color: var(--border-medium);
        }

        .control-btn:active {
            transform: scale(0.97);
            transition: transform 0.1s ease;
        }

        .control-btn.active-red {
            background: var(--danger);
            border-color: var(--danger);
        }

        .control-btn.active-red:hover {
            background: #dc2626;
            border-color: #dc2626;
        }

        .control-btn.active-green {
            background: var(--success);
            border-color: var(--success);
        }

        .control-btn.active-green:hover {
            background: #16a34a;
            border-color: #16a34a;
        }

        .control-btn:disabled {
            opacity: 0.5;
            cursor: not-allowed;
            pointer-events: none;
            -webkit-pointer-events: none;
        }

        .control-btn:disabled:hover {
            background: var(--bg-elevated);
            border-color: var(--border-subtle);
            transform: none;
        }

        .control-btn:disabled:hover::before {
            opacity: 0;
        }

        .control-btn:disabled:active {
            transform: none !important;
            background: var(--bg-elevated) !important;
        }

        @keyframes spin {
            to { transform: rotate(360deg); }
        }

        .spinner {
            animation: spin 1s linear infinite;
        }

        .pip-wrapper {
            position: fixed;
            bottom: 220px;
            right: 16px;
            cursor: grab;
            touch-action: none;
            width: 140px;
            aspect-ratio: 16/9;
            border-radius: 10px;
            border: 1px solid var(--border-subtle);
            overflow: hidden;
            z-index: 75;
            transition: all 0.2s cubic-bezier(0.4, 0, 0.2, 1);
            background: var(--bg-secondary);
        }

        @media (min-width: 768px) {
            .pip-wrapper {
                width: 280px;
                bottom: 240px;
                border-radius: 12px;
            }
        }

        @media (max-width: 950px) and (orientation: landscape) {
            .pip-wrapper {
                width: 140px;
                bottom: 120px;
            }
        }

        .pip-wrapper:hover {
            border-color: var(--border-medium);
        }

        .connection-dot {
            width: 8px;
            height: 8px;
            background-color: var(--danger);
            border-radius: 50%;
            display: inline-block;
            transition: background-color 0.3s;
        }
        .connection-dot.connected {
            background-color: var(--success);
        }
        .connection-dot.connecting {
            background-color: var(--warning);
            animation: pulse 2s infinite;
        }

        @keyframes pulse {
            0%, 100% { opacity: 1; }
            50% { opacity: 0.8; }
        }

        .ping-container {
            display: flex;
            align-items: center;
            gap: 6px;
            font-size: 0.75rem;
            color: var(--text-muted);
        }

        .ping-bars {
            display: flex;
            align-items: flex-end;
            gap: 2px;
            height: 12px;
        }

        .ping-bar {
            width: 3px;
            background-color: var(--border-medium);
            border-radius: 1px;
            transition: background-color 0.3s, height 0.3s;
        }

        .ping-bar-1 { height: 4px; }
        .ping-bar-2 { height: 8px; }
        .ping-bar-3 { height: 12px; }

        .ping-good .ping-bar { background-color: var(--success); }
        .ping-fair .ping-bar-1, .ping-fair .ping-bar-2 { background-color: var(--warning); }
        .ping-poor .ping-bar-1 { background-color: var(--danger); }

        input[type=range] {
            -webkit-appearance: none;
            background: transparent;
        }
        input[type=range]::-webkit-slider-thumb {
            -webkit-appearance: none;
            height: 14px;
            width: 14px;
            border-radius: 50%;
            background: var(--text-primary);
            cursor: pointer;
            margin-top: -5px;
            transition: transform 0.15s;
        }
        input[type=range]::-webkit-slider-thumb:hover {
            transform: scale(1.15);
        }
        input[type=range]::-webkit-slider-runnable-track {
            width: 100%;
            height: 4px;
            cursor: pointer;
            background: rgba(255, 255, 255, 0.15);
            border-radius: 2px;
        }

        .volume-controls {
            position: absolute;
            bottom: 14px;
            right: 14px;
            background: var(--bg-elevated-strong);
            backdrop-filter: blur(24px) saturate(180%);
            -webkit-backdrop-filter: blur(24px) saturate(180%);
            padding: 12px 16px;
            border-radius: 10px;
            display: flex;
            flex-direction: column;
            gap: 10px;
            opacity: 0;
            transition: all 0.2s cubic-bezier(0.4, 0, 0.2, 1);
            align-items: flex-end;
            border: 1px solid var(--border-medium);
        }
        .video-container:hover .volume-controls {
            opacity: 1;
            transform: translateY(0);
        }

        .vol-row {
            display: flex;
            align-items: center;
            gap: 10px;
        }

        .vol-row button {
            padding: 4px;
            border-radius: 6px;
            transition: all 0.15s;
        }
        .vol-row button:hover {
            background: rgba(255, 255, 255, 0.1);
        }

        .speaking-glow {
            border: 4px solid #3b82f6 !important;
            box-shadow: 0 0 20px rgba(59, 130, 246, 0.6) !important;
            transition: border 0.2s ease-in-out, box-shadow 0.2s ease-in-out;
            z-index: 50;
        }

        #localPipWrapper.speaking-glow {
            border: 3px solid #3b82f6 !important;
            box-shadow: 0 0 10px rgba(59, 130, 246, 0.4) !important;
            z-index: 75;
        }

        .video-container {
            cursor: grab;
            touch-action: none;
        }

        .video-container:active {
            cursor: grabbing;
        }

        .video-container.is-dragging {
            position: fixed;
            z-index: 1000;
            cursor: grabbing;
            transform: scale(1.02) translate3d(0, 0, 0);
            pointer-events: none;
            opacity: 0.95;
            will-change: transform;
            transition: none;
            user-select: none;
            -webkit-user-select: none;
            outline: none;
        }

        .video-container.is-dragging * {
            user-select: none;
            -webkit-user-select: none;
            outline: none;
        }

        .video-container.drag-placeholder {
            opacity: 0.25;
            border: 2px dashed var(--accent);
            background: transparent;
            user-select: none;
            -webkit-user-select: none;
        }

        #remote-grid:has(.is-dragging) {
            user-select: none;
            -webkit-user-select: none;
        }

        #remote-grid:has(.is-dragging) .video-container {
            user-select: none;
            -webkit-user-select: none;
        }

        .video-container.is-shifting {
            transition: transform 0.25s cubic-bezier(0.2, 0, 0, 1);
            user-select: none;
            -webkit-user-select: none;
        }

        .video-container:fullscreen {
            border-radius: 0;
            background: #000;
            display: flex;
            align-items: center;
            justify-content: center;
        }

        .video-container:fullscreen video {
            max-height: 100vh;
            max-width: 100vw;
            height: 100%;
            width: 100%;
        }

        .video-container:fullscreen .volume-controls {
            bottom: 32px;
            right: 32px;
            transform: scale(1.15);
            transform-origin: bottom right;
            padding: 12px;
            gap: 8px;
        }

        .mic-meter {
            width: 100%;
            height: 4px;
            background: rgba(255, 255, 255, 0.1);
            border-radius: 2px;
            overflow: hidden;
            margin-top: 10px;
        }
        .mic-bar {
            height: 100%;
            width: 0%;
            background: var(--success);
            border-radius: 2px;
            transition: width 0.04s linear;
        }

        .taskbar {
            background: #000000;
            border-top: 1px solid rgba(255, 255, 255, 0.15);
            backdrop-filter: blur(32px) saturate(180%) brightness(115%);
            -webkit-backdrop-filter: blur(32px) saturate(180%) brightness(115%);
            padding-bottom: env(safe-area-inset-bottom);
        }

        @media (min-width: 768px) {
            .taskbar {
                padding-bottom: env(safe-area-inset-bottom);
            }
        }

        @media (max-width: 1024px) {
            #btnShare {
                display: none !important;
            }
        }
        @supports (-webkit-touch-callout: none) {
            #btnShare {
                display: none !important;
            }
        }

        input[type="text"],
        input[type="password"],
        select {
            background: var(--bg-tertiary);
            border: 1px solid var(--border-subtle);
            color: var(--text-primary);
            transition: all 0.2s cubic-bezier(0.4, 0, 0.2, 1);
            border-radius: 8px;
        }

        input[type="text"]:focus,
        input[type="password"]:focus,
        select:focus {
            outline: none;
            border-color: var(--accent);
            background: var(--bg-secondary);
        }

        input[type="text"]::placeholder,
        input[type="password"]::placeholder {
            color: var(--text-muted);
            opacity: 0.8;
        }

        .btn-primary {
            background: var(--accent);
            transition: all 0.2s cubic-bezier(0.4, 0, 0.2, 1);
            border-radius: 10px;
            box-shadow: 0 1px 3px rgba(0, 0, 0, 0.3);
        }
        .btn-primary:hover {
            background: var(--accent-hover);
            transform: translateY(-1px);
            box-shadow: 0 4px 12px rgba(59, 130, 246, 0.25);
        }

        .btn-primary:active {
            transform: translateY(0);
            transition: transform 0.1s ease;
        }

        .btn-secondary {
            background: var(--bg-elevated);
            border: 1px solid var(--border-subtle);
            backdrop-filter: blur(20px) saturate(160%);
            -webkit-backdrop-filter: blur(20px) saturate(160%);
            transition: all 0.2s cubic-bezier(0.4, 0, 0.2, 1);
            border-radius: 10px;
        }
        .btn-secondary:hover {
            background: var(--bg-elevated-strong);
            border-color: var(--border-medium);
        }

        .status-pill {
            background: var(--bg-elevated);
            border: 1px solid var(--border-subtle);
            backdrop-filter: blur(24px) saturate(160%);
            -webkit-backdrop-filter: blur(24px) saturate(160%);
            border-radius: 10px;
            transition: all 0.2s cubic-bezier(0.4, 0, 0.2, 1);
        }

        .status-pill:hover {
            background: var(--bg-elevated-strong);
            border-color: var(--border-medium);
        }

        .label-text {
            color: var(--text-secondary);
            font-size: 0.75rem;
            font-weight: 500;
            letter-spacing: 0.01em;
        }

        .empty-state-icon {
            color: var(--text-muted);
            opacity: 0.4;
        }

        .fadeIn {
            animation: fadeIn 0.3s ease-in-out;
        }

        @keyframes fadeOut {
            0% { opacity: 1; visibility: visible; }
            100% { opacity: 0; visibility: hidden; }
        }

        @keyframes fadeIn {
            0% { opacity: 0; }
            100% { opacity: 1; }
        }

        #particleCanvas {
            position: absolute;
            top: 0;
            left: 0;
            width: 100%;
            height: 100%;
            pointer-events: none;
            z-index: 1;
        }

        #roomSidebar {
            position: fixed;
            left: -320px;
            top: 0;
            bottom: 0;
            width: 320px;
            z-index: 100;
            transition: transform 0.4s cubic-bezier(0.4, 0, 0.2, 1);
            background: #000000;
            border-right: 1px solid var(--border-medium);
            display: flex;
            flex-direction: column;
        }

        #roomSidebar.open {
            transform: translateX(320px);
        }

        @media (min-width: 768px) {
            body.sidebar-open #appLayout {
                margin-left: 320px;
                width: calc(100% - 320px);
                transition: margin-left 0.4s cubic-bezier(0.4, 0, 0.2, 1), width 0.4s cubic-bezier(0.4, 0, 0.2, 1);
            }

            body.sidebar-open #sidebarOverlay {
                display: none;
            }
        }

        .sidebar-header {
            padding: 24px;
            border-bottom: 1px solid var(--border-subtle);
            display: flex;
            align-items: center;
            justify-content: space-between;
        }

        .sidebar-content {
            flex: 1;
            overflow-y: auto;
            padding: 16px;
        }

        .room-item {
            background: var(--bg-secondary);
            border: 1px solid var(--border-subtle);
            border-radius: 12px;
            padding: 16px;
            margin-bottom: 12px;
            transition: all 0.2s ease;
            cursor: pointer;
        }

        .room-item:hover {
            border-color: var(--accent);
            background: var(--bg-tertiary);
        }

        .room-item.active {
            border-color: var(--accent);
            background: rgba(255, 255, 255, 0.05);
        }

        .room-name {
            font-weight: 600;
            font-size: 0.95rem;
            color: var(--text-primary);
            margin-bottom: 8px;
            display: flex;
            align-items: center;
            justify-content: space-between;
        }

        .user-count {
            font-size: 0.75rem;
            color: var(--text-muted);
            background: var(--bg-primary);
            padding: 2px 8px;
            border-radius: 99px;
            border: 1px solid var(--border-subtle);
        }

        .room-users {
            display: flex;
            flex-wrap: wrap;
            gap: 6px;
        }

        .mini-avatar {
            width: 24px;
            height: 24px;
            border-radius: 6px;
            background: var(--bg-primary);
            border: 1px solid var(--border-subtle);
            overflow: hidden;
            display: flex;
            align-items: center;
            justify-content: center;
        }

        .mini-avatar img {
            width: 100%;
            height: 100%;
            object-fit: cover;
        }

        .mini-avatar-placeholder {
            font-size: 10px;
            font-weight: 600;
            color: var(--text-muted);
        }

        .mini-avatar.speaking-glow {
            border: 2px solid #3b82f6 !important;
            box-shadow: 0 0 6px rgba(59, 130, 246, 0.4) !important;
            transition: border 0.2s ease-in-out, box-shadow 0.2s ease-in-out;
        }

        .sidebar-overlay {
            position: fixed;
            inset: 0;
            background: rgba(0, 0, 0, 0.4);
            z-index: 90;
            opacity: 0;
            pointer-events: none;
            transition: opacity 0.3s ease;
        }

        .sidebar-overlay.open {
            opacity: 1;
            pointer-events: auto;
        }

        @media (min-width: 768px) {
            body.sidebar-open .sidebar-overlay {
                display: none !important;
            }
        }

        .modal-overlay {
            position: fixed;
            inset: 0;
            background: rgba(0, 0, 0, 0.75);
            backdrop-filter: blur(8px);
            z-index: 200;
            display: flex;
            align-items: center;
            justify-content: center;
            opacity: 0;
            pointer-events: none;
            transition: all 0.3s ease;
        }

        .modal-overlay.open {
            opacity: 1;
            pointer-events: auto;
        }

        .modal-content {
            background: var(--bg-elevated);
            border: 1px solid var(--border-medium);
            backdrop-filter: blur(24px) saturate(160%);
            -webkit-backdrop-filter: blur(24px) saturate(160%);
            border-radius: 24px;
            width: 90%;
            max-width: 400px;
            padding: 32px;
            transform: scale(0.96);
            transition: all 0.3s cubic-bezier(0.16, 1, 0.3, 1);
        }

        .modal-overlay.open .modal-content {
            transform: scale(1);
        }

        .room-user-row {
            display: flex;
            align-items: center;
            gap: 12px;
            padding: 8px;
            border-radius: 8px;
            transition: background 0.2s;
        }

        .room-user-row:hover {
            background: rgba(255, 255, 255, 0.05);
        }

        .room-user-name {
            font-size: 0.85rem;
            color: var(--text-secondary);
            font-weight: 500;
        }

        .status-indicators {
            display: flex;
            gap: 4px;
            margin-left: auto;
            align-items: center;
        }

        .status-icon {
            color: var(--text-muted);
            opacity: 0.7;
        }

        .status-icon.active {
            color: #ef4444;
            opacity: 1;
        }

        .idle-fullscreen {
            cursor: none !important;
        }

        .idle-fullscreen .volume-controls,
        .idle-fullscreen .name-tag {
            opacity: 0 !important;
            pointer-events: none !important;
            transition: opacity 0.5s ease-out;
        }

        .video-container:fullscreen .volume-controls,
        .video-container:fullscreen .name-tag {
            transition: opacity 0.2s ease-in;
        }
    </style>

    <link rel="stylesheet" href="/assets/croppie.min.css" />
    <script src="/assets/croppie.min.js"></script>
</head>
<body class="flex flex-col overflow-hidden" style="background-color: var(--bg-primary);">

    <div id="sidebarOverlay" class="sidebar-overlay" onclick="toggleSidebar()"></div>

    <div id="roomSidebar">
        <div class="sidebar-header">
            <h2 id="sidebarTitle" class="text-xl font-bold text-white">Channels</h2>
            <button id="btnCloseSidebar" onclick="toggleSidebar()" class="text-zinc-400 hover:text-white transition-colors">
                <svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="18" y1="6" x2="6" y2="18"></line><line x1="6" y1="6" x2="18" y2="18"></line></svg>
            </button>
        </div>
        <div class="sidebar-content">
            <div id="sidebarActions">
                <button onclick="createNewChannel()" class="w-full btn-primary py-3 mb-6 flex items-center justify-center gap-2 font-semibold">
                    <svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="12" y1="5" x2="12" y2="19"></line><line x1="5" y1="12" x2="19" y2="12"></line></svg>
                    Create New Channel
                </button>
            </div>
            <div id="roomListContainer">

            </div>
        </div>
        </div>
    </div>

    <div id="nameModal" class="modal-overlay">
        <div class="modal-content text-center space-y-6">
            <h3 id="modalTitle" class="text-2xl font-bold text-white">Name Channel</h3>
            <div class="space-y-4">
                <input type="text" id="modalInput" placeholder="Enter name..." class="w-full rounded-xl px-4 py-3 text-white transition-all bg-[var(--bg-tertiary)] border border-[var(--border-subtle)] focus:border-[var(--accent)] outline-none" maxlength="32">
                <div class="flex gap-3">
                    <button onclick="closeNameModal()" class="btn-secondary flex-1 py-3 text-white rounded-xl font-medium transition-all">Cancel</button>
                    <button id="modalSubmit" class="btn-primary flex-1 py-3 text-white rounded-xl font-medium transition-all">Confirm</button>
                </div>
            </div>
        </div>
    </div>

    <div id="passwordModal" class="modal-overlay">
        <div class="modal-content text-center space-y-6">
            <h3 id="passwordModalTitle" class="text-2xl font-bold text-white">Password Required</h3>
            <p id="passwordModalMessage" class="text-zinc-300"></p>
            <div class="space-y-4">
                <input type="password" id="passwordModalInput" placeholder="Enter password..." class="w-full rounded-xl px-4 py-3 text-white transition-all bg-[var(--bg-tertiary)] border border-[var(--border-subtle)] focus:border-[var(--accent)] outline-none" onkeydown="if(event.key==='Enter') document.getElementById('passwordModalSubmit').click()">
                <div class="flex gap-3">
                    <button onclick="closePasswordModal()" class="btn-secondary flex-1 py-3 text-white rounded-xl font-medium transition-all">Cancel</button>
                    <button id="passwordModalSubmit" class="btn-primary flex-1 py-3 text-white rounded-xl font-medium transition-all">Confirm</button>
                </div>
            </div>
        </div>
    </div>

    <div id="alertModal" class="modal-overlay">
        <div class="modal-content text-center space-y-6">
            <h3 id="alertTitle" class="text-2xl font-bold text-white">Alert</h3>
            <p id="alertMessage" class="text-zinc-300"></p>
            <button onclick="closeCustomAlert()" class="btn-primary w-full py-3 text-white rounded-xl font-medium transition-all">OK</button>
        </div>
    </div>

    <div id="confirmModal" class="modal-overlay">
        <div class="modal-content text-center space-y-6">
            <h3 id="confirmTitle" class="text-2xl font-bold text-white">Confirm</h3>
            <p id="confirmMessage" class="text-zinc-300"></p>
            <div class="flex gap-3">
                <button onclick="closeCustomConfirm()" class="btn-secondary flex-1 py-3 text-white rounded-xl font-medium transition-all">Cancel</button>
                <button id="confirmSubmit" class="btn-primary flex-1 py-3 text-white rounded-xl font-medium transition-all">Confirm</button>
            </div>
        </div>
    </div>

    <div id="kickModal" class="modal-overlay">
        <div class="modal-content text-center space-y-6">
            <h3 id="kickTitle" class="text-2xl font-bold text-white">Kick User</h3>
            <p id="kickMessage" class="text-zinc-300"></p>
            <div class="flex gap-3">
                <button onclick="closeKickModal()" class="btn-secondary flex-1 py-3 text-white rounded-xl font-medium transition-all">Cancel</button>
                <button id="kickSubmit" class="btn-primary flex-1 py-3 text-white rounded-xl font-medium transition-all" style="background: var(--danger);">Kick</button>
            </div>
        </div>
    </div>

    <div id="welcomeOverlay" class="fixed inset-0 z-[70] flex flex-col items-center justify-center p-4" style="display: none; background: var(--bg-primary);">
        <canvas id="particleCanvas"></canvas>
        <div class="text-center space-y-8 max-w-md w-full relative z-10">
            <div class="space-y-3" id="welcomeTitleContainer">
                <h1 class="text-5xl md:text-6xl font-bold tracking-tight" style="color: #ffffff; text-shadow: 0 0 20px rgba(255, 255, 255, 0.5), 0 2px 8px rgba(255, 255, 255, 0.3), 0 8px 16px rgba(0, 0, 0, 0.5); font-weight: 800; letter-spacing: -0.02em;">Rust Rooms</h1>
                <p style="color: var(--text-secondary);" class="text-base md:text-lg font-normal">Simple, secure, and fast video conferencing.</p>
            </div>

            <div id="startActionContainer" class="relative min-h-[64px] flex justify-center items-center">
                 <button id="btnStartRoom" onclick="createRoom()" class="btn-primary absolute w-full md:w-auto px-10 py-4 text-white rounded-2xl font-semibold text-base transition-all">
                    Start Room
                </button>

                <div id="passwordInputContainer" class="absolute w-full max-w-xs transition-all duration-300 transform translate-y-4 opacity-0 pointer-events-none flex gap-2">
                     <input type="password" id="roomPasswordInput" placeholder="Password required" class="flex-1 rounded-xl px-4 py-3 text-white" onkeypress="if(event.key==='Enter') submitPassword()">
                     <button onclick="submitPassword()" class="btn-primary px-5 py-3 text-white rounded-xl font-medium transition-all">
                        <svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="20 6 9 17 4 12"/></svg>
                     </button>
                </div>
            </div>
        </div>
    </div>

    <div id="configOverlay" class="fixed inset-0 z-[60] flex flex-col items-center justify-center p-4 transition-opacity duration-300 hidden opacity-0" style="background: rgba(0, 0, 0, 0.95); backdrop-filter: blur(16px) saturate(120%);">
        <canvas id="particleCanvasConfig" class="absolute inset-0 pointer-events-none" style="z-index: 1;"></canvas>
        <div id="configPanel" class="glass-panel p-6 md:p-8 rounded-3xl max-w-5xl w-full max-h-[95vh] overflow-y-auto relative z-10">
            <div class="text-center space-y-1 mb-5">
                <h1 class="text-2xl md:text-3xl font-bold tracking-tight" style="color: var(--text-primary);">Setup</h1>
                <p style="color: var(--text-secondary);" class="text-sm font-normal">Configure your stream.</p>
            </div>

            <div class="flex flex-col lg:flex-row gap-6 lg:gap-8">

                <div class="lg:w-1/2 flex flex-col gap-4">
                    <div class="relative aspect-video rounded-2xl overflow-hidden flex-shrink-0" style="background: var(--bg-secondary); border: 1px solid var(--border-subtle);">
                        <video id="previewVideo" autoplay playsinline muted class="w-full h-full object-contain"></video>
                        <div class="absolute inset-0 flex items-center justify-center pointer-events-none" id="previewPlaceholder" style="color: var(--text-muted);">
                            <span>Camera Off</span>
                        </div>
                        <div class="absolute bottom-3 left-3 px-3 py-1.5 rounded-xl text-xs font-medium backdrop-blur-sm" style="background: rgba(0, 0, 0, 0.6); color: var(--text-primary);">
                            Preview
                        </div>
                    </div>

                    <div class="flex gap-3">
                        <button onclick="togglePreviewMic()" id="btnPreviewMic" disabled class="btn-secondary flex-1 py-3 text-white rounded-xl font-medium transition-all flex items-center justify-center gap-2 opacity-50 cursor-not-allowed">
                            Mute
                        </button>
                        <button onclick="togglePreviewCam()" id="btnPreviewCam" disabled class="btn-secondary flex-1 py-3 text-white rounded-xl font-medium transition-all flex items-center justify-center gap-2 opacity-50 cursor-not-allowed">
                            Stop Cam
                        </button>
                    </div>
                </div>

                <div class="lg:w-1/2 space-y-4">
                    <div class="flex flex-col sm:flex-row gap-4">
                        <div class="flex-shrink-0 flex justify-center sm:justify-start">
                            <div class="text-center">
                                <label class="label-text block mb-2">Avatar</label>
                                <div onclick="document.getElementById('avatarInput').click()" class="w-20 h-20 rounded-2xl cursor-pointer overflow-hidden flex items-center justify-center transition-all group relative mx-auto" style="background: var(--bg-secondary); border: 2px solid var(--border-subtle);">
                                    <img id="avatarPreview" src="" class="hidden w-full h-full object-cover" draggable="false">
                                    <span id="avatarPlaceholder" class="text-3xl" style="color: var(--text-muted);">👤</span>
                                    <div class="absolute inset-0 flex items-center justify-center opacity-0 group-hover:opacity-100 transition-opacity text-xs font-semibold" style="background: rgba(0, 0, 0, 0.7); color: var(--text-primary);">Edit</div>
                                </div>
                                <input type="file" id="avatarInput" hidden accept="image/*" onchange="handleAvatarUpload(this)">
                            </div>
                        </div>

                        <div class="flex-1">
                            <label class="label-text block mb-2">Nickname</label>
                            <input type="text" id="nicknameInput" placeholder="Enter your name" class="w-full rounded-xl px-4 py-2.5 text-white transition-all" style="font-size: 0.875rem;" maxlength="32">
                        </div>
                    </div>

                    <div class="grid grid-cols-1 gap-3">
                        <div>
                            <label class="label-text block mb-2">Microphone</label>
                            <select id="audioSource" onchange="startPreview()" class="w-full rounded-xl px-3 py-2.5 text-sm text-white transition-all">
                                <option value="">Default</option>
                            </select>
                            <div class="mic-meter"><div id="setupMicBar" class="mic-bar"></div></div>
                        </div>
                        <div>
                            <label class="label-text block mb-2">Speaker</label>
                            <div class="flex gap-2">
                                <select id="audioOutputSource" onchange="changeAudioOutput(this.value)" class="flex-1 min-w-0 rounded-xl px-3 py-2.5 text-sm text-white transition-all">
                                    <option value="default">Default</option>
                                </select>
                                <button onclick="testSpeaker('audioOutputSource')" class="p-2.5 rounded-xl transition-all" style="background: var(--bg-secondary); color: var(--text-primary);" title="Test Speaker">
                                    <svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5"></polygon><path d="M19.07 4.93a10 10 0 0 1 0 14.14M15.54 8.46a5 5 0 0 1 0 7.07"></path></svg>
                                </button>
                            </div>
                        </div>
                        <div>
                            <label class="label-text block mb-2">Camera</label>
                            <select id="videoSource" onchange="startPreview()" class="w-full rounded-xl px-3 py-2.5 text-sm text-white transition-all">
                                <option value="">Default</option>
                            </select>
                        </div>
                    </div>

                    <button id="btnJoin" onclick="joinRoom()" disabled class="btn-primary w-full py-3.5 text-white rounded-xl font-semibold transition-all disabled:opacity-50 disabled:cursor-not-allowed">
                        Loading...
                    </button>
                </div>
            </div>
        </div>
    </div>

    <div id="settingsOverlay" class="fixed inset-0 z-[200] flex items-center justify-center p-4 hidden" style="background: rgba(0, 0, 0, 0.95); backdrop-filter: blur(20px) saturate(140%);" onclick="if(event.target === this) closeSettings()">
        <div class="glass-panel p-6 md:p-8 rounded-3xl max-w-5xl w-full max-h-[95vh] overflow-y-auto relative z-10">
             <button onclick="closeSettings()" class="absolute top-5 right-5 transition-colors p-1 rounded-lg hover:bg-white/10" style="color: var(--text-muted);">
                <svg xmlns="http://www.w3.org/2000/svg" width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="18" y1="6" x2="6" y2="18"></line><line x1="6" y1="6" x2="18" y2="18"></line></svg>
            </button>

            <div class="text-center space-y-1 mb-5">
                <h2 class="text-2xl md:text-3xl font-bold tracking-tight" style="color: var(--text-primary);">Settings</h2>
                <p style="color: var(--text-secondary);" class="text-sm font-normal">Update your profile and devices.</p>
            </div>

            <div class="flex flex-col lg:flex-row gap-6 lg:gap-8">

                <div class="lg:w-1/2 space-y-4">
                    <div class="flex flex-col items-center gap-4 p-4 rounded-2xl" style="background: var(--bg-secondary); border: 1px solid var(--border-subtle);">
                        <label class="label-text">Avatar</label>
                        <div class="flex flex-col items-center gap-3">
                            <div onclick="document.getElementById('settingsAvatarInput').click()" class="w-32 h-32 rounded-3xl cursor-pointer overflow-hidden flex items-center justify-center transition-all relative" style="background: var(--bg-tertiary); border: 2px solid var(--border-subtle);">
                                <img id="settingsAvatarPreview" src="" class="hidden w-full h-full object-cover" draggable="false">
                                <span id="settingsAvatarPlaceholder" class="text-6xl" style="color: var(--text-muted);">👤</span>
                                <div class="absolute inset-0 flex items-center justify-center opacity-0 group-hover:opacity-100 transition-opacity text-sm font-semibold" style="background: rgba(0, 0, 0, 0.75); color: var(--text-primary);">Change</div>
                            </div>
                                                            <input type="file" id="settingsAvatarInput" hidden accept="image/*" onchange="handleSettingsAvatarUpload(this)">
                                                        </div>
                                                    </div>
                    <div>
                        <label class="label-text block mb-2">Nickname</label>
                        <input type="text" id="settingsNicknameInput" placeholder="Enter your name" class="w-full rounded-xl px-4 py-2.5 text-white transition-all" style="font-size: 0.875rem;" maxlength="32">
                    </div>
                </div>

                <div class="lg:w-1/2 space-y-4">
                    <div class="grid grid-cols-1 gap-4">
                         <div>
                            <label class="label-text block mb-2">Microphone</label>
                            <select id="settingsAudioSource" onchange="currentAudioInputId=this.value" class="w-full rounded-xl px-3 py-2.5 text-sm text-white transition-all">
                            </select>
                            <div class="mic-meter"><div id="settingsMicBar" class="mic-bar"></div></div>
                        </div>
                         <div>
                            <label class="label-text block mb-2">Speaker</label>
                            <div class="flex gap-2">
                                <select id="settingsAudioOutputSource" onchange="changeAudioOutput(this.value)" class="flex-1 min-w-0 rounded-xl px-3 py-2.5 text-sm text-white transition-all">
                                </select>
                                <button onclick="testSpeaker('settingsAudioOutputSource')" class="p-2.5 rounded-xl transition-all" style="background: var(--bg-secondary); color: var(--text-primary);" title="Test Speaker">
                                    <svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5"></polygon><path d="M19.07 4.93a10 10 0 0 1 0 14.14M15.54 8.46a5 5 0 0 1 0 7.07"></path></svg>
                                </button>
                            </div>
                        </div>
                        <div>
                            <label class="label-text block mb-2">Camera</label>
                            <select id="settingsVideoSource" onchange="currentVideoInputId=this.value" class="w-full rounded-xl px-3 py-2.5 text-sm text-white transition-all">
                            </select>
                        </div>
                    </div>
                </div>
            </div>

            <div class="pt-2 mt-2">
                <button onclick="saveSettings()" class="btn-primary w-full py-3.5 text-white rounded-xl font-semibold transition-all">
                    Save Changes
                </button>
            </div>
        </div>
    </div>

    <div id="appLayout" class="hidden flex-col h-full w-full">
        <div class="flex-none p-3 sm:p-4 md:p-5 z-40 flex justify-between items-center gap-2 md:gap-4">
            <div class="flex items-center gap-2 md:gap-3 flex-1 min-w-0">
                <button id="sidebarToggle" onclick="toggleSidebar()" class="control-btn shadow-xl hidden !w-10 !h-10 md:!w-14 md:!h-14 flex-shrink-0" title="Channels">
                    <svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="3" y1="12" x2="21" y2="12"></line><line x1="3" y1="6" x2="21" y2="6"></line><line x1="3" y1="18" x2="21" y2="18"></line></svg>
                </button>
                <div id="currentChannelName" class="text-white font-semibold text-lg md:text-xl truncate min-w-0"></div>
            </div>

            <div class="flex items-center justify-end gap-2 md:gap-3 flex-shrink-0">
                <div class="status-pill px-3 md:px-4 py-1.5 md:py-2 rounded-full flex items-center justify-center gap-2 md:gap-2.5 flex-shrink-0 h-8 md:h-10">
                    <div id="connectionDot" class="connection-dot"></div>
                    <span id="statusText" class="text-xs md:text-sm font-medium hidden sm:inline-block" style="color: var(--text-primary);">Waiting...</span>
                    <button id="btnReconnect" onclick="retryConnection()" class="hidden ml-1 p-1 rounded-full transition-all" style="color: var(--text-muted);" title="Retry Connection">
                        <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M21 12a9 9 0 0 0-9-9 9.75 9.75 0 0 0-6.74 2.74L3 8"/><path d="M3 3v5h5"/><path d="M3 12a9 9 0 0 0 9 9 9.75 9.75 0 0 0 6.74-2.74L21 16"/><path d="M16 16h5v5"/></svg>
                    </button>
                    <div id="pingContainer" class="ping-container hidden !ml-0 !pl-1.5 md:!ml-0 md:!pl-2 border-l !border-[var(--border-subtle)]">
                        <span id="pingText" class="tabular-nums shrink-0 mr-1 md:mr-1.5">0ms</span>
                        <div id="pingBars" class="ping-bars">
                            <div class="ping-bar ping-bar-1"></div>
                            <div class="ping-bar ping-bar-2"></div>
                            <div class="ping-bar ping-bar-3"></div>
                        </div>
                    </div>
                    <div id="nodeIdBadge" class="hidden !ml-0 !pl-1.5 md:!ml-0 md:!pl-2 border-l !border-[var(--border-subtle)] flex items-center gap-1.5">
                        <svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" style="color: var(--text-muted); flex-shrink: 0;"><rect x="2" y="2" width="20" height="8" rx="2" ry="2"/><rect x="2" y="14" width="20" height="8" rx="2" ry="2"/><line x1="6" y1="6" x2="6.01" y2="6"/><line x1="6" y1="18" x2="6.01" y2="18"/></svg>
                        <span id="nodeIdText" class="text-xs font-mono tracking-wider shrink-0" style="color: var(--text-muted); font-size: 0.65rem; letter-spacing: 0.08em;"></span>
                    </div>
                </div>

                <div id="btnCopy" class="status-pill px-3 md:px-4 py-1.5 md:py-2 rounded-full cursor-pointer transition-all flex items-center justify-center gap-2 hover:border-opacity-30 flex-shrink-0" onclick="copyLink()" title="Invite Link">
                    <span class="text-xs md:text-sm font-medium hidden md:inline-block" style="color: var(--text-primary);">Invite Link</span>
                    <svg id="iconCopy" xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect width="14" height="14" x="8" y="8" rx="2" ry="2"/><path d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2"/></svg>
                </div>
            </div>
        </div>

        <main class="flex-1 w-full relative min-h-0">
            <div class="absolute inset-0 pb-4 md:pb-5 px-4 overflow-hidden flex items-center justify-center">
                 <div id="remoteGrid" class="grid gap-4 w-full h-full max-w-[1600px] transition-all duration-500 grid-expand"></div>
            </div>

            <div id="emptyState" class="hidden absolute top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 text-center pointer-events-none">
                <div class="mb-5">
                    <svg class="mx-auto h-16 w-16 empty-state-icon" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1" d="M17 20h5v-2a3 3 0 00-5.356-1.857M17 20H7m10 0v-2c0-.656-.126-1.283-.356-1.857M7 20H2v-2a3 3 0 015.356-1.857M7 20v-2c0-.656.126-1.283.356-1.857m0 0a5.002 5.002 0 019.288 0M15 7a3 3 0 11-6 0 3 3 0 016 0zm6 3a2 2 0 11-4 0 2 2 0 014 0zM7 10a2 2 0 11-4 0 2 2 0 014 0z" />
                    </svg>
                </div>
                <p class="text-xl font-semibold" style="color: var(--text-secondary);">Waiting for others to join...</p>
                <p class="text-sm mt-2" style="color: var(--text-muted);">Share the invite link to get started.</p>
            </div>

            <div class="pip-wrapper" id="localPipWrapper">
                 <div class="w-full h-full relative flex flex-col">
                    <div id="localAvatarLayer" class="absolute inset-0 z-20 flex items-center justify-center" style="display: none; background: var(--bg-secondary);">
                        <img id="localAvatarImg" src="" class="absolute inset-0 w-full h-full object-cover blur-xl opacity-30 hidden" draggable="false">
                        <div class="relative w-14 h-14 md:w-20 md:h-20 rounded-2xl flex items-center justify-center overflow-hidden z-10" style="background: var(--bg-secondary); border: 2px solid var(--border-subtle);">
                             <img id="localAvatarCenterImg" src="" class="w-full h-full object-cover hidden" draggable="false">
                             <div id="localAvatarPlaceholder" class="text-2xl md:text-3xl flex items-center justify-center w-full h-full" style="color: var(--text-muted); line-height: 1;">👤</div>
                        </div>
                    </div>

                    <video id="localVideo" autoplay playsinline muted class="w-full h-full object-cover z-10"></video>
                    <div id="localLabel" class="name-tag absolute bottom-2 left-2 px-2.5 py-1 rounded-lg text-[10px] md:text-xs font-medium backdrop-blur-sm z-30" style="background: rgba(0, 0, 0, 0.6); color: var(--text-primary);">
                        You
                    </div>
                </div>
            </div>
        </main>

        <footer class="flex-none taskbar w-full z-50">
            <div class="flex justify-center items-center py-4 md:py-5 gap-2.5 md:gap-3 px-4">
                <button class="control-btn" id="btnMic" onclick="toggleMic()" title="Toggle Microphone">
                    <svg xmlns="http://www.w3.org/2000/svg" width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 2a3 3 0 0 0-3 3v7a3 3 0 0 0 6 0V5a3 3 0 0 0-3-3Z"/><path d="M19 10v2a7 7 0 0 1-14 0v-2"/><line x1="12" x2="12" y1="19" y2="22"/></svg>
                </button>
                <button class="control-btn" id="btnDeafen" onclick="toggleDeafen()" title="Deafen (D)">
                    <svg xmlns="http://www.w3.org/2000/svg" width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M3 18v-6a9 9 0 0 1 18 0v6"></path><path d="M21 19a2 2 0 0 1-2 2h-1a2 2 0 0 1-2-2v-3a2 2 0 0 1 2-2h3zM3 19a2 2 0 0 0 2 2h1a2 2 0 0 0 2-2v-3a2 2 0 0 0-2-2H3z"></path></svg>
                </button>
                <button class="control-btn" id="btnCam" onclick="toggleCam()" title="Toggle Camera" disabled>
                    <svg xmlns="http://www.w3.org/2000/svg" width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M14.5 4h-5L7 7H4a2 2 0 0 0-2 2v9a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2V9a2 2 0 0 0-2-2h-3l-2.5-3z"/><circle cx="12" cy="13" r="3"/></svg>
                </button>
                <button class="control-btn" id="btnShare" onclick="toggleScreen()" title="Share Screen">
                    <svg xmlns="http://www.w3.org/2000/svg" width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect width="20" height="14" x="2" y="3" rx="2"/><line x1="8" x2="16" y1="21" y2="21"/><line x1="12" x2="12" y1="17" y2="21"/></svg>
                </button>
                <button class="control-btn" onclick="openSettings()" title="Settings">
                    <svg xmlns="http://www.w3.org/2000/svg" width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="3"></circle><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1-2-2 2 2 0 0 1 2-2h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.09a1.65 1.65 0 0 0-1.51 1z"></path></svg>
                </button>
                <div class="w-px mx-2" style="background: var(--border-medium);"></div>
                <button class="control-btn active-red" onclick="leaveRoom()" title="Leave Room">
                    <svg xmlns="http://www.w3.org/2000/svg" width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M9 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h4"/><polyline points="16 17 21 12 16 7"/><line x1="21" x2="9" y1="12" y2="12"/></svg>
                </button>
            </div>
        </footer>
    </div>

    <script>
        if ('serviceWorker' in navigator) {
            navigator.serviceWorker.register('/service-worker.js')
                .then(reg => console.log('SW registered'))
                .catch(err => console.log('SW error', err));
        }
    </script>
    <script>

        (function() {
            const canvas = document.getElementById('particleCanvas');
            const ctx = canvas.getContext('2d');
            let particles = [];
            let animationId = null;

            function resize() {
                canvas.width = window.innerWidth;
                canvas.height = window.innerHeight;
            }
            resize();
            window.addEventListener('resize', resize);

            class Particle {
                constructor() {
                    this.x = Math.random() * canvas.width;
                    this.y = Math.random() * canvas.height;
                    this.vx = (Math.random() - 0.5) * 0.5;
                    this.vy = (Math.random() - 0.5) * 0.5;
                    this.radius = Math.random() * 2 + 1;
                    this.opacity = Math.random() * 0.5 + 0.2;
                }
                update() {
                    this.x += this.vx;
                    this.y += this.vy;
                    if (this.x < 0) this.x = canvas.width;
                    if (this.x > canvas.width) this.x = 0;
                    if (this.y < 0) this.y = canvas.height;
                    if (this.y > canvas.height) this.y = 0;
                }
                draw() {
                    ctx.beginPath();
                    ctx.arc(this.x, this.y, this.radius, 0, Math.PI * 2);
                    ctx.fillStyle = `rgba(147, 130, 255, ${this.opacity})`;
                    ctx.fill();
                }
            }

            function init() {
                particles = [];
                const particleCount = Math.floor((canvas.width * canvas.height) / 15000);
                for (let i = 0; i < particleCount; i++) {
                    particles.push(new Particle());
                }
            }

            function animate() {
                const overlay = document.getElementById('welcomeOverlay');
                const isVisible = overlay && !overlay.classList.contains('hidden') && overlay.style.display !== 'none' && !document.hidden;

                if (!isVisible) {
                    animationId = requestAnimationFrame(animate);
                    return;
                }

                ctx.clearRect(0, 0, canvas.width, canvas.height);
                particles.forEach(p => {
                    p.update();
                    p.draw();
                });
                animationId = requestAnimationFrame(animate);
            }

            init();
            animate();
        })();

        (function() {
            const canvas = document.getElementById('particleCanvasConfig');
            const ctx = canvas.getContext('2d');
            let particles = [];
            let animationId = null;

            function resize() {
                canvas.width = window.innerWidth;
                canvas.height = window.innerHeight;
            }
            resize();
            window.addEventListener('resize', resize);

            class Particle {
                constructor() {
                    this.x = Math.random() * canvas.width;
                    this.y = Math.random() * canvas.height;
                    this.vx = (Math.random() - 0.5) * 0.5;
                    this.vy = (Math.random() - 0.5) * 0.5;
                    this.radius = Math.random() * 2 + 1;
                    this.opacity = Math.random() * 0.5 + 0.2;
                }
                update() {
                    this.x += this.vx;
                    this.y += this.vy;
                    if (this.x < 0) this.x = canvas.width;
                    if (this.x > canvas.width) this.x = 0;
                    if (this.y < 0) this.y = canvas.height;
                    if (this.y > canvas.height) this.y = 0;
                }
                draw() {
                    ctx.beginPath();
                    ctx.arc(this.x, this.y, this.radius, 0, Math.PI * 2);
                    ctx.fillStyle = `rgba(147, 130, 255, ${this.opacity})`;
                    ctx.fill();
                }
            }

            function init() {
                particles = [];
                const particleCount = Math.floor((canvas.width * canvas.height) / 15000);
                for (let i = 0; i < particleCount; i++) {
                    particles.push(new Particle());
                }
            }

            function animate() {
                const overlay = document.getElementById('configOverlay');
                const isVisible = overlay && !overlay.classList.contains('hidden') && overlay.style.display !== 'none' && !document.hidden;

                if (!isVisible) {
                    animationId = requestAnimationFrame(animate);
                    return;
                }

                ctx.clearRect(0, 0, canvas.width, canvas.height);
                particles.forEach(p => {
                    p.update();
                    p.draw();
                });
                animationId = requestAnimationFrame(animate);
            }

            init();
            animate();
        })();
    </script>
    <script>
        let parts = window.location.pathname.split('/').filter(p => p !== '');
        let roomId = parts[0] || '';
        let channelId = parts[1] || (roomId ? 'General' : '');
        if (channelId.length > 32) channelId = channelId.substring(0, 32);

        const initialChannelNameEl = document.getElementById('currentChannelName');
        if (initialChannelNameEl && channelId) {
            initialChannelNameEl.innerText = `# ${channelId}`;
        }

        const currentPath = window.location.pathname;
        const newPath = `/${roomId}${channelId ? '/' + channelId : ''}`;
        if (currentPath !== newPath && roomId) {
            window.history.replaceState({ roomId, channelId }, "", newPath);
        }

        const wsProtocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
        let wsUrl = roomId ? `${wsProtocol}//${window.location.host}/ws/${roomId}/${channelId}` : '';

        let ws;
        let localStream;
        let screenStream;
        let peers = {};
        let peerCamStatus = {};
        let peerScreenStatus = {};
        let peerScreenHasAudio = {};
        let peerMicTrackId = {};
        let peerScreenAudioTrackId = {};
        let userNickname = "Guest";
        let userAvatar = null;
        let sidebarOpen = false;
        let globalRoomList = {};
        let isConfigured = false;
        let audioContext;
        let wakeLock = null;
        let currentAudioOutputId = 'default';
        let currentAudioInputId = null;
        let currentVideoInputId = null;
        let isDeafened = false;
        let roomCreationPassword = sessionStorage.getItem('rustrooms_room_password');
        let workletLoadingPromise = null;

        let persistentUserId = localStorage.getItem('rustrooms_user_id');
        if (!persistentUserId) {
            persistentUserId = crypto.randomUUID();
            localStorage.setItem('rustrooms_user_id', persistentUserId);
        }

        let reconnectionAttempts = 0;
        const maxReconnectionAttempts = 10;
        const baseReconnectionDelay = 1000;
        const maxReconnectionDelay = 30000;
        let hasLeftRoom = false;
        let isReconnecting = false;
        let awaitingPassword = false;

        let reconnectStatusTimeout = null;
        const reconnectDelayMs = 5000;

        let heartbeatInterval = null;
        const heartbeatIntervalMs = 3000;
        const heartbeatTimeoutMs = 5000;
        let lastPingSentTime = 0;
        let lastPongTime = Date.now();
        let heartbeatTimeout = null;

        let currentNodeId = null;
        let rankedPeers = [];
        let failoverIndex = 0;
        let currentServerHost = window.location.host;
        let originalHost = window.location.host;

        async function fetchNodeId(host) {
            try {
                const protocol = window.location.protocol;
                const resp = await fetch(`${protocol}//${host}/node-id`);
                if (resp.ok) {
                    const data = await resp.json();
                    currentNodeId = data.nodeId;
                    const badge = document.getElementById('nodeIdBadge');
                    const text = document.getElementById('nodeIdText');
                    if (badge && text) {
                        text.innerText = data.nodeId;
                        badge.classList.remove('hidden');
                    }
                }
            } catch (e) {
                console.warn('Failed to fetch node ID:', e);
            }
        }

        async function pingHost(host) {
            const protocol = window.location.protocol;
            try {
                const start = performance.now();
                const resp = await fetch(`${protocol}//${host}/ping`, { mode: 'cors', cache: 'no-cache' });
                const end = performance.now();
                if (resp.ok) return Math.round(end - start);
            } catch (e) {  }
            return Infinity;
        }

        async function discoverAndRankPeers() {
            const protocol = window.location.protocol;
            try {
                const resp = await fetch(`${protocol}//${currentServerHost}/cluster-peers`);
                if (!resp.ok) return [];
                const data = await resp.json();
                const allHosts = [currentServerHost];
                if (data.peers && data.peers.length > 0) {
                    for (const peer of data.peers) {
                        if (!allHosts.includes(peer)) allHosts.push(peer);
                    }
                }

                const results = await Promise.all(allHosts.map(async (host) => {
                    const latency = await pingHost(host);
                    return { host, latency };
                }));

                results.sort((a, b) => a.latency - b.latency);
                console.log('Peer latency ranking:', results.map(r => `${r.host}: ${r.latency}ms`).join(', '));
                return results.filter(r => r.latency < Infinity);
            } catch (e) {
                console.warn('Failed to discover peers:', e);
                return [{ host: currentServerHost, latency: 0 }];
            }
        }

        async function findBestServer() {
            const peers = await discoverAndRankPeers();
            rankedPeers = peers;
            failoverIndex = 0;
            if (peers.length > 0 && peers[0].host !== currentServerHost) {
                const best = peers[0];
                console.log(`Switching to closer server: ${best.host} (${best.latency}ms)`);
                currentServerHost = best.host;
                wsUrl = `${wsProtocol}//${currentServerHost}/ws/${roomId}/${channelId}`;
            }
        }

        function getNextFailoverHost() {
            if (rankedPeers.length === 0) return null;
            failoverIndex++;
            if (failoverIndex >= rankedPeers.length) {
                failoverIndex = 0;
                return null;
            }
            return rankedPeers[failoverIndex];
        }

        function getScreenAudioFlag(data) {
            if (!data) return undefined;
            if (data.hasAudio !== undefined) return !!data.hasAudio;
            if (data.screenAudio !== undefined) return !!data.screenAudio;
            return undefined;
        }

        function updatePeerTrackHints(userId, data) {
            if (!data || !userId) return;
            if (data.micTrackId !== undefined) {
                peerMicTrackId[userId] = data.micTrackId || null;
            }
            if (data.screenAudioTrackId !== undefined) {
                peerScreenAudioTrackId[userId] = data.screenAudioTrackId || null;
            }
        }

        function ensureScreenAudioUI(userId) {
            if (!peerScreenHasAudio[userId]) return;

            const vid = document.getElementById(`vid-${userId}`);
            const volControls = document.getElementById(`vol-controls-${userId}`);
            if (!vid || !vid.srcObject || !volControls) return;

            if (document.getElementById(`vol-row-screen-${userId}`)) return;

            const savedScreenVol = getVolumeSettings(userId, 'screen');
            const row = document.createElement('div');
            row.className = 'vol-row';
            row.id = `vol-row-screen-${userId}`;
            row.innerHTML = `
                <div class="flex items-center gap-2">
                    <button class="text-white hover:text-blue-400" onclick="toggleMute('${userId}', 'screen')" id="mute-screen-${userId}">
                        <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="4" y="2" width="16" height="14" rx="2" ry="2"></rect><line x1="12" y1="22" x2="12" y2="16"></line><path d="M5 12h14"></path><path d="M12 12v4"></path></svg>
                    </button>
                    <input type="range" min="0" max="1" step="0.05" value="${savedScreenVol}" oninput="setVolume('${userId}', 'screen', this.value)">
                </div>
            `;
            volControls.appendChild(row);
        }

        const rtcConfig = {
            iceServers: [
                {
                    urls: "{{TURN_URL}}",
                    username: "{{TURN_USERNAME}}",
                    credential: "{{TURN_CREDENTIAL}}"
                }
            ]
        };

        function getReconnectDelay(attempt) {
            const exponentialDelay = Math.min(
                baseReconnectionDelay * Math.pow(2, attempt),
                maxReconnectionDelay
            );

            const jitter = exponentialDelay * 0.25 * (Math.random() * 2 - 1);
            return Math.max(exponentialDelay + jitter, baseReconnectionDelay);
        }

        function updatePingUI(pingMs) {
            const pingContainer = document.getElementById('pingContainer');
            const pingText = document.getElementById('pingText');

            if (pingContainer && pingText) {
                pingContainer.classList.remove('hidden');
                pingText.innerText = `${pingMs}ms`;

                pingContainer.classList.remove('ping-good', 'ping-fair', 'ping-poor');
                if (pingMs < 100) {
                    pingContainer.classList.add('ping-good');
                } else if (pingMs < 250) {
                    pingContainer.classList.add('ping-fair');
                } else {
                    pingContainer.classList.add('ping-poor');
                }
            }
        }

        function sendPing() {
            if (ws && ws.readyState === WebSocket.OPEN) {
                lastPingSentTime = Date.now();
                ws.send(JSON.stringify({ type: 'ping' }));

                if (heartbeatTimeout) clearTimeout(heartbeatTimeout);
                heartbeatTimeout = setTimeout(() => {
                    const timeSincePong = Date.now() - lastPongTime;
                    if (timeSincePong > heartbeatIntervalMs + heartbeatTimeoutMs) {
                        console.warn('Heartbeat timeout - no pong received, closing connection');
                        ws.close();
                    }
                }, heartbeatTimeoutMs);
            }
        }

        function startHeartbeat() {
            stopHeartbeat();
            lastPongTime = Date.now();

            sendPing();
            heartbeatInterval = setInterval(sendPing, heartbeatIntervalMs);
        }

        function stopHeartbeat() {
            if (heartbeatInterval) {
                clearInterval(heartbeatInterval);
                heartbeatInterval = null;
            }
            if (heartbeatTimeout) {
                clearTimeout(heartbeatTimeout);
                heartbeatTimeout = null;
            }
            const pingContainer = document.getElementById('pingContainer');
            if (pingContainer) pingContainer.classList.add('hidden');
        }

        function handlePong() {
            lastPongTime = Date.now();
            const pingMs = lastPongTime - lastPingSentTime;

            if (lastPingSentTime > 0) {
                updatePingUI(pingMs);
            }

            if (heartbeatTimeout) {
                clearTimeout(heartbeatTimeout);
                heartbeatTimeout = null;
            }
        }

        const localVideo = document.getElementById('localVideo');
        const previewVideo = document.getElementById('previewVideo');
        const remoteGrid = document.getElementById('remoteGrid');
        const emptyState = document.getElementById('emptyState');
        const connectionDot = document.getElementById('connectionDot');
        const statusText = document.getElementById('statusText');
        const configOverlay = document.getElementById('configOverlay');
        const appLayout = document.getElementById('appLayout');
        const nicknameInput = document.getElementById('nicknameInput');
        const audioSelect = document.getElementById('audioSource');
        const audioOutputSelect = document.getElementById('audioOutputSource');
        const videoSelect = document.getElementById('videoSource');
        const avatarPreview = document.getElementById('avatarPreview');
        const avatarPlaceholder = document.getElementById('avatarPlaceholder');

        async function initAudioWorklet() {
            if (workletLoadingPromise) return workletLoadingPromise;

            if (!audioContext) {
                audioContext = new (window.AudioContext || window.webkitAudioContext)();
            }

            workletLoadingPromise = (async () => {
                try {
                    if (audioContext.state === 'suspended') {
                        await audioContext.resume();
                    }
                    await audioContext.audioWorklet.addModule('/rnnoise_processor.js');
                    console.log("AudioWorklet loaded");
                    return true;
                } catch (err) {
                    console.error("Failed to load AudioWorklet", err);
                    workletLoadingPromise = null;
                    return false;
                }
            })();

            return workletLoadingPromise;
        }

        async function requestWakeLock() {
            try {
                if ('wakeLock' in navigator) {
                    wakeLock = await navigator.wakeLock.request('screen');
                    wakeLock.addEventListener('release', () => {
                        console.log('Wake Lock released');
                    });
                    console.log('Wake Lock active');
                }
            } catch (err) {
                console.error(`${err.name}, ${err.message}`);
            }
        }

        document.addEventListener('visibilitychange', async () => {
            if (wakeLock !== null && document.visibilityState === 'visible') {
                await requestWakeLock();
            }
        });

        async function loadDevices() {
            const btnJoin = document.getElementById('btnJoin');
            const btnCam = document.getElementById('btnCam');

            isCameraReady = false;
            if (btnCam) btnCam.disabled = true;

            loadPreferences();
            try {

                if (pendingCamToggle) {
                    localStream = await navigator.mediaDevices.getUserMedia({ audio: true, video: false });
                } else {
                    localStream = await navigator.mediaDevices.getUserMedia({ audio: true, video: true });
                }
                if (localStream) {
                    if (pendingMicToggle) {
                        const audioTrack = localStream.getAudioTracks()[0];
                        if (audioTrack) audioTrack.enabled = !audioTrack.enabled;
                        pendingMicToggle = false;
                    }
                }
                previewVideo.srcObject = localStream;
                document.getElementById('previewPlaceholder').style.display = 'none';
                updatePreviewButtons();
                await new Promise(r => setTimeout(r, 500));
                await populateDeviceList();
                navigator.mediaDevices.ondevicechange = populateDeviceList;

                initAudioWorklet();

                await startPreview();

            } catch (e) {
                console.warn("Device access failed", e);
                try {
                    localStream = await navigator.mediaDevices.getUserMedia({ audio: true, video: false });
                    if (localStream && pendingMicToggle) {
                        const audioTrack = localStream.getAudioTracks()[0];
                        if (audioTrack) audioTrack.enabled = !audioTrack.enabled;
                        pendingMicToggle = false;
                    }
                    updatePreviewButtons();
                    await populateDeviceList();
                    await startPreview();
                } catch(e2) {
                     console.error("Audio failed too", e2);
                     updatePreviewButtons();
                }
            }

            if(btnJoin) {
                 btnJoin.disabled = false;
                 btnJoin.innerHTML = "Join Room";
            }

            isCameraReady = true;
            if(btnCam) {
                 btnCam.disabled = false;
                 if (pendingCamToggle) {
                     btnCam.classList.add('active-red');
                     btnCam.innerHTML = `<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="1" y1="1" x2="23" y2="23"></line><path d="M21 21l-3.5-3.5m-2-2l-2-2m-2-2l-2-2m-2-2l-3.5-3.5"></path><path d="M15 7h5a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2h-5"></path><path d="M4 8v8a2 2 0 0 0 2 2h4.5"></path></svg>`;
                 }
            }
        }

        async function populateDeviceList() {
            try {
                const devices = await navigator.mediaDevices.enumerateDevices();
                const currentAudio = audioSelect.value;
                const currentAudioOutput = currentAudioOutputId;
                const currentVideo = videoSelect.value;

                const audioTrack = localStream ? localStream.getAudioTracks()[0] : null;
                const videoTrack = localStream ? localStream.getVideoTracks()[0] : null;

                const activeAudioId = audioTrack ? audioTrack.getSettings().deviceId : null;
                const activeVideoId = videoTrack ? videoTrack.getSettings().deviceId : null;

                audioSelect.innerHTML = '';
                audioOutputSelect.innerHTML = '';
                videoSelect.innerHTML = '';

                devices.forEach(device => {
                    const option = document.createElement('option');
                    option.value = device.deviceId;
                    option.text = device.label || `${device.kind} (${device.deviceId.slice(0,5)}...)`;
                    if (device.kind === 'audioinput') {
                        audioSelect.appendChild(option);
                    } else if (device.kind === 'audiooutput') {
                        audioOutputSelect.appendChild(option);
                    }
                    else if (device.kind === 'videoinput') videoSelect.appendChild(option);
                });

                const targetAudioId = currentAudioInputId || activeAudioId;
                if (targetAudioId && [...audioSelect.options].some(o => o.value === targetAudioId)) {
                    audioSelect.value = targetAudioId;
                }

                const targetAudioOutputId = currentAudioOutputId || 'default';
                if (targetAudioOutputId && [...audioOutputSelect.options].some(o => o.value === targetAudioOutputId)) {
                    audioOutputSelect.value = targetAudioOutputId;
                }

                const targetVideoId = currentVideoInputId || activeVideoId;
                if (targetVideoId && [...videoSelect.options].some(o => o.value === targetVideoId)) {
                    videoSelect.value = targetVideoId;
                }

            } catch(e) {
                console.error("Enumeration error", e);
            }
        }

        async function populateSettingsDeviceList() {
            try {
                const devices = await navigator.mediaDevices.enumerateDevices();
                const settingsAudio = document.getElementById('settingsAudioSource');
                const settingsAudioOutput = document.getElementById('settingsAudioOutputSource');
                const settingsVideo = document.getElementById('settingsVideoSource');

                const audioTrack = localStream ? localStream.getAudioTracks()[0] : null;
                const videoTrack = localStream ? localStream.getVideoTracks()[0] : null;

                const activeAudioId = audioTrack ? audioTrack.getSettings().deviceId : null;
                const activeAudioOutputId = currentAudioOutputId;
                const activeVideoId = videoTrack ? videoTrack.getSettings().deviceId : null;

                settingsAudio.innerHTML = '';
                settingsAudioOutput.innerHTML = '';
                settingsVideo.innerHTML = '';

                devices.forEach(device => {
                    const option = document.createElement('option');
                    option.value = device.deviceId;
                    option.text = device.label || `${device.kind} (${device.deviceId.slice(0,5)}...)`;
                    if (device.kind === 'audioinput') {
                        settingsAudio.appendChild(option);
                    } else if (device.kind === 'audiooutput') {
                        settingsAudioOutput.appendChild(option);
                    }
                    else if (device.kind === 'videoinput') settingsVideo.appendChild(option);
                });

                const targetAudioId = currentAudioInputId || activeAudioId;
                if (targetAudioId && [...settingsAudio.options].some(o => o.value === targetAudioId)) {
                    settingsAudio.value = targetAudioId;
                }

                const targetAudioOutputId = currentAudioOutputId || 'default';
                if (targetAudioOutputId && [...settingsAudioOutput.options].some(o => o.value === targetAudioOutputId)) {
                    settingsAudioOutput.value = targetAudioOutputId;
                }

                const targetVideoId = currentVideoInputId || activeVideoId;
                if (targetVideoId && [...settingsVideo.options].some(o => o.value === targetVideoId)) {
                    settingsVideo.value = targetVideoId;
                }
            } catch (e) { console.error(e); }
        }

        async function changeAudioOutput(deviceId) {
            currentAudioOutputId = deviceId;
            const elements = document.querySelectorAll('video, audio');
            for (const el of elements) {
                await attachSinkId(el, deviceId);
            }
            savePreferences();
        }

        async function attachSinkId(element, sinkId) {
            if (typeof element.setSinkId === 'function') {
                try {
                    await element.setSinkId(sinkId);
                } catch (e) {
                    console.warn("Failed to set audio output device", e);
                }
            }
        }

        async function switchMediaStream(audioId, videoId) {
             const currentAudioTrack = localStream ? localStream.getAudioTracks()[0] : null;
             const currentVideoTrack = localStream ? localStream.getVideoTracks()[0] : null;
             const currentAudioId = currentAudioTrack ? currentAudioTrack.getSettings().deviceId : "";
             const currentVideoId = currentVideoTrack ? currentVideoTrack.getSettings().deviceId : "";

             const settingsVideoEl = document.getElementById('settingsVideoSource');
             const originalSettingsVideoValue = settingsVideoEl ? settingsVideoEl.value : null;
             if (videoId && videoId !== currentVideoId && settingsVideoEl) {
                 settingsVideoEl.disabled = true;
             }

             if (audioId && audioId !== currentAudioId) {
                 try {
                     const constraints = {
                        audio: {
                            deviceId: { exact: audioId },
                            echoCancellation: true,
                            noiseSuppression: false,
                            autoGainControl: true,
                            sampleRate: 48000
                        }
                    };
                     let stream = await navigator.mediaDevices.getUserMedia(constraints);

                     if (!audioContext) audioContext = new (window.AudioContext || window.webkitAudioContext)();
                     await initAudioWorklet();
                     if (audioContext.state === 'suspended') audioContext.resume().catch(e => {});

                     const source = audioContext.createMediaStreamSource(stream);
                     const worklet = new AudioWorkletNode(audioContext, 'rnnoise-processor');
                     const dest = audioContext.createMediaStreamDestination();
                     source.connect(worklet);
                     worklet.connect(dest);

                     const newTrack = dest.stream.getAudioTracks()[0];

                     if (localStream && localStream._originalStream) {
                         localStream._originalStream.getTracks().forEach(t => t.stop());
                     }
                      if (localStream) {
                          if (currentAudioTrack) {
                              currentAudioTrack.stop();
                              localStream.removeTrack(currentAudioTrack);
                          }
                          localStream.addTrack(newTrack);
                      } else {
                          localStream = new MediaStream([newTrack]);
                          if (localVideo) localVideo.srcObject = localStream;
                      }
                      localStream._originalStream = stream;

                      for (const userId in peers) {
                         const pc = peers[userId];
                         const sender = pc.getSenders().find(s => s.track && s.track.kind === 'audio');
                         if (sender) {
                              sender.replaceTrack(newTrack);
                         } else {
                              pc.addTrack(newTrack, localStream);
                              negotiate(userId, pc);
                         }
                      }

                     await setupAudioMonitor(localStream, 'local');
                     await setupVolumeMeter(localStream, 'settingsMicBar');

                 } catch (e) {
                     console.error("Audio switch failed", e);
                     alert("Failed to switch microphone: " + e.message);
                 }
             }

             if (videoId && videoId !== currentVideoId) {
                 try {
                     const constraints = { video: { deviceId: { exact: videoId } } };
                     const newVideoStream = await navigator.mediaDevices.getUserMedia(constraints);
                     const newTrack = newVideoStream.getVideoTracks()[0];

                      if (localStream) {
                          localStream.addTrack(newTrack);
                      } else {
                          localStream = new MediaStream([newTrack]);
                          if (localVideo) localVideo.srcObject = localStream;
                      }

                      if (!screenStream) {
                         for (const userId in peers) {
                            const pc = peers[userId];
                            const sender = pc.getSenders().find(s => s.track && s.track.kind === 'video');
                            if (sender) {
                                sender.replaceTrack(newTrack);
                            } else {
                                pc.addTrack(newTrack, localStream);
                                negotiate(userId, pc);
                            }
                         }

                         if (ws && ws.readyState === WebSocket.OPEN) {
                             ws.send(JSON.stringify({
                                 type: 'cam-toggle',
                                 data: { enabled: true }
                             }));
                         }
                      }

                     if (currentVideoTrack) {
                         localStream.removeTrack(currentVideoTrack);
                         currentVideoTrack.stop();
                     }

                 } catch (e) {
                     console.error("Video switch failed", e);
                 } finally {

                     if (settingsVideoEl) {
                         settingsVideoEl.disabled = false;
                     }
                 }
             }

             updateLocalAvatar();
        }

        async function setupAudioMonitor(stream, targetId) {
            if (!audioContext) return;
            if (!stream.getAudioTracks().length) return;

            if (audioContext.state === 'suspended') {
                await audioContext.resume();
            }

            const source = audioContext.createMediaStreamSource(stream);
            const analyser = audioContext.createAnalyser();
            analyser.fftSize = 256;
            source.connect(analyser);

            const bufferLength = analyser.frequencyBinCount;
            const dataArray = new Uint8Array(bufferLength);

            function checkAudio() {
                if (targetId !== 'local' && !document.getElementById(targetId)) {
                    return;
                }

                analyser.getByteFrequencyData(dataArray);
                let sum = 0;
                for(let i = 0; i < bufferLength; i++) {
                    sum += dataArray[i];
                }
                const average = sum / bufferLength;

                let targetEl;
                let isVideoActive = false;

                if (targetId === 'local') {
                    isVideoActive = localVideo.srcObject && localVideo.srcObject.getVideoTracks().length > 0;
                    targetEl = document.getElementById('localPipWrapper');
                } else {
                    const rawUserId = targetId.startsWith('wrapper-') ? targetId.replace('wrapper-', '') : targetId;
                    const isCamOn = peerCamStatus[rawUserId] !== false;
                    const isScreenOn = peerScreenStatus[rawUserId] === true;

                    if (isCamOn || isScreenOn) {
                        const wrapper = document.getElementById(targetId);
                        if (wrapper) {
                            const vid = document.getElementById(`vid-${rawUserId}`);
                            if (vid && vid.classList.contains('active')) {
                                isVideoActive = true;
                            }
                        }
                    }

                    const wrapper = document.getElementById(targetId);
                    if (wrapper) {
                        if (isVideoActive) {
                            targetEl = wrapper;
                        } else {
                            targetEl = wrapper.querySelector('.avatar-center');
                        }
                    }
                }

                if (targetEl) {
                    if (average > 10) {
                        targetEl.classList.add('speaking-glow');

                        if (targetId === 'local') {
                            const localSidebarAvatar = document.querySelector(`.room-user-row[data-user-id="${persistentUserId}"] .mini-avatar`);
                            if (localSidebarAvatar) localSidebarAvatar.classList.add('speaking-glow');
                        } else {
                            const rawUserId = targetId.startsWith('wrapper-') ? targetId.replace('wrapper-', '') : targetId;
                            const sidebarAvatar = document.querySelector(`.room-user-row[data-user-id="${rawUserId}"] .mini-avatar`);
                            if (sidebarAvatar) sidebarAvatar.classList.add('speaking-glow');
                        }

                        if (targetId !== 'local' && targetEl.classList.contains('avatar-center')) {
                            const wrapper = document.getElementById(targetId);
                            if (wrapper) wrapper.classList.remove('speaking-glow');
                        }

                        if (targetId !== 'local' && !targetEl.classList.contains('avatar-center')) {
                            const avatar = document.getElementById(targetId)?.querySelector('.avatar-center');
                            if (avatar) avatar.classList.remove('speaking-glow');
                        }
                    } else {
                        targetEl.classList.remove('speaking-glow');

                        if (targetId === 'local') {
                            const localSidebarAvatar = document.querySelector(`.room-user-row[data-user-id="${persistentUserId}"] .mini-avatar`);
                            if (localSidebarAvatar) localSidebarAvatar.classList.remove('speaking-glow');
                        } else {
                            const wrapper = document.getElementById(targetId);
                            if (wrapper) {
                                wrapper.classList.remove('speaking-glow');
                                const avatar = wrapper.querySelector('.avatar-center');
                                if (avatar) avatar.classList.remove('speaking-glow');
                            }
                            const rawUserId = targetId.startsWith('wrapper-') ? targetId.replace('wrapper-', '') : targetId;
                            const sidebarAvatar = document.querySelector(`.room-user-row[data-user-id="${rawUserId}"] .mini-avatar`);
                            if (sidebarAvatar) sidebarAvatar.classList.remove('speaking-glow');
                        }
                    }
                } else {

                    if (targetId !== 'local') {
                        const rawUserId = targetId.startsWith('wrapper-') ? targetId.replace('wrapper-', '') : targetId;
                        const sidebarAvatar = document.querySelector(`.room-user-row[data-user-id="${rawUserId}"] .mini-avatar`);
                        if (sidebarAvatar) sidebarAvatar.classList.remove('speaking-glow');
                    }
                }

                requestAnimationFrame(checkAudio);
            }
            checkAudio();
        }

        function loadPreferences() {
            const stored = localStorage.getItem('rustrooms_profile');
            if (stored) {
                try {
                    const data = JSON.parse(stored);
                    if (data.nickname) {
                        userNickname = data.nickname;
                        if (nicknameInput) nicknameInput.value = userNickname;
                        if (document.getElementById('settingsNicknameInput')) document.getElementById('settingsNicknameInput').value = userNickname;
                    }
                    if (data.avatar) {
                        userAvatar = data.avatar;
                        if (avatarPreview) {
                            avatarPreview.src = userAvatar;
                            avatarPreview.classList.remove('hidden');
                            avatarPlaceholder.classList.add('hidden');
                        }
                        if (document.getElementById('settingsAvatarPreview')) {
                            const sap = document.getElementById('settingsAvatarPreview');
                            sap.src = userAvatar;
                            sap.classList.remove('hidden');
                            document.getElementById('settingsAvatarPlaceholder').classList.add('hidden');
                        }
                    }
                    if (data.audioOutputId) {
                        currentAudioOutputId = data.audioOutputId;
                    }
                    if (data.audioInputId) {
                        currentAudioInputId = data.audioInputId;
                    }
                    if (data.videoInputId) {
                        currentVideoInputId = data.videoInputId;
                    }
                    if (data.isMuted !== undefined) {
                        pendingMicToggle = data.isMuted;
                    }
                    if (data.isCamOff !== undefined) {
                        pendingCamToggle = data.isCamOff;
                    }
                    if (data.isDeafened !== undefined) {
                        isDeafened = data.isDeafened;
                    }
                } catch (e) { console.error("Load pref error", e); }
            }
        }

        function savePreferences() {
            let audioInputId = currentAudioInputId;
            let videoInputId = currentVideoInputId;
            let audioOutputId = currentAudioOutputId;

            const isSettingsOpen = settingsOverlay && !settingsOverlay.classList.contains('hidden');
            const isConfigOpen = configOverlay && !configOverlay.classList.contains('hidden');

            if (isSettingsOpen) {
                const sAudio = document.getElementById('settingsAudioSource');
                const sVideo = document.getElementById('settingsVideoSource');
                const sAudioOut = document.getElementById('settingsAudioOutputSource');
                if (sAudio && sAudio.value !== undefined) audioInputId = sAudio.value;
                if (sVideo && sVideo.value !== undefined) videoInputId = sVideo.value;
                if (sAudioOut && sAudioOut.value !== undefined) audioOutputId = sAudioOut.value;
            } else if (isConfigOpen) {
                if (audioSelect) audioInputId = audioSelect.value;
                if (videoSelect) videoInputId = videoSelect.value;
                if (audioOutputSelect) audioOutputId = audioOutputSelect.value;
            }

            let isMuted = pendingMicToggle;
            let isCamOff = pendingCamToggle;

            if (localStream) {
                const audioTrack = localStream.getAudioTracks()[0];
                const videoTrack = localStream.getVideoTracks()[0];
                if (audioTrack) isMuted = !audioTrack.enabled;
                if (videoTrack) isCamOff = !videoTrack.enabled;
            }

            localStorage.setItem('rustrooms_profile', JSON.stringify({
                nickname: userNickname,
                avatar: userAvatar,
                audioOutputId: audioOutputId,
                audioInputId: audioInputId,
                videoInputId: videoInputId,
                isMuted: isMuted,
                isCamOff: isCamOff,
                isDeafened: isDeafened
            }));

            currentAudioInputId = audioInputId;
            currentVideoInputId = videoInputId;
            currentAudioOutputId = audioOutputId;
        }

        async function testSpeaker(selectId) {
            const el = document.getElementById(selectId);
            if (!el) return;
            const deviceId = el.value;

            if (!audioContext) audioContext = new (window.AudioContext || window.webkitAudioContext)();
            if (audioContext.state === 'suspended') await audioContext.resume();

            const osc = audioContext.createOscillator();
            const gain = audioContext.createGain();

            osc.connect(gain);

            const isSetSinkIdSupported = 'setSinkId' in HTMLMediaElement.prototype;
            const isNonDefaultDevice = deviceId && deviceId !== 'default';

            if (isNonDefaultDevice && isSetSinkIdSupported) {
                const dest = audioContext.createMediaStreamDestination();
                gain.connect(dest);

                const audio = new Audio();
                audio.srcObject = dest.stream;

                try {
                    await audio.setSinkId(deviceId);
                } catch(e) {
                    console.warn("setSinkId failed", e);
                }

                audio.play().catch(e => console.warn("Audio play failed", e));
            } else {
                gain.connect(audioContext.destination);
            }

            osc.type = 'sine';
            osc.frequency.setValueAtTime(523.25, audioContext.currentTime);
            osc.frequency.exponentialRampToValueAtTime(1046.5, audioContext.currentTime + 0.1);

            gain.gain.setValueAtTime(0.2, audioContext.currentTime);
            gain.gain.exponentialRampToValueAtTime(0.001, audioContext.currentTime + 0.5);

            osc.start();
            osc.stop(audioContext.currentTime + 0.5);
        }

        let setupMeterFrameId = null;
        let settingsMeterFrameId = null;

        async function setupVolumeMeter(stream, barId) {
            const bar = document.getElementById(barId);
            if (!bar) return;

            if (barId === 'setupMicBar') {
                if (setupMeterFrameId) cancelAnimationFrame(setupMeterFrameId);
            } else if (barId === 'settingsMicBar') {
                if (settingsMeterFrameId) cancelAnimationFrame(settingsMeterFrameId);
            }

            if (bar._audioSource) {
                try { bar._audioSource.disconnect(); } catch(e) {}
                bar._audioSource = null;
            }
            if (bar._analyser) {
                try { bar._analyser.disconnect(); } catch(e) {}
                bar._analyser = null;
            }

            if (!stream || !stream.getAudioTracks().length) {
                bar.style.width = '0%';
                return;
            }

            if (!audioContext) audioContext = new (window.AudioContext || window.webkitAudioContext)();
            if (audioContext.state === 'suspended') await audioContext.resume();

            const source = audioContext.createMediaStreamSource(stream);
            const analyser = audioContext.createAnalyser();
            analyser.fftSize = 256;
            source.connect(analyser);

            bar._audioSource = source;
            bar._analyser = analyser;

            const dataArray = new Uint8Array(analyser.frequencyBinCount);

            function draw() {
                if (!bar._analyser) return;
                analyser.getByteFrequencyData(dataArray);
                let sum = 0;
                for (let i = 0; i < dataArray.length; i++) {
                    sum += dataArray[i];
                }
                const average = sum / dataArray.length;
                const val = Math.min(100, (average / 60) * 100);
                bar.style.width = val + '%';

                if (barId === 'setupMicBar') {
                    setupMeterFrameId = requestAnimationFrame(draw);
                } else {
                    settingsMeterFrameId = requestAnimationFrame(draw);
                }
            }
            draw();
        }

        function handleAvatarUpload(input) {
            const file = input.files[0];
            if (!file) return;

            const reader = new FileReader();
            reader.onload = function(e) {
                openCropModal(e.target.result, 'setup');
            };
            reader.readAsDataURL(file);
            input.value = '';
        }

        let isPreviewStarting = false;
        let pendingCamToggle = false;
        let pendingMicToggle = false;
        let isCameraReady = true;

        async function startPreview() {
            if (isPreviewStarting) {
                return;
            }

            let previousVideoEnabled = true;
            let previousAudioEnabled = true;
            if (localStream) {
                const oldV = localStream.getVideoTracks()[0];
                const oldA = localStream.getAudioTracks()[0];
                if (oldV) previousVideoEnabled = oldV.enabled;
                if (oldA) previousAudioEnabled = oldA.enabled;
            }

            isPreviewStarting = true;

            const btnPreviewCam = document.getElementById('btnPreviewCam');
            const btnPreviewMic = document.getElementById('btnPreviewMic');
            if (btnPreviewCam) {
                btnPreviewCam.disabled = true;
                btnPreviewCam.classList.add('opacity-50', 'cursor-not-allowed');
            }
            if (btnPreviewMic) {
                btnPreviewMic.disabled = true;
                btnPreviewMic.classList.add('opacity-50', 'cursor-not-allowed');
            }

            const videoSelectEl = document.getElementById('videoSource');
            const audioSelectEl = document.getElementById('audioSource');

            const savedAudioValue = audioSelectEl ? audioSelectEl.value : null;
            const savedVideoValue = videoSelectEl ? videoSelectEl.value : null;

            const originalVideoSelectContent = videoSelectEl ? videoSelectEl.innerHTML : null;
            const originalAudioSelectContent = audioSelectEl ? audioSelectEl.innerHTML : null;
            if (videoSelectEl) {
                videoSelectEl.innerHTML = '<option value="">Loading...</option>';
                videoSelectEl.disabled = true;
            }
            if (audioSelectEl) {
                audioSelectEl.disabled = true;
            }

            try {
                if (localStream) {
                    localStream.getTracks().forEach(track => track.stop());
                    if (localStream._originalStream) {
                         localStream._originalStream.getTracks().forEach(track => track.stop());
                    }
                    localStream = null;
                }

                const audioSource = savedAudioValue || (audioSelectEl ? audioSelectEl.value : null);
                const videoSource = savedVideoValue || (videoSelectEl ? videoSelectEl.value : null);

                savePreferences();

                const shouldGetVideo = !pendingCamToggle;

                const constraints = {
                    audio: {
                        deviceId: audioSource ? { exact: audioSource } : undefined,
                        echoCancellation: true,
                        noiseSuppression: false,
                        autoGainControl: true,
                        sampleRate: 48000
                    },
                    video: shouldGetVideo ? { deviceId: videoSource ? { exact: videoSource } : undefined } : false
                };

                let rawStream = await navigator.mediaDevices.getUserMedia(constraints);

                const newV = rawStream.getVideoTracks()[0];
                const newA = rawStream.getAudioTracks()[0];
                if (newA) newA.enabled = previousAudioEnabled;

                if (newV) {
                    if (pendingCamToggle) {
                        newV.enabled = false;
                    } else {
                        newV.enabled = previousVideoEnabled;
                    }
                }

                await setupVolumeMeter(rawStream, 'setupMicBar');

                 if (rawStream.getAudioTracks().length > 0) {
                     if (!audioContext) audioContext = new (window.AudioContext || window.webkitAudioContext)();

                     const workletLoaded = await initAudioWorklet();

                     if (audioContext.state === 'suspended') {
                         audioContext.resume().catch(e => {});
                     }

                     if (workletLoaded) {
                         const source = audioContext.createMediaStreamSource(rawStream);
                         const worklet = new AudioWorkletNode(audioContext, 'rnnoise-processor');
                         const dest = audioContext.createMediaStreamDestination();

                         source.connect(worklet);
                         worklet.connect(dest);

                         const processedAudio = dest.stream.getAudioTracks()[0];
                         if (processedAudio) processedAudio.enabled = previousAudioEnabled;

                         const videoTracks = rawStream.getVideoTracks();

                         localStream = new MediaStream([processedAudio, ...videoTracks]);
                         localStream._originalStream = rawStream;
                     } else {
                         localStream = rawStream;
                     }
                } else {
                    localStream = rawStream;
                }

                previewVideo.srcObject = localStream;
                updatePreviewButtons();

                if (ws && ws.readyState === WebSocket.OPEN) {
                    if (document.getElementById('localVideo')) document.getElementById('localVideo').srcObject = localStream;
                    updateLocalLabel();
                    updateLocalAvatar();

                    const btnMic = document.getElementById('btnMic');
                    const btnCam = document.getElementById('btnCam');
                    const micOffSvg = `<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="1" y1="1" x2="23" y2="23"></line><path d="M9 9v3a3 3 0 0 0 5.12 2.12M15 9.34V4a3 3 0 0 0-5.94-.6"></path><path d="M17 16.95A7 7 0 0 1 5 12v-2m14 0v2a7 7 0 0 1-.11 1.23"></path><line x1="12" x2="12" y1="19" y2="22"></line></svg>`;
                    const camOffSvg = `<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="1" y1="1" x2="23" y2="23"></line><path d="M21 21l-3.5-3.5m-2-2l-2-2m-2-2l-2-2m-2-2l-3.5-3.5"></path><path d="M15 7h5a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2h-5"></path><path d="M4 8v8a2 2 0 0 0 2 2h4.5"></path></svg>`;
                    const micOnSvg = `<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 2a3 3 0 0 0-3 3v7a3 3 0 0 0 6 0V5a3 3 0 0 0-3-3Z"/><path d="M19 10v2a7 7 0 0 1-14 0v-2"/><line x1="12" x2="12" y1="19" y2="22"/></svg>`;
                    const camOnSvg = `<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M14.5 4h-5L7 7H4a2 2 0 0 0-2 2v9a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2V9a2 2 0 0 0-2-2h-3l-2.5-3z"/><circle cx="12" cy="13" r="3"/></svg>`;

                    const audioTrack = localStream.getAudioTracks()[0];
                    let isMicOn = audioTrack && audioTrack.enabled;
                    if (pendingMicToggle) {
                        isMicOn = !isMicOn;
                    }
                    if (!isMicOn) {
                         if (btnMic) { btnMic.classList.add('active-red'); btnMic.innerHTML = micOffSvg; }
                    } else {
                         if (btnMic) { btnMic.classList.remove('active-red'); btnMic.innerHTML = micOnSvg; }
                    }

                    const videoTrack = localStream.getVideoTracks()[0];
                    let isCamOn = videoTrack && videoTrack.enabled;
                    if (pendingCamToggle) {
                        isCamOn = !isCamOn;
                    }
                    if (!isCamOn) {
                         if (btnCam) { btnCam.classList.add('active-red'); btnCam.innerHTML = camOffSvg; }
                    } else {
                         if (btnCam) { btnCam.classList.remove('active-red'); btnCam.innerHTML = camOnSvg; }
                    }

                    for (const userId in peers) {
                        const pc = peers[userId];
                        let negotiationNeeded = false;

                        if (audioTrack) {
                            const sender = pc.getSenders().find(s => s.track && s.track.kind === 'audio');
                            if (sender) {
                                sender.replaceTrack(audioTrack);
                            } else {
                                pc.addTrack(audioTrack, localStream);
                                negotiationNeeded = true;
                            }
                        }

                        if (videoTrack) {
                            const sender = pc.getSenders().find(s => s.track && s.track.kind === 'video');
                            if (sender) {
                                sender.replaceTrack(videoTrack);
                            } else {
                                pc.addTrack(videoTrack, localStream);
                                negotiationNeeded = true;
                            }
                        }

                        if (negotiationNeeded) {
                            negotiate(userId, pc);
                        }
                    }

                    if (videoTrack) {
                        let isCamOn = videoTrack.enabled;
                        if (isPreviewStarting && pendingCamToggle) {
                            isCamOn = !isCamOn;
                        }
                        ws.send(JSON.stringify({
                            type: 'cam-toggle',
                            data: { enabled: isCamOn }
                        }));
                    }
                }
            } catch (e) {
                console.error("Preview failed", e);
                document.getElementById('previewPlaceholder').style.display = 'flex';
                 try {
                    let rawStream = await navigator.mediaDevices.getUserMedia({ audio: true, video: false });

                    const newA = rawStream.getAudioTracks()[0];
                    if (newA) newA.enabled = previousAudioEnabled;

                    if (rawStream.getAudioTracks().length > 0) {
                         if (!audioContext) audioContext = new (window.AudioContext || window.webkitAudioContext)();

                         const workletLoaded = await initAudioWorklet();

                         if (audioContext.state === 'suspended') {
                             audioContext.resume().catch(e => {});
                         }

                         if (workletLoaded) {
                             const source = audioContext.createMediaStreamSource(rawStream);
                             const worklet = new AudioWorkletNode(audioContext, 'rnnoise-processor');
                             const dest = audioContext.createMediaStreamDestination();

                             source.connect(worklet);
                             worklet.connect(dest);

                             const processedAudio = dest.stream.getAudioTracks()[0];
                             if (processedAudio) processedAudio.enabled = previousAudioEnabled;

                             localStream = new MediaStream([processedAudio]);
                             localStream._originalStream = rawStream;
                         } else {
                             localStream = rawStream;
                         }
                    } else {
                        localStream = rawStream;
                    }

                    previewVideo.srcObject = null;
                    await setupVolumeMeter(localStream, 'setupMicBar');
                    updatePreviewButtons();
                } catch(e2) {
                    console.error("Mic fallback start err:", e2);
                    updatePreviewButtons();
                }
            } finally {
                isPreviewStarting = false;

                if (btnPreviewCam) {
                    btnPreviewCam.disabled = false;
                    btnPreviewCam.classList.remove('opacity-50', 'cursor-not-allowed');
                }
                if (btnPreviewMic) {
                    btnPreviewMic.disabled = false;
                    btnPreviewMic.classList.remove('opacity-50', 'cursor-not-allowed');
                }

                if (videoSelectEl && originalVideoSelectContent) {
                    videoSelectEl.innerHTML = originalVideoSelectContent;
                    if (savedVideoValue && [...videoSelectEl.options].some(o => o.value === savedVideoValue)) {
                        videoSelectEl.value = savedVideoValue;
                    }
                    videoSelectEl.disabled = false;
                }
                if (audioSelectEl && originalAudioSelectContent) {
                    audioSelectEl.innerHTML = originalAudioSelectContent;
                    if (savedAudioValue && [...audioSelectEl.options].some(o => o.value === savedAudioValue)) {
                        audioSelectEl.value = savedAudioValue;
                    }
                    audioSelectEl.disabled = false;
                }

                if (localStream) {
                    let needsUpdate = false;
                    if (pendingCamToggle) {
                        const videoTrack = localStream.getVideoTracks()[0];
                        if (videoTrack && videoTrack.enabled) {
                            videoTrack.enabled = false;
                            needsUpdate = true;
                        }
                    }
                    if (pendingMicToggle) {
                        const audioTrack = localStream.getAudioTracks()[0];
                        if (audioTrack && audioTrack.enabled) {
                            audioTrack.enabled = false;
                            needsUpdate = true;
                        }
                    }
                    if (needsUpdate) {
                        updatePreviewButtons();
                    }
                }
            }
        }

        function updatePreviewButtons() {
             const btnMic = document.getElementById('btnPreviewMic');
             const btnCam = document.getElementById('btnPreviewCam');

             if (!localStream) {
                 btnMic.disabled = true;
                 btnMic.classList.add('opacity-50', 'cursor-not-allowed');
                 btnMic.innerText = "No Mic";

                 btnCam.disabled = true;
                 btnCam.classList.add('opacity-50', 'cursor-not-allowed');
                 btnCam.innerText = "No Cam";
                 document.getElementById('previewPlaceholder').style.display = 'flex';
                 return;
             }

             const audioTrack = localStream.getAudioTracks()[0];
             const videoTrack = localStream.getVideoTracks()[0];

             if (!audioTrack) {
                 btnMic.disabled = true;
                 btnMic.classList.add('opacity-50', 'cursor-not-allowed');
                 btnMic.innerText = "No Mic";
             } else {
                 if (!isPreviewStarting) {
                     btnMic.disabled = false;
                     btnMic.classList.remove('opacity-50', 'cursor-not-allowed');
                 }

                 let isAudioEffectivelyEnabled = audioTrack.enabled;
                 if (pendingMicToggle) {
                     isAudioEffectivelyEnabled = !isAudioEffectivelyEnabled;
                 }

                 if (!isAudioEffectivelyEnabled) {
                     btnMic.classList.add('bg-red-500', 'hover:bg-red-600');
                     btnMic.innerText = "Unmute";
                 } else {
                     btnMic.classList.remove('bg-red-500', 'hover:bg-red-600');
                     btnMic.innerText = "Mute";
                 }
             }

             if (!videoTrack) {

                 if (!isPreviewStarting) {
                     btnCam.disabled = false;
                     btnCam.classList.remove('opacity-50', 'cursor-not-allowed');
                 }
                 btnCam.classList.add('bg-red-500', 'hover:bg-red-600');
                 btnCam.innerText = "Start Cam";
                 document.getElementById('previewPlaceholder').style.display = 'flex';
             } else {

                 if (!isPreviewStarting) {
                     btnCam.disabled = false;
                     btnCam.classList.remove('opacity-50', 'cursor-not-allowed');
                 }

                 let isEffectivelyEnabled = videoTrack.enabled;
                 if (pendingCamToggle) {
                     isEffectivelyEnabled = !isEffectivelyEnabled;
                 }

                 if (!isEffectivelyEnabled) {
                     btnCam.classList.add('bg-red-500', 'hover:bg-red-600');
                     btnCam.innerText = "Start Cam";
                     document.getElementById('previewPlaceholder').style.display = 'flex';
                 } else {
                     btnCam.classList.remove('bg-red-500', 'hover:bg-red-600');
                     btnCam.innerText = "Stop Cam";
                     document.getElementById('previewPlaceholder').style.display = 'none';
                 }
             }
        }

        function togglePreviewMic() {
             if (isPreviewStarting) {
                 pendingMicToggle = !pendingMicToggle;

                 const btnMic = document.getElementById('btnPreviewMic');
                 if (btnMic) {
                    if (btnMic.innerText.includes("Mute") && !btnMic.innerText.includes("Unmute")) {
                        btnMic.classList.add('bg-red-500', 'hover:bg-red-600');
                        btnMic.innerText = "Unmute";
                    } else {
                        btnMic.classList.remove('bg-red-500', 'hover:bg-red-600');
                        btnMic.innerText = "Mute";
                    }
                 }
                 savePreferences();
                 return;
             }
             if (!localStream) return;
            const track = localStream.getAudioTracks()[0];
            if (track) {
                track.enabled = !track.enabled;
                updatePreviewButtons();
                savePreferences();
            }
        }

        function togglePreviewCam() {
             if (isPreviewStarting) {
                 pendingCamToggle = !pendingCamToggle;

                 const btnCam = document.getElementById('btnPreviewCam');
                 const videoTrack = localStream ? localStream.getVideoTracks()[0] : null;
                 const willBeEnabled = videoTrack ? !videoTrack.enabled : !pendingCamToggle;

                 if (btnCam) {
                    if (!willBeEnabled) {
                        btnCam.classList.add('bg-red-500', 'hover:bg-red-600');
                        btnCam.innerText = "Start Cam";
                        document.getElementById('previewPlaceholder').style.display = 'flex';
                    } else {
                        btnCam.classList.remove('bg-red-500', 'hover:bg-red-600');
                        btnCam.innerText = "Stop Cam";
                        document.getElementById('previewPlaceholder').style.display = 'none';
                    }
                 }
                 savePreferences();
                 return;
             }
             if (!localStream) return;

             const videoTrack = localStream.getVideoTracks()[0];
             const btnCam = document.getElementById('btnPreviewCam');

             if (videoTrack) {

                 btnCam.disabled = true;
                 btnCam.innerHTML = `<svg class="spinner" xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M21 12a9 9 0 1 1-6.219-8.56"/></svg>`;

                 videoTrack.stop();
                 localStream.removeTrack(videoTrack);
                 pendingCamToggle = true;

                 btnCam.disabled = false;
                 updatePreviewButtons();
                 savePreferences();
             } else {

                 (async () => {

                     if (btnCam) {
                         btnCam.innerHTML = `<svg class="spinner" xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M21 12a9 9 0 1 1-6.219-8.56"/></svg>`;
                         btnCam.disabled = true;
                     }

                     try {
                         const videoSource = videoSelect.value;
                         const constraints = {
                             video: { deviceId: videoSource ? { exact: videoSource } : undefined }
                         };
                         const newStream = await navigator.mediaDevices.getUserMedia(constraints);
                         const newTrack = newStream.getVideoTracks()[0];

                         if (!newTrack || newTrack.readyState !== 'live') {
                             console.warn("Camera track not properly initialized, retrying...");
                             newTrack?.stop();
                             if (newTrack && localStream.getVideoTracks().includes(newTrack)) {
                                 localStream.removeTrack(newTrack);
                             }
                             await new Promise(r => setTimeout(r, 100));
                             const retryStream = await navigator.mediaDevices.getUserMedia(constraints);
                             const retryTrack = retryStream.getVideoTracks()[0];
                             if (retryTrack) {
                                 retryTrack.enabled = true;
                                 localStream.addTrack(retryTrack);
                                 retryStream.getTracks().forEach(t => { if (t !== retryTrack) t.stop(); });
                             }
                         } else {
                             newTrack.enabled = true;
                             localStream.addTrack(newTrack);
                         }

                         pendingCamToggle = false;
                         previewVideo.srcObject = localStream;
                         updatePreviewButtons();
                         savePreferences();
                     } catch (e) {
                         console.error("Could not add camera", e);
                         alert("Could not access camera. Please check permissions.");
                         updatePreviewButtons();
                     }
                 })();
             }
        }

        async function joinRoom() {

            hasLeftRoom = false;

            userNickname = nicknameInput.value.trim() || "Guest";
            savePreferences();

            if (!audioContext) {
                audioContext = new (window.AudioContext || window.webkitAudioContext)();
            }
            if (audioContext.state === 'suspended') {
                await audioContext.resume();
            }

            previewVideo.srcObject = null;

            if (setupMeterFrameId) cancelAnimationFrame(setupMeterFrameId);
            configOverlay.classList.add('opacity-0', 'pointer-events-none');
            setTimeout(() => {
                configOverlay.style.display = 'none';
                appLayout.classList.remove('hidden');
                appLayout.classList.add('flex');
                document.getElementById('sidebarToggle').classList.remove('hidden');
                applySidebarState(true);
            }, 300);

            const videoTrack = localStream ? localStream.getVideoTracks()[0] : null;
            if (localVideo) {
                if (videoTrack && videoTrack.enabled) {
                    localVideo.srcObject = localStream;
                } else {
                    localVideo.srcObject = null;
                }
            }

            updateLocalLabel();
            updateLocalAvatar();
            const btnMic = document.getElementById('btnMic');
            const btnCam = document.getElementById('btnCam');

            const micOffSvg = `<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="1" y1="1" x2="23" y2="23"></line><path d="M9 9v3a3 3 0 0 0 5.12 2.12M15 9.34V4a3 3 0 0 0-5.94-.6"></path><path d="M17 16.95A7 7 0 0 1 5 12v-2m14 0v2a7 7 0 0 1-.11 1.23"></path><line x1="12" x2="12" y1="19" y2="22"></line></svg>`;
            const camOffSvg = `<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="1" y1="1" x2="23" y2="23"></line><path d="M21 21l-3.5-3.5m-2-2l-2-2m-2-2l-2-2m-2-2l-3.5-3.5"></path><path d="M15 7h5a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2h-5"></path><path d="M4 8v8a2 2 0 0 0 2 2h4.5"></path></svg>`;

             if (localStream) {
                const audioTrack = localStream.getAudioTracks()[0];
                const videoTrack = localStream.getVideoTracks()[0];

                const micOnSvg = `<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 2a3 3 0 0 0-3 3v7a3 3 0 0 0 6 0V5a3 3 0 0 0-3-3Z"/><path d="M19 10v2a7 7 0 0 1-14 0v-2"/><line x1="12" x2="12" y1="19" y2="22"/></svg>`;
                const camOnSvg = `<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M14.5 4h-5L7 7H4a2 2 0 0 0-2 2v9a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2V9a2 2 0 0 0-2-2h-3l-2.5-3z"/><circle cx="12" cy="13" r="3"/></svg>`;

                if (!audioTrack || !audioTrack.enabled) {
                     btnMic.classList.add('active-red');
                     btnMic.innerHTML = micOffSvg;
                } else {
                     btnMic.classList.remove('active-red');
                     btnMic.innerHTML = micOnSvg;
                }

                if (!videoTrack || !videoTrack.enabled) {
                     btnCam.classList.add('active-red');
                     btnCam.innerHTML = camOffSvg;
                } else {
                     btnCam.classList.remove('active-red');
                     btnCam.innerHTML = camOnSvg;
                }

                await setupAudioMonitor(localStream, 'local');
            } else {
                 btnMic.classList.add('active-red');
                 btnMic.innerHTML = micOffSvg;
                 btnCam.classList.add('active-red');
                 btnCam.innerHTML = camOffSvg;
            }

            if (isDeafened) {
                const btnDeafen = document.getElementById('btnDeafen');
                const deafenOffSvg = `<svg xmlns="http://www.w3.org/2000/svg" width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="1" y1="1" x2="23" y2="23"></line><path d="M21 14a2 2 0 0 0-2-2h-3a2 2 0 0 0-2 2v3a2 2 0 0 0 2 2h1a2 2 0 0 0 2-2V14z"></path><path d="M3 14a2 2 0 0 1 2-2h3a2 2 0 0 1 2 2v3a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V14z"></path><path d="M20.4 10.4C20.2 6.5 17 3.5 13 3.1"></path><path d="M6.5 5.5A9 9 0 0 0 3 12"></path></svg>`;
                if (btnDeafen) {
                    btnDeafen.classList.add('active-red');
                    btnDeafen.innerHTML = deafenOffSvg;
                }

                document.querySelectorAll('video, audio').forEach(el => {
                    if (el.id !== 'localVideo' && el.id !== 'previewVideo') {
                        el.muted = true;
                    }
                });
            }

            connectWs();

            sessionStorage.setItem('rustrooms_setup_done', 'true');

            window.addEventListener('offline', () => {
                console.warn('Network connection lost (offline)');
                updateStatus('disconnected', 'Network Offline');

                updateConnectionStatus();
            });

            window.addEventListener('online', () => {

                if (hasLeftRoom) {
                    console.log('User left the room, not reconnecting on network restore');
                    return;
                }

                if (isReconnecting) {
                    console.log('Already reconnecting, skipping network restore trigger');
                    return;
                }

                console.log('Network connection restored (online)');
                updateStatus('connecting', 'Reconnecting...');

                reconnectionAttempts = 0;
                connectWs();
            });

            await requestWakeLock();
        }

        const welcomeOverlay = document.getElementById('welcomeOverlay');

        function playNotificationSound(type) {
            if (!audioContext) return;
            if (audioContext.state === 'suspended') audioContext.resume();

            const osc = audioContext.createOscillator();
            const gain = audioContext.createGain();

            osc.connect(gain);
            gain.connect(audioContext.destination);

            const now = audioContext.currentTime;

            if (type === 'join') {
                osc.type = 'sine';
                osc.frequency.setValueAtTime(523.25, now);
                osc.frequency.exponentialRampToValueAtTime(783.99, now + 0.1);

                gain.gain.setValueAtTime(0.1, now);
                gain.gain.exponentialRampToValueAtTime(0.001, now + 0.5);

                osc.start(now);
                osc.stop(now + 0.5);
            } else if (type === 'leave') {
                osc.type = 'sine';
                osc.frequency.setValueAtTime(440, now);
                osc.frequency.exponentialRampToValueAtTime(220, now + 0.2);

                gain.gain.setValueAtTime(0.1, now);
                gain.gain.exponentialRampToValueAtTime(0.001, now + 0.3);

                osc.start(now);
                osc.stop(now + 0.3);
            } else if (type === 'disconnect') {
                osc.type = 'sine';
                osc.frequency.setValueAtTime(600, now);
                osc.frequency.exponentialRampToValueAtTime(200, now + 0.2);

                gain.gain.setValueAtTime(0.1, now);
                gain.gain.exponentialRampToValueAtTime(0.001, now + 0.3);

                osc.start(now);
                osc.stop(now + 0.3);
            } else if (type === 'mute') {
                 osc.type = 'sine';
                 osc.frequency.setValueAtTime(400, now);
                 gain.gain.setValueAtTime(0.1, now);
                 gain.gain.exponentialRampToValueAtTime(0.001, now + 0.1);
                 osc.start(now);
                 osc.stop(now + 0.1);
            } else if (type === 'unmute') {
                 osc.type = 'sine';
                 osc.frequency.setValueAtTime(800, now);
                 gain.gain.setValueAtTime(0.1, now);
                 gain.gain.exponentialRampToValueAtTime(0.001, now + 0.1);
                 osc.start(now);
                 osc.stop(now + 0.1);
            }
        }

        function updateStatus(state, message) {
            statusText.innerText = message;
            connectionDot.className = 'connection-dot ' + state;
        }

        function updateConnectionStatus() {

            const peerIds = Object.keys(peers);
            let hasConnectedPeers = false;
            let hasConnectingPeers = false;

            for (const userId of peerIds) {
                const pc = peers[userId];
                if (pc) {
                    const iceState = pc.iceConnectionState;
                    const connState = pc.connectionState;

                    if (iceState === 'connected' || iceState === 'completed') {
                        hasConnectedPeers = true;
                    } else if (iceState === 'checking' || iceState === 'new') {
                        hasConnectingPeers = true;
                    }
                }
            }

            if (peerIds.length > 0 && !hasConnectedPeers && !hasConnectingPeers) {
                updateStatus('disconnected', 'Connection Lost');
            } else if (hasConnectedPeers) {
                updateStatus('connected', 'Connected');
            }
        }

        function toggleSidebar() {
            const body = document.body;
            const sidebar = document.getElementById('roomSidebar');
            const overlay = document.getElementById('sidebarOverlay');
            const sidebarToggle = document.getElementById('sidebarToggle');
            const isDesktop = window.innerWidth >= 768;
            const wasOpen = sidebar.classList.contains('open');

            sidebar.classList.toggle('open');

            if (isDesktop) {
                overlay.classList.remove('open');
            } else {
                overlay.classList.toggle('open');
            }

            const isOpen = sidebar.classList.contains('open');
            if (isOpen) {
                body.classList.add('sidebar-open');
                sidebarToggle.classList.add('hidden');

                if (isDesktop) {
                    const pip = document.getElementById('localPipWrapper');
                    if (pip) {
                        const pipRect = pip.getBoundingClientRect();
                        const sidebarWidth = 320;
                        const margin = 16;

                        if (pipRect.left < sidebarWidth + margin) {

                            const newLeft = sidebarWidth + margin;
                            pip.style.left = newLeft + 'px';
                            pip.style.bottom = '';
                            pip.style.right = '';
                        }
                    }
                }
            } else {
                body.classList.remove('sidebar-open');
                sidebarToggle.classList.remove('hidden');

                const pip = document.getElementById('localPipWrapper');
                if (pip) {
                    pip.style.left = '';
                    pip.style.right = '';
                    pip.style.bottom = '';
                }
            }
            localStorage.setItem('rustrooms_sidebar_open', isOpen ? 'true' : 'false');
        }

        let lastViewportWasDesktop = window.innerWidth >= 768;
        window.addEventListener('resize', () => {
            const isDesktop = window.innerWidth >= 768;
            if (isDesktop !== lastViewportWasDesktop) {
                lastViewportWasDesktop = isDesktop;

                const body = document.body;
                const sidebar = document.getElementById('roomSidebar');
                const overlay = document.getElementById('sidebarOverlay');
                const sidebarToggle = document.getElementById('sidebarToggle');
                const isOpen = sidebar.classList.contains('open');

                if (isOpen) {
                    if (isDesktop) {
                        overlay.classList.remove('open');
                        body.classList.add('sidebar-open');
                        sidebarToggle.classList.add('hidden');

                        const pip = document.getElementById('localPipWrapper');
                        if (pip) {
                            const pipRect = pip.getBoundingClientRect();
                            const sidebarWidth = 320;
                            const margin = 16;

                            if (pipRect.left < sidebarWidth + margin) {
                                const newLeft = sidebarWidth + margin;
                                pip.style.left = newLeft + 'px';
                                pip.style.bottom = '';
                                pip.style.right = '';
                            }
                        }
                    } else {
                        overlay.classList.add('open');
                        body.classList.add('sidebar-open');
                        sidebarToggle.classList.add('hidden');
                    }
                }
            }
        });

        function applySidebarState(noTransition = false) {
            const savedState = localStorage.getItem('rustrooms_sidebar_open');
            const isOpen = savedState === 'true';
            const isDesktop = window.innerWidth >= 768;
            const sidebarToggle = document.getElementById('sidebarToggle');

            if (isOpen) {
                const body = document.body;
                const sidebar = document.getElementById('roomSidebar');
                const overlay = document.getElementById('sidebarOverlay');

                if (noTransition) {
                    sidebar.style.transition = 'none';
                }
                body.classList.add('sidebar-open');
                sidebar.classList.add('open');
                sidebarToggle.classList.add('hidden');

                if (isDesktop) {
                    overlay.classList.remove('open');

                    const pip = document.getElementById('localPipWrapper');
                    if (pip) {
                        const pipRect = pip.getBoundingClientRect();
                        const sidebarWidth = 320;
                        const margin = 16;

                        if (pipRect.left < sidebarWidth + margin) {

                            const newLeft = sidebarWidth + margin;
                            pip.style.left = newLeft + 'px';
                            pip.style.bottom = '';
                            pip.style.right = '';
                        }
                    }
                }

                if (noTransition) {

                    sidebar.offsetHeight;

                    setTimeout(() => {
                        sidebar.style.transition = '';
                    }, 50);
                }
            } else {
                sidebarToggle.classList.remove('hidden');
            }
        }

        function showNameModal(title, placeholder, callback) {
            const modal = document.getElementById('nameModal');
            const modalTitle = document.getElementById('modalTitle');
            const modalInput = document.getElementById('modalInput');
            const modalSubmit = document.getElementById('modalSubmit');

            modalTitle.innerText = title;
            modalInput.placeholder = placeholder;
            modalInput.value = '';
            modal.classList.add('open');
            setTimeout(() => modalInput.focus(), 100);

            modalSubmit.onclick = () => {
                const name = modalInput.value.trim();
                callback(name);
                closeNameModal();
            };

            const handleEnter = (e) => {
                if (e.key === 'Enter') {
                    modalSubmit.click();
                    modalInput.removeEventListener('keydown', handleEnter);
                }
            };
            modalInput.addEventListener('keydown', handleEnter);
        }

        function closeNameModal() {
            const modal = document.getElementById('nameModal');
            modal.classList.remove('open');
        }

        function showCustomAlert(title, message) {
            document.getElementById('alertTitle').innerText = title;
            document.getElementById('alertMessage').innerText = message;
            document.getElementById('alertModal').classList.add('open');
        }

        function closeCustomAlert() {
            document.getElementById('alertModal').classList.remove('open');
        }

        function showPasswordModal(title, message, callback) {
            const modal = document.getElementById('passwordModal');
            const modalTitle = document.getElementById('passwordModalTitle');
            const modalMessage = document.getElementById('passwordModalMessage');
            const modalInput = document.getElementById('passwordModalInput');
            const modalSubmit = document.getElementById('passwordModalSubmit');

            modalTitle.innerText = title;
            modalMessage.innerText = message || "";
            modalInput.value = '';
            modal.classList.add('open');
            setTimeout(() => modalInput.focus(), 100);

            modalSubmit.onclick = () => {
                const pass = modalInput.value;
                callback(pass);
                closePasswordModal();
            };
        }

        function closePasswordModal() {
            const modal = document.getElementById('passwordModal');
            modal.classList.remove('open');
        }

        function showCustomConfirm(title, message, onConfirm) {
            document.getElementById('confirmTitle').innerText = title;
            document.getElementById('confirmMessage').innerText = message;
            const modal = document.getElementById('confirmModal');
            const submitBtn = document.getElementById('confirmSubmit');

            const newBtn = submitBtn.cloneNode(true);
            submitBtn.parentNode.replaceChild(newBtn, submitBtn);

            newBtn.onclick = () => {
                onConfirm();
                closeCustomConfirm();
            };

            modal.classList.add('open');
        }

        function closeCustomConfirm() {
            document.getElementById('confirmModal').classList.remove('open');
        }

        let userClickTracker = {};
        let pendingKickUserId = null;
        let pendingKickUserNickname = null;

        function handleUserClick(el) {
            const userId = el.dataset.userId;
            const nickname = el.dataset.userNickname;

            if (!userId || userId === persistentUserId) return;

            const now = Date.now();
            const windowMs = 5000;
            const threshold = 10;

            if (!userClickTracker[userId]) {
                userClickTracker[userId] = [];
            }

            userClickTracker[userId] = userClickTracker[userId].filter(timestamp => now - timestamp < windowMs);
            userClickTracker[userId].push(now);

            if (userClickTracker[userId].length >= threshold) {
                userClickTracker[userId] = [];
                showKickModal(userId, nickname);
            }
        }

        function showKickModal(userId, nickname) {
            const modal = document.getElementById('kickModal');
            const title = document.getElementById('kickTitle');
            const message = document.getElementById('kickMessage');
            const submitBtn = document.getElementById('kickSubmit');

            pendingKickUserId = userId;
            pendingKickUserNickname = nickname;

            title.textContent = 'Kick User';
            message.textContent = `Are you sure you want to kick "${nickname}" from the room?`;

            submitBtn.onclick = () => {
                if (pendingKickUserId && ws && ws.readyState === WebSocket.OPEN) {
                    ws.send(JSON.stringify({
                        type: 'kick-user',
                        data: { userId: pendingKickUserId }
                    }));
                }
                closeKickModal();
            };

            modal.classList.add('open');
        }

        function closeKickModal() {
            document.getElementById('kickModal').classList.remove('open');
            pendingKickUserId = null;
            pendingKickUserNickname = null;
        }

        let roomDragState = {
            draggedRid: null
        };

        function handleRoomDragStart(e, rid) {
            roomDragState.draggedRid = rid;
            e.dataTransfer.effectAllowed = 'move';
            e.target.closest('.room-item').classList.add('opacity-50');
        }

        function handleRoomDragEnd(e) {
            e.target.closest('.room-item').classList.remove('opacity-50');
            document.querySelectorAll('.room-item').forEach(el => el.classList.remove('border-t-2', 'border-blue-500'));
        }

        function handleRoomDragOver(e) {
            e.preventDefault();
            e.dataTransfer.dropEffect = 'move';
            const roomItem = e.target.closest('.room-item');
            if (roomItem && roomItem.dataset.rid !== roomDragState.draggedRid) {
                roomItem.classList.add('border-t-2', 'border-blue-500');
            }
        }

        function handleRoomDragLeave(e) {
            const roomItem = e.target.closest('.room-item');
            if (roomItem) {
                roomItem.classList.remove('border-t-2', 'border-blue-500');
            }
        }

        function handleRoomDrop(e, targetRid) {
            e.preventDefault();
            const draggedRid = roomDragState.draggedRid;
            if (draggedRid === targetRid) return;

            let order = JSON.parse(localStorage.getItem('rustrooms_room_order_' + roomId) || '[]');
            const currentRids = Object.keys(globalRoomList);
            if (order.length === 0) order = currentRids.sort();

            const fromIndex = order.indexOf(draggedRid);
            const toIndex = order.indexOf(targetRid);

            if (fromIndex !== -1 && toIndex !== -1) {
                order.splice(fromIndex, 1);
                order.splice(toIndex, 0, draggedRid);
                localStorage.setItem('rustrooms_room_order_' + roomId, JSON.stringify(order));
                updateRoomListUI();
            }
        }

        async function createNewRoom() {
            showNameModal("Start New Room", "Enter room name (optional)", (name) => {
                window.location.href = `/${name ? encodeURIComponent(name) : crypto.randomUUID()}/General`;
            });
        }

        async function createNewChannel() {
            showNameModal("Create New Channel", "Enter channel name", (name) => {
                if (!name) return;
                performChannelSwitch(roomId, encodeURIComponent(name));
            });
        }

        async function performChannelSwitch(newRoomId, newChannelId) {
            if (newChannelId && newChannelId.length > 32) newChannelId = newChannelId.substring(0, 32);

            if (ws) {
                ws.onclose = null;
                ws.close();

                await new Promise(resolve => setTimeout(resolve, 200));
            }
            stopHeartbeat();

            for (const userId in peers) {
                removePeer(userId);
            }
            peers = {};
            peerCamStatus = {};
            peerScreenStatus = {};
            peerScreenHasAudio = {};
            peerMicTrackId = {};
            peerScreenAudioTrackId = {};
            remoteGrid.innerHTML = '';

            roomId = newRoomId;
            channelId = newChannelId;

            const channelNameEl = document.getElementById('currentChannelName');
            if (channelNameEl) {
                channelNameEl.innerText = `# ${channelId}`;
            }

            const newUrl = `/${roomId}/${channelId}`;
            if (window.location.pathname !== newUrl) {
                history.pushState({ roomId, channelId }, "", newUrl);
            }

            wsUrl = `${wsProtocol}//${currentServerHost}/ws/${roomId}/${channelId}`;
            updateStatus('connecting', 'Connecting...');

            if (typeof updateRoomListUI === 'function') updateRoomListUI();

            reconnectionAttempts = 0;
            isReconnecting = false;
            connectWs();
        }

        function switchChannel(newChannelId) {
            if (newChannelId === channelId) return;
            performChannelSwitch(roomId, newChannelId);
        }

        function switchRoom(newRoomId) {
            if (newRoomId === roomId) return;
            performChannelSwitch(newRoomId, 'General');
        }

        window.onpopstate = function(event) {
            const parts = window.location.pathname.split('/').filter(p => p !== '');
            const newRoomId = parts[0] || '';
            const newChannelId = parts[1] || (newRoomId ? 'general' : '');

            if (newRoomId && (newRoomId !== roomId || newChannelId !== channelId)) {
                performChannelSwitch(newRoomId, newChannelId);
            } else if (!newRoomId) {
                window.location.reload();
            }
        };

        function renameRoom(targetRoomId, event) {
            if (event) event.stopPropagation();

            if (targetRoomId.toLowerCase() === 'general') {
                showCustomAlert("Action Not Allowed", "Cannot rename the General room.");
                return;
            }

            const roomData = globalRoomList[targetRoomId];
            if (roomData && roomData.users && Object.keys(roomData.users).length > 0) {
                showCustomAlert("Room Not Empty", "You cannot rename a room that still has users in it.");
                return;
            }

            showNameModal("Rename Channel", "Enter new name", (newName) => {
                if (!newName) return;
                if (ws && ws.readyState === WebSocket.OPEN) {
                    ws.send(JSON.stringify({
                        type: 'rename-channel',
                        data: { channelId: targetRoomId, newName: newName }
                    }));
                }
            });
        }

        function deleteRoom(targetRoomId, event) {
            if (event) event.stopPropagation();

            if (targetRoomId.toLowerCase() === 'general') {
                showCustomAlert("Action Not Allowed", "Cannot delete the General room.");
                return;
            }

            const roomData = globalRoomList[targetRoomId];
            if (roomData && roomData.users && Object.keys(roomData.users).length > 0) {
                showCustomAlert("Room Not Empty", "You cannot delete a room that still has users in it.");
                return;
            }

            showCustomConfirm("Delete Channel", `Delete "${targetRoomId}"? This cannot be undone.`, () => {
                if (ws && ws.readyState === WebSocket.OPEN) {
                    ws.send(JSON.stringify({
                        type: 'delete-channel',
                        data: { channelId: targetRoomId }
                    }));
                }
            });
        }

        function updateRoomListUI() {
            const container = document.getElementById('roomListContainer');
            if (!container) return;

            container.innerHTML = '';

            let order = JSON.parse(localStorage.getItem('rustrooms_room_order_' + roomId) || '[]');
            const currentRids = Object.keys(globalRoomList);

            order = order.filter(rid => currentRids.includes(rid));
            currentRids.forEach(rid => {
                if (!order.includes(rid)) order.push(rid);
            });

            order.forEach(rid => {
                const roomInfo = globalRoomList[rid];
                if (!roomInfo) return;
                const isActive = (rid === channelId);

                const roomEl = document.createElement('div');
                roomEl.className = `room-item ${isActive ? 'active' : ''}`;
                roomEl.draggable = true;
                roomEl.dataset.rid = rid;

                roomEl.onclick = () => switchChannel(rid);

                roomEl.ondragstart = (e) => handleRoomDragStart(e, rid);
                roomEl.ondragend = (e) => handleRoomDragEnd(e);
                roomEl.ondragover = (e) => handleRoomDragOver(e);
                roomEl.ondragleave = (e) => handleRoomDragLeave(e);
                roomEl.ondrop = (e) => handleRoomDrop(e, rid);

                let usersHtml = '';
                const users = roomInfo.users || {};
                const userIds = Object.keys(users);

                userIds.forEach(uid => {
                    const u = users[uid];
                    const isMuted = u.isMuted;
                    const isDeafened = u.isDeafened;
                    const isScreenSharing = u.isScreenSharing === true;

                    usersHtml += `
                        <div class="room-user-row pointer-events-auto cursor-pointer" data-user-id="${uid}" data-user-nickname="${u.nickname.replace(/"/g, '&quot;')}" onclick="handleUserClick(this)">
                            <div class="mini-avatar">
                                ${u.avatar ? `<img src="${u.avatar}">` : `<div class="mini-avatar-placeholder">${u.nickname.charAt(0).toUpperCase()}</div>`}
                            </div>
                            <span class="room-user-name">${u.nickname}</span>
                            <div class="status-indicators">
                                ${isScreenSharing ? `
                                    <div class="status-icon active" style="color: #10b981;" title="Screen Sharing">
                                        <svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><rect x="2" y="3" width="20" height="14" rx="2" ry="2"></rect><line x1="8" y1="21" x2="16" y2="21"></line><line x1="12" y1="17" x2="12" y2="21"></line></svg>
                                    </div>
                                ` : ''}
                                ${isMuted || isDeafened ? `
                                    <div class="status-icon active" title="${isDeafened ? 'Deafened' : 'Muted'}">
                                        ${isDeafened ? `
                                            <svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><line x1="1" y1="1" x2="23" y2="23"></line><path d="M21 14a2 2 0 0 0-2-2h-3a2 2 0 0 0-2 2v3a2 2 0 0 0 2 2h1a2 2 0 0 0 2-2V14z"></path><path d="M3 14a2 2 0 0 1 2-2h3a2 2 0 0 1 2 2v3a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V14z"></path><path d="M20.4 10.4C20.2 6.5 17 3.5 13 3.1"></path><path d="M6.5 5.5A9 9 0 0 0 3 12"></path></svg>
                                        ` : `
                                            <svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><line x1="1" y1="1" x2="23" y2="23"></line><path d="M9 9v3a3 3 0 0 0 5.12 2.12M15 9.34V4a3 3 0 0 0-5.94-.6"></path><path d="M17 16.95A7 7 0 0 1 5 12v-2m14 0v2a7 7 0 0 1-.11 1.23"></path></svg>
                                        `}
                                    </div>
                                ` : ''}
                            </div>
                        </div>
                    `;
                });

                roomEl.innerHTML = `
                    <div class="room-name pointer-events-none">
                        <span class="truncate pr-2">${roomInfo.name}</span>
                        <div class="flex items-center gap-2">
                             <div class="user-count">${userIds.length}</div>
                             ${rid.toLowerCase() !== 'general' ? `
                                <div class="flex gap-1 pointer-events-auto">
                                    <button onclick="renameRoom(this.closest('.room-item').dataset.rid, event)" class="p-1 text-zinc-500 hover:text-blue-500 transition-colors" title="Rename Channel">
                                        <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M11 4H4a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7"></path><path d="M18.5 2.5a2.121 2.121 0 0 1 3 3L12 15l-4 1 1-4 9.5-9.5z"></path></svg>
                                    </button>
                                    <button onclick="deleteRoom(this.closest('.room-item').dataset.rid, event)" class="p-1 text-zinc-500 hover:text-red-500 transition-colors" title="Delete Channel">
                                        <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="3 6 5 6 21 6"></polyline><path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"></path><line x1="10" y1="11" x2="10" y2="17"></line><line x1="14" y1="11" x2="14" y2="17"></line></svg>
                                    </button>
                                </div>
                             ` : ''}
                        </div>
                    </div>
                    <div class="room-users flex flex-col gap-1 mt-2 pointer-events-none">
                        ${usersHtml}
                        ${userIds.length === 0 ? '<span class="text-[10px] text-zinc-600 italic px-2">Empty</span>' : ''}
                    </div>
                `;

                container.appendChild(roomEl);
            });
        }

        async function createRoom() {
            try {
                const res = await fetch('/new');
                if (res.status === 401) {
                    const btn = document.getElementById('btnStartRoom');
                    const pw = document.getElementById('passwordInputContainer');
                    const input = document.getElementById('roomPasswordInput');

                    btn.classList.add('opacity-0', 'pointer-events-none', 'scale-90');
                    pw.classList.remove('opacity-0', 'pointer-events-none', 'translate-y-4');
                    pw.classList.add('translate-y-0');

                    setTimeout(() => input.focus(), 100);
                } else if (res.ok) {
                    window.location.href = `/${crypto.randomUUID()}/General`;
                } else {
                    alert("Error creating room");
                }
            } catch (e) {
                console.error(e);
                alert("Error creating room");
            }
        }

        async function submitPassword() {
            const input = document.getElementById('roomPasswordInput');
            const password = input.value;
            if (!password) return;

            try {

                sessionStorage.setItem('rustrooms_room_password', password);

                const res = await fetch('/new?password=' + encodeURIComponent(password));
                 if (res.ok) {
                     window.location.href = `/${crypto.randomUUID()}/General`;
                 } else if (res.status === 401) {
                     sessionStorage.removeItem('rustrooms_room_password');
                     input.classList.add('ring-2', 'ring-red-500', 'border-red-500');
                     setTimeout(() => input.classList.remove('ring-2', 'ring-red-500', 'border-red-500'), 500);
                     input.value = '';
                     input.placeholder = "Incorrect Password";
                 } else {
                     sessionStorage.removeItem('rustrooms_room_password');
                     alert("Error creating room");
                 }
            } catch (e) {
                console.error(e);
                sessionStorage.removeItem('rustrooms_room_password');
                alert("Error creating room");
            }
        }

        if (roomId) {
            loadPreferences();
            const setupDone = sessionStorage.getItem('rustrooms_setup_done') === 'true';
            if (setupDone && roomId) {

                loadDevices().then(() => findBestServer().then(() => joinRoom()));
            } else {
                configOverlay.classList.remove('hidden');
                configOverlay.classList.remove('opacity-0');
                loadDevices();
            }
        } else {
            welcomeOverlay.style.display = 'flex';
        }

        function connectWs() {
            updateStatus('connecting', 'Connecting...');
            ws = new WebSocket(wsUrl);

                        ws.onopen = () => {

                            if (reconnectStatusTimeout) {
                                clearTimeout(reconnectStatusTimeout);
                                reconnectStatusTimeout = null;
                            }

                            playNotificationSound('join');
                            reconnectionAttempts = 0;
                            isReconnecting = false;
                            failoverIndex = 0;
                            updateStatus('connected', 'Connected');
                            startHeartbeat();
                            fetchNodeId(currentServerHost);
                            discoverAndRankPeers().then(peers => { rankedPeers = peers; });
                            const camEnabled = localStream && localStream.getVideoTracks()[0] && localStream.getVideoTracks()[0].enabled;
                            const screenEnabled = !!screenStream;
                            const screenHasAudio = screenStream && screenStream.getAudioTracks().length > 0;
                            const audioTrack = localStream && localStream.getAudioTracks()[0];
                            const isMuted = !audioTrack || !audioTrack.enabled;

                                    ws.send(JSON.stringify({
                                type: "join",
                                data: {
                                    userId: persistentUserId,
                                    nickname: userNickname,
                                    avatar: userAvatar,
                                    camEnabled: camEnabled,
                                    screenEnabled: screenEnabled,
                                    screenAudio: screenHasAudio,
                                    micTrackId: audioTrack ? audioTrack.id : null,
                                    screenAudioTrackId: screenStream ? (screenStream.getAudioTracks()[0]?.id || null) : null,
                                    isMuted: isMuted,
                                    isDeafened: isDeafened,
                                    password: roomCreationPassword
                                }
                            }));
                            checkEmpty();
                        };

                        ws.onmessage = async (event) => {
                            const msg = JSON.parse(event.data);

                            switch (msg.type) {
                                case 'error':
                                    if (msg.data && msg.data.code === 'PASSWORD_REQUIRED') {

                                        awaitingPassword = true;
                                        hasLeftRoom = true;
                                        isReconnecting = false;
                                        if (reconnectStatusTimeout) {
                                            clearTimeout(reconnectStatusTimeout);
                                            reconnectStatusTimeout = null;
                                        }

                                        const modal = document.getElementById('passwordModal');
                                        if (modal && !modal.classList.contains('open')) {
                                            showPasswordModal("Room Creation Password", msg.data.message || "Password required to create this room:", (pass) => {
                                                if (pass) {
                                                    roomCreationPassword = pass;
                                                    sessionStorage.setItem('rustrooms_room_password', pass);
                                                    awaitingPassword = false;
                                                    hasLeftRoom = false;
                                                    reconnectionAttempts = 0;
                                                    isReconnecting = false;
                                                    connectWs();
                                                } else {
                                                    hasLeftRoom = true;
                                                    window.location.href = "/";
                                                }
                                            });
                                        }
                                    } else {
                                        showCustomAlert("Error", msg.data.message || "An error occurred.");
                                    }
                                    break;
                                case 'room-list':
                                    globalRoomList = msg.data;
                                    if (typeof updateRoomListUI === 'function') updateRoomListUI();
                                    break;
                                case 'room-deleted':
                                    alert("The room has been deleted.");
                                    window.location.href = "/";
                                    break;
                                case 'user-joined':
                                    playNotificationSound('join');
                                    const joinedScreenAudio = getScreenAudioFlag(msg.data);
                                    updatePeerTrackHints(msg.userId, msg.data);

                                    if (peers[msg.userId]) {

                                        if (msg.data.camEnabled !== undefined) {
                                            peerCamStatus[msg.userId] = msg.data.camEnabled;
                                        }
                                        if (msg.data.screenEnabled !== undefined) {
                                            peerScreenStatus[msg.userId] = msg.data.screenEnabled;
                                        }
                                        if (joinedScreenAudio !== undefined) {
                                            peerScreenHasAudio[msg.userId] = joinedScreenAudio;
                                        }
                                        if (peerScreenStatus[msg.userId] === true && joinedScreenAudio === true) {
                                            ensureScreenAudioUI(msg.userId);
                                        }
                                        updatePeerInfo(msg.userId, msg.data?.nickname, msg.data?.avatar, msg.data?.isMuted, msg.data?.isDeafened);
                                    } else {

                                        if (document.getElementById(`wrapper-${msg.userId}`)) {
                                            removePeer(msg.userId);
                                        }

                                        if (msg.data.camEnabled !== undefined) {
                                            peerCamStatus[msg.userId] = msg.data.camEnabled;
                                        }
                                        if (msg.data.screenEnabled !== undefined) {
                                            peerScreenStatus[msg.userId] = msg.data.screenEnabled;
                                        }
                                        if (joinedScreenAudio !== undefined) {
                                            peerScreenHasAudio[msg.userId] = joinedScreenAudio;
                                        }
                                        initPeer(msg.userId, true, msg.data?.nickname, msg.data?.avatar, msg.data?.isMuted, msg.data?.isDeafened);
                                        if (peerScreenStatus[msg.userId] === true && joinedScreenAudio === true) {
                                            ensureScreenAudioUI(msg.userId);
                                        }
                                    }

                                    const myAudioTrack = localStream && localStream.getAudioTracks()[0];
                                    const myMuted = !myAudioTrack || !myAudioTrack.enabled;
                                    const myCamEnabled = localStream && localStream.getVideoTracks()[0] && localStream.getVideoTracks()[0].enabled;
                                    const myScreenEnabled = !!screenStream;
                                    const myScreenHasAudio = screenStream && screenStream.getAudioTracks().length > 0;

                                    ws.send(JSON.stringify({
                                        type: 'identify',
                                        target: msg.userId,
                                        data: {
                                            userId: persistentUserId,
                                            nickname: userNickname,
                                            avatar: userAvatar,
                                            camEnabled: myCamEnabled,
                                            screenEnabled: myScreenEnabled,
                                            screenAudio: myScreenHasAudio,
                                            micTrackId: myAudioTrack ? myAudioTrack.id : null,
                                            screenAudioTrackId: screenStream ? (screenStream.getAudioTracks()[0]?.id || null) : null,
                                            isMuted: myMuted,
                                            isDeafened: isDeafened
                                        }
                                    }));
                                    break;
                                case 'user-left':

                                    if (msg.userId !== persistentUserId) {
                                        playNotificationSound('leave');
                                        removePeer(msg.userId);
                                        delete peerCamStatus[msg.userId];
                                        delete peerScreenStatus[msg.userId];
                                        delete peerScreenHasAudio[msg.userId];
                                        delete peerMicTrackId[msg.userId];
                                        delete peerScreenAudioTrackId[msg.userId];
                                    }
                                    break;
                                case 'user-kicked':
                                    if (msg.userId === persistentUserId) {
                                        alert("You have been kicked from the room.");
                                        hasLeftRoom = true;
                                        window.location.href = "/";
                                    } else {
                                        playNotificationSound('leave');
                                        removePeer(msg.userId);
                                        delete peerCamStatus[msg.userId];
                                        delete peerScreenStatus[msg.userId];
                                        delete peerScreenHasAudio[msg.userId];
                                        delete peerMicTrackId[msg.userId];
                                        delete peerScreenAudioTrackId[msg.userId];
                                        updateRoomListUI();
                                    }
                                    break;
                                case 'user-update':
                                    updatePeerTrackHints(msg.userId, msg.data);
                                     updatePeerInfo(msg.userId, msg.data.nickname, msg.data.avatar, msg.data.isMuted, msg.data.isDeafened);
                                    break;
                                case 'cam-toggle':
                                    if (msg.data && msg.data.enabled !== undefined) {
                                        peerCamStatus[msg.userId] = msg.data.enabled;
                                    }
                                    break;
                                case 'screen-toggle':
                                    if (msg.data && msg.data.enabled !== undefined) {
                                        updatePeerTrackHints(msg.userId, msg.data);
                                        peerScreenStatus[msg.userId] = msg.data.enabled;
                                        if (msg.data.hasAudio !== undefined) {
                                            peerScreenHasAudio[msg.userId] = msg.data.hasAudio;
                                        }
                                        if (msg.data.enabled && msg.data.hasAudio === true) {
                                            ensureScreenAudioUI(msg.userId);
                                        }
                                        const v = document.getElementById(`vid-${msg.userId}`);
                                        if (v) v.style.objectFit = msg.data.enabled ? 'contain' : 'contain';

                                        if (!msg.data.enabled || msg.data.hasAudio === false) {
                                            const row = document.getElementById(`vol-row-screen-${msg.userId}`);
                                            if (row) row.remove();
                                            const aud = document.getElementById(`aud-screen-${msg.userId}`);
                                            if (aud) aud.remove();
                                        }
                                    }
                                    break;
                                case 'identify':
                                    const identifiedScreenAudio = getScreenAudioFlag(msg.data);
                                    updatePeerTrackHints(msg.userId, msg.data);
                                    if (msg.data.camEnabled !== undefined) {
                                        peerCamStatus[msg.userId] = msg.data.camEnabled;
                                    }
                                    if (msg.data.screenEnabled !== undefined) {
                                        peerScreenStatus[msg.userId] = msg.data.screenEnabled;
                                    }
                                    if (identifiedScreenAudio !== undefined) {
                                        peerScreenHasAudio[msg.userId] = identifiedScreenAudio;
                                    }
                                    if (peers[msg.userId]) {
                                        updatePeerInfo(msg.userId, msg.data.nickname, msg.data.avatar, msg.data.isMuted, msg.data.isDeafened);
                                    } else {
                                        initPeer(msg.userId, false, msg.data.nickname, msg.data.avatar, msg.data.isMuted, msg.data.isDeafened);
                                    }
                                    if (peerScreenStatus[msg.userId] === true && identifiedScreenAudio === true) {
                                        ensureScreenAudioUI(msg.userId);
                                    }
                                    break;
                                case 'rename-channel':
                                    if (roomId === msg.data.roomId && channelId === msg.data.oldName) {
                                        performChannelSwitch(roomId, msg.data.newName);
                                    }
                                    break;
                                case 'signal':
                                    handleSignal(msg.userId, msg.data);
                                    break;
                                case 'pong':
                                    handlePong();
                                    break;
                            }
                        };

                        ws.onclose = () => {

                            stopHeartbeat();

                            if (reconnectStatusTimeout) {
                                clearTimeout(reconnectStatusTimeout);
                                reconnectStatusTimeout = null;
                            }

                            if (hasLeftRoom) {
                                console.log('User left the room, not reconnecting');
                                isReconnecting = false;
                                return;
                            }

                            if (isReconnecting) {
                                console.log('Reconnection already in progress, skipping duplicate onclose');
                                return;
                            }

                            isReconnecting = true;
                            reconnectionAttempts++;

                            const nextPeer = getNextFailoverHost();
                            if (nextPeer && reconnectionAttempts <= 2) {
                                console.log(`Failing over to peer: ${nextPeer.host} (${nextPeer.latency}ms)`);
                                updateStatus('connecting', 'Switching servers...');
                                currentServerHost = nextPeer.host;
                                wsUrl = `${wsProtocol}//${currentServerHost}/ws/${roomId}/${channelId}`;
                                setTimeout(() => {
                                    isReconnecting = false;
                                    connectWs();
                                }, 500);
                            } else if (reconnectionAttempts >= maxReconnectionAttempts) {
                                updateStatus('disconnected', 'Disconnected');
                                const btn = document.getElementById('btnReconnect');
                                if (btn) btn.classList.remove('hidden');
                                isReconnecting = false;
                                console.error('WebSocket disconnected after multiple retries. No further attempts will be made.');
                                stopHeartbeat();
                            } else {
                                const delay = getReconnectDelay(reconnectionAttempts);

                                reconnectStatusTimeout = setTimeout(() => {

                                    if (isReconnecting && (!ws || ws.readyState !== WebSocket.OPEN)) {
                                        updateStatus('connecting', `Reconnecting... (Attempt ${reconnectionAttempts}/${maxReconnectionAttempts})`);
                                    }
                                }, reconnectDelayMs);

                                console.log(`Reconnecting in ${Math.round(delay)}ms...`);
                                setTimeout(() => {

                                    if (reconnectStatusTimeout) {
                                        clearTimeout(reconnectStatusTimeout);
                                        reconnectStatusTimeout = null;
                                    }
                                    isReconnecting = false;
                                    connectWs();
                                }, delay);
                            }
                        };

                        ws.onerror = (error) => {
                            console.error('WebSocket Error:', error);
                        };
                    }

        function retryConnection() {
            const btn = document.getElementById('btnReconnect');
            if (btn) {
                btn.classList.add('text-green-500', 'bg-green-500/10');
                btn.classList.remove('text-slate-400', 'hover:text-white', 'hover:bg-slate-700');

                setTimeout(() => {
                    btn.classList.add('hidden');
                    btn.classList.remove('text-green-500', 'bg-green-500/10');
                    btn.classList.add('text-slate-400', 'hover:text-white', 'hover:bg-slate-700');

                    hasLeftRoom = false;
                    isReconnecting = false;
                    reconnectionAttempts = 0;
                    connectWs();
                }, 300);
            }
        }

        function setAvatar(layer, avatar) {
            layer.innerHTML = '';
            if (avatar) {
               const bgImg = document.createElement('img');
               bgImg.src = avatar;
               bgImg.className = 'avatar-img';
               bgImg.draggable = false;

               const centerDiv = document.createElement('div');
               centerDiv.className = 'avatar-center';

               const centerImg = document.createElement('img');
               centerImg.src = avatar;
               centerImg.draggable = false;

               centerDiv.appendChild(centerImg);
               layer.appendChild(bgImg);
               layer.appendChild(centerDiv);
           } else {
               const centerDiv = document.createElement('div');
               centerDiv.className = 'avatar-center';
               centerDiv.style.background = 'transparent';
               centerDiv.style.border = 'none';

               const text = document.createElement('div');
               text.className = 'text-6xl';
               text.style.display = 'flex';
               text.style.alignItems = 'center';
               text.style.justifyContent = 'center';
               text.style.width = '100%';
               text.style.height = '100%';
               text.style.margin = '0';
               text.innerText = '👤';

               centerDiv.appendChild(text);
               layer.appendChild(centerDiv);
           }
        }

        function updatePeerInfo(userId, nickname, avatar, isMuted, isDeafened) {
            const wrapper = document.getElementById(`wrapper-${userId}`);
            if (wrapper) {
                const nameSpan = wrapper.querySelector('.peer-name');
                if (nameSpan && nickname) nameSpan.innerText = nickname;

                const statusContainer = wrapper.querySelector('.peer-status-icons');
                if (statusContainer) {
                    statusContainer.innerHTML = '';
                    if (isDeafened) {
                        statusContainer.classList.remove('hidden');
                        statusContainer.classList.add('flex');
                        statusContainer.innerHTML = `<span class="text-red-500"><svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><line x1="1" y1="1" x2="23" y2="23"></line><path d="M21 14a2 2 0 0 0-2-2h-3a2 2 0 0 0-2 2v3a2 2 0 0 0 2 2h1a2 2 0 0 0 2-2V14z"></path><path d="M3 14a2 2 0 0 1 2-2h3a2 2 0 0 1 2 2v3a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V14z"></path><path d="M20.4 10.4C20.2 6.5 17 3.5 13 3.1"></path><path d="M6.5 5.5A9 9 0 0 0 3 12"></path></svg></span>`;
                    } else if (isMuted) {
                        statusContainer.classList.remove('hidden');
                        statusContainer.classList.add('flex');
                        statusContainer.innerHTML = `<span class="text-red-500"><svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><line x1="1" y1="1" x2="23" y2="23"></line><path d="M9 9v3a3 3 0 0 0 5.12 2.12M15 9.34V4a3 3 0 0 0-5.94-.6"></path><path d="M17 16.95A7 7 0 0 1 5 12v-2m14 0v2a7 7 0 0 1-.11 1.23"></path></svg></span>`;
                    } else {
                        statusContainer.classList.add('hidden');
                        statusContainer.classList.remove('flex');
                    }
                }

                const avatarLayer = wrapper.querySelector('.avatar-layer');
                if (avatarLayer) {
                     setAvatar(avatarLayer, avatar);
                }
            }
        }

        let dragState = {
            isDragging: false,
            draggedEl: null,
            placeholder: null,
            startX: 0,
            startY: 0,
            initialRect: null,
            allTiles: [],
            currentIndex: 0,
            tilePositions: null
        };

        function setupSmoothDragAndDrop(container) {
            container.addEventListener('mousedown', handleDragStart);
            container.addEventListener('touchstart', handleDragStart, { passive: false });
        }

        function handleDragStart(e) {
            if (e.target.closest('button') || e.target.closest('input') || e.target.closest('a')) {
                return;
            }

            const isTouch = e.type === 'touchstart';
            const clientX = isTouch ? e.touches[0].clientX : e.clientX;
            const clientY = isTouch ? e.touches[0].clientY : e.clientY;

            dragState.isDragging = false;
            dragState.draggedEl = this;
            dragState.startX = clientX;
            dragState.startY = clientY;
            dragState.initialRect = this.getBoundingClientRect();

            if (isTouch) {
                document.addEventListener('touchmove', handleDragMove, { passive: false });
                document.addEventListener('touchend', handleDragEnd);
                document.addEventListener('touchcancel', handleDragEnd);
            } else {
                document.addEventListener('mousemove', handleDragMove);
                document.addEventListener('mouseup', handleDragEnd);
            }
        }

        function handleDragMove(e) {
            if (!dragState.draggedEl) return;

            const isTouch = e.type === 'touchmove';
            const clientX = isTouch ? e.touches[0].clientX : e.clientX;
            const clientY = isTouch ? e.touches[0].clientY : e.clientY;

            if (!dragState.isDragging) {
                const deltaX = Math.abs(clientX - dragState.startX);
                const deltaY = Math.abs(clientY - dragState.startY);
                if (deltaX < 5 && deltaY < 5) return;

                dragState.isDragging = true;
                dragState.allTiles = [...remoteGrid.querySelectorAll('.video-container')];
                dragState.currentIndex = dragState.allTiles.indexOf(dragState.draggedEl);

                dragState.tilePositions = dragState.allTiles.map(tile => {
                    if (tile === dragState.draggedEl) return null;
                    const rect = tile.getBoundingClientRect();
                    return {
                        tile,
                        rect,
                        centerX: rect.left + rect.width / 2,
                        centerY: rect.top + rect.height / 2,
                        minDimension: Math.min(rect.width, rect.height)
                    };
                }).filter(p => p !== null);

                dragState.placeholder = dragState.draggedEl.cloneNode(true);
                dragState.placeholder.classList.add('drag-placeholder');
                dragState.placeholder.classList.remove('is-dragging');
                dragState.placeholder.style.pointerEvents = 'none';

                dragState.draggedEl.classList.add('is-dragging');
                dragState.draggedEl.style.transition = 'none';
                dragState.draggedEl.style.width = dragState.initialRect.width + 'px';
                dragState.draggedEl.style.height = dragState.initialRect.height + 'px';
                dragState.draggedEl.style.left = dragState.initialRect.left + 'px';
                dragState.draggedEl.style.top = dragState.initialRect.top + 'px';

                dragState.draggedEl.parentNode.insertBefore(dragState.placeholder, dragState.draggedEl);

                dragState.allTiles.forEach(tile => {
                    if (tile !== dragState.draggedEl && tile !== dragState.placeholder) {
                        tile.classList.add('is-shifting');
                    }
                });

                e.preventDefault();
            }

            const offsetX = clientX - dragState.startX;
            const offsetY = clientY - dragState.startY;

            dragState.draggedEl.style.transform = `scale(1.05) translate3d(${offsetX}px, ${offsetY}px, 0)`;

            const draggedCenterX = dragState.initialRect.left + dragState.initialRect.width / 2 + offsetX;
            const draggedCenterY = dragState.initialRect.top + dragState.initialRect.height / 2 + offsetY;

            let newDropIndex = -1;

            for (let i = 0; i < dragState.tilePositions.length; i++) {
                const pos = dragState.tilePositions[i];

                const distance = Math.hypot(draggedCenterX - pos.centerX, draggedCenterY - pos.centerY);

                if (distance < pos.minDimension * 0.6) {
                    newDropIndex = dragState.allTiles.indexOf(pos.tile);
                    break;
                }
            }

            if (newDropIndex !== -1 && newDropIndex !== dragState.currentIndex) {
                const placeholderArray = [...remoteGrid.querySelectorAll('.video-container')];
                const currentPlaceholderIndex = placeholderArray.indexOf(dragState.placeholder);

                if (newDropIndex > dragState.currentIndex) {
                    const targetTile = dragState.allTiles[newDropIndex];
                    const targetIndex = placeholderArray.indexOf(targetTile);
                    if (targetIndex !== -1) {
                        targetTile.after(dragState.placeholder);
                    }
                } else {
                    const targetTile = dragState.allTiles[newDropIndex];
                    const targetIndex = placeholderArray.indexOf(targetTile);
                    if (targetIndex !== -1) {
                        targetTile.before(dragState.placeholder);
                    }
                }

                dragState.currentIndex = newDropIndex;
            }

            if (isTouch) {
                e.preventDefault();
            }
        }

        function handleDragEnd(e) {
            if (!dragState.draggedEl) return;

            const wasDragging = dragState.isDragging;

            dragState.draggedEl.classList.remove('is-dragging');
            dragState.draggedEl.style.position = '';
            dragState.draggedEl.style.zIndex = '';
            dragState.draggedEl.style.boxShadow = '';
            dragState.draggedEl.style.transform = '';
            dragState.draggedEl.style.transition = '';
            dragState.draggedEl.style.pointerEvents = '';
            dragState.draggedEl.style.opacity = '';
            dragState.draggedEl.style.width = '';
            dragState.draggedEl.style.height = '';
            dragState.draggedEl.style.left = '';
            dragState.draggedEl.style.top = '';

            document.querySelectorAll('.video-container.is-shifting').forEach(tile => {
                tile.classList.remove('is-shifting');
            });

            if (dragState.placeholder && dragState.placeholder.parentNode) {
                dragState.placeholder.parentNode.insertBefore(dragState.draggedEl, dragState.placeholder);
                dragState.placeholder.remove();
            }

            if (wasDragging) {
                saveTileOrder();
            }

            document.removeEventListener('mousemove', handleDragMove);
            document.removeEventListener('mouseup', handleDragEnd);
            document.removeEventListener('touchmove', handleDragMove);
            document.removeEventListener('touchend', handleDragEnd);
            document.removeEventListener('touchcancel', handleDragEnd);

            dragState.isDragging = false;
            dragState.draggedEl = null;
            dragState.placeholder = null;
            dragState.tilePositions = null;
        }

        function saveTileOrder() {
            const order = [...remoteGrid.querySelectorAll('.video-container')]
                .map(el => el.dataset.userId)
                .filter(id => id);
            localStorage.setItem('tileOrder', JSON.stringify(order));
        }

        function loadTileOrder() {
            try {
                const saved = localStorage.getItem('tileOrder');
                if (!saved) return;
                const order = JSON.parse(saved);
                const containers = {};

                [...remoteGrid.querySelectorAll('.video-container')].forEach(el => {
                    const userId = el.dataset.userId;
                    if (userId) {
                        containers[userId] = el;
                    }
                });

                order.forEach(userId => {
                    if (containers[userId]) {
                        remoteGrid.appendChild(containers[userId]);
                    }
                });
            } catch (e) {
                console.warn('Failed to load tile order:', e);
            }
        }

        function checkEmpty() {
            const count = Object.keys(peers).length;
            if (count === 0) {
                emptyState.style.display = 'block';
            } else {
                emptyState.style.display = 'none';
                loadTileOrder();
            }
            updateGridLayout(count);
        }

        function updateGridLayout(count) {
            remoteGrid.className = 'grid gap-2 md:gap-4 w-full h-full max-w-[1600px] transition-all duration-500 grid-expand';

            if (count === 0) return;

            if (count === 1) {
                remoteGrid.classList.add('grid-cols-1');
            } else if (count === 2) {
                remoteGrid.classList.add('grid-cols-1', 'md:grid-cols-2');
            } else if (count === 3) {
                remoteGrid.classList.add('grid-cols-1');
                remoteGrid.style.gridTemplateColumns = 'repeat(auto-fit, minmax(min(100%, 400px), 1fr))';
                remoteGrid.style.justifyContent = 'center';
            } else if (count === 4) {
                remoteGrid.classList.add('grid-cols-2');
                remoteGrid.style.gridTemplateColumns = '';
            } else if (count === 5) {
                remoteGrid.style.gridTemplateColumns = 'repeat(auto-fit, minmax(min(100%, 350px), 1fr))';
            } else if (count === 6) {
                remoteGrid.classList.add('grid-cols-2', 'md:grid-cols-3');
                remoteGrid.style.gridTemplateColumns = '';
            } else if (count === 7) {
                remoteGrid.style.gridTemplateColumns = 'repeat(auto-fit, minmax(min(100%, 320px), 1fr))';
            } else if (count === 8) {
                remoteGrid.classList.add('grid-cols-2', 'md:grid-cols-4');
                remoteGrid.style.gridTemplateColumns = '';
            } else if (count === 9) {
                remoteGrid.classList.add('grid-cols-3');
                remoteGrid.style.gridTemplateColumns = '';
            } else {
                remoteGrid.classList.add('grid-cols-3', 'md:grid-cols-4');
                remoteGrid.style.gridTemplateColumns = '';
            }
        }

        function forceStereoAudio(sdp) {
            let sdpLines = sdp.split('\r\n');
            let opusPayload = -1;
            let rtpmapLineIndex = -1;

            for (let i = 0; i < sdpLines.length; i++) {
                if (sdpLines[i].startsWith('a=rtpmap:')) {
                    if (sdpLines[i].includes('opus/48000')) {
                        opusPayload = sdpLines[i].split(':')[1].split(' ')[0];
                        rtpmapLineIndex = i;
                        break;
                    }
                }
            }

            if (opusPayload === -1) return sdp;

            let fmtpLineIndex = -1;
            for (let i = 0; i < sdpLines.length; i++) {
                if (sdpLines[i].startsWith('a=fmtp:' + opusPayload)) {
                    fmtpLineIndex = i;
                    break;
                }
            }

            if (fmtpLineIndex === -1) {
                sdpLines.splice(rtpmapLineIndex + 1, 0, 'a=fmtp:' + opusPayload + ' stereo=1;sprop-stereo=1;maxaveragebitrate=510000;useinbandfec=1;cbr=1;usedtx=0');
            } else {
                let fmtpLine = sdpLines[fmtpLineIndex];
                if (!fmtpLine.includes('stereo=1')) {
                    sdpLines[fmtpLineIndex] = fmtpLine + ';stereo=1;sprop-stereo=1;maxaveragebitrate=510000;useinbandfec=1;cbr=1;usedtx=0';
                }
            }
            return sdpLines.join('\r\n');
        }

        function negotiate(userId, pc) {
            pc.createOffer()
                .then(offer => {
                    offer.sdp = forceStereoAudio(offer.sdp);
                    return pc.setLocalDescription(offer);
                })
                .then(() => sendSignal(userId, { type: 'offer', sdp: pc.localDescription }))
                .catch(e => console.error("Negotiation error", e));
        }

        function createPeerUI(userId, displayName, avatarUrl, remoteIsDeafened, remoteIsMuted) {

            if (document.getElementById(`wrapper-${userId}`)) {
                return;
            }

            const container = document.createElement('div');
            container.id = `wrapper-${userId}`;
            container.className = 'video-container group bg-slate-800 border border-slate-700';

            const vid = document.createElement('video');
            vid.id = `vid-${userId}`;
            vid.autoplay = true;
            vid.playsInline = true;
            attachSinkId(vid, currentAudioOutputId);
            vid.autoplay = true;
            vid.playsInline = true;
            attachSinkId(vid, currentAudioOutputId);

            const savedVol = getVolumeSettings(userId, 'main');
            vid.volume = savedVol;

            vid.srcObject = new MediaStream();
            if (isDeafened) vid.muted = true;

            const avatarLayer = document.createElement('div');
            avatarLayer.className = 'avatar-layer';

            setAvatar(avatarLayer, avatarUrl);

            const label = document.createElement('div');
            label.className = 'name-tag absolute bottom-3 left-3 bg-black/50 px-3 py-1 rounded-full text-sm text-white backdrop-blur-md z-30 flex items-center gap-1.5';

            const nameSpan = document.createElement('span');
            nameSpan.className = 'peer-name';
            nameSpan.innerText = displayName;

            const statusContainer = document.createElement('div');
            statusContainer.className = 'peer-status-icons items-center' + (remoteIsDeafened || remoteIsMuted ? ' flex' : ' hidden');

            if (remoteIsDeafened) {
                statusContainer.innerHTML = `<span class="text-red-500"><svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><line x1="1" y1="1" x2="23" y2="23"></line><path d="M21 14a2 2 0 0 0-2-2h-3a2 2 0 0 0-2 2v3a2 2 0 0 0 2 2h1a2 2 0 0 0 2-2V14z"></path><path d="M3 14a2 2 0 0 1 2-2h3a2 2 0 0 1 2 2v3a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V14z"></path><path d="M20.4 10.4C20.2 6.5 17 3.5 13 3.1"></path><path d="M6.5 5.5A9 9 0 0 0 3 12"></path></svg></span>`;
            } else if (remoteIsMuted) {
                statusContainer.innerHTML = `<span class="text-red-500"><svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><line x1="1" y1="1" x2="23" y2="23"></line><path d="M9 9v3a3 3 0 0 0 5.12 2.12M15 9.34V4a3 3 0 0 0-5.94-.6"></path><path d="M17 16.95A7 7 0 0 1 5 12v-2m14 0v2a7 7 0 0 1-.11 1.23"></path></svg></span>`;
            }

            label.appendChild(nameSpan);
            label.appendChild(statusContainer);

            const volControls = document.createElement('div');
            volControls.id = `vol-controls-${userId}`;
            volControls.className = 'volume-controls z-30';

            const mainVolRow = document.createElement('div');
            mainVolRow.className = 'vol-row';
            mainVolRow.id = `vol-row-main-${userId}`;
            mainVolRow.innerHTML = `
                <button class="text-white hover:text-blue-400" onclick="toggleMute('${userId}', 'main')" id="mute-main-${userId}">
                    <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">${savedVol === 0 ? '<polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5"></polygon><line x1="23" y1="9" x2="17" y2="15"></line><line x1="17" y1="9" x2="23" y2="15"></line>' : '<polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5"></polygon><path d="M19.07 4.93a10 10 0 0 1 0 14.14M15.54 8.46a5 5 0 0 1 0 7.07"></path>'}</svg>
                </button>
                <input type="range" min="0" max="1" step="0.05" value="${savedVol}" oninput="setVolume('${userId}', 'main', this.value)">
            `;
            if (savedVol === 0) {
                const btn = mainVolRow.querySelector("button");
                if (btn) btn.classList.add("text-red-500");
            }
            volControls.appendChild(mainVolRow);

            const fsBtn = document.createElement('button');
            fsBtn.className = 'absolute top-3 right-3 p-2 rounded-xl bg-black/40 hover:bg-blue-600 text-white backdrop-blur-md transition-all opacity-0 group-hover:opacity-100 scale-90 hover:scale-100 z-30';
            fsBtn.innerHTML = '<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M8 3H5a2 2 0 0 0-2 2v3m18 0V5a2 2 0 0 0-2-2h-3m0 18h3a2 2 0 0 0 2-2v-3M3 16v3a2 2 0 0 0 2 2h3"/></svg>';
            fsBtn.onclick = () => toggleFullscreen(userId);
            fsBtn.title = "Toggle Fullscreen";

            fsBtn.addEventListener('fullscreenchange', () => {
                if (document.fullscreenElement === container) {
                    fsBtn.innerHTML = '<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M8 3v3a2 2 0 0 1-2 2H3m18 0h-3a2 2 0 0 1-2-2V3m0 18v-3a2 2 0 0 1 2-2h3"/></svg>';
                    fsBtn.classList.add('bg-blue-600');
                } else {
                    fsBtn.innerHTML = '<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M8 3H5a2 2 0 0 0-2 2v3m18 0V5a2 2 0 0 0-2-2h-3m0 18h3a2 2 0 0 0 2-2v-3M3 16v3a2 2 0 0 0 2 2h3"/></svg>';
                    fsBtn.classList.remove('bg-blue-600');
                }
            });

            container.dataset.userId = userId;

            setupSmoothDragAndDrop(container);

            container.appendChild(vid);
            container.appendChild(avatarLayer);
            container.appendChild(label);
            container.appendChild(volControls);
            container.appendChild(fsBtn);

            const remoteGrid = document.getElementById('remoteGrid');
            if (remoteGrid) {
                remoteGrid.appendChild(container);
                checkEmpty();
            } else {
                console.error('remoteGrid not found!');
            }

        }

        function initPeer(userId, initiator, nickname, avatarUrl, isMuted, remoteIsDeafened) {
            if (peers[userId]) return;

            const displayName = nickname || `User ${userId.substr(0,4)}`;

            const pc = new RTCPeerConnection(rtcConfig);
            peers[userId] = pc;

            if (localStream) {
                localStream.getAudioTracks().forEach(track => pc.addTrack(track, localStream));
            }

            if (screenStream) {
                const screenTrack = screenStream.getVideoTracks()[0];
                if (screenTrack) {
                    if (localStream) {
                        pc.addTrack(screenTrack, localStream);
                    } else {
                        pc.addTrack(screenTrack, screenStream);
                    }
                }
                const screenAudioTrack = screenStream.getAudioTracks()[0];
                if (screenAudioTrack) {
                    const sender = pc.addTrack(screenAudioTrack, screenStream);
                    const params = sender.getParameters();
                    if (!params.encodings) params.encodings = [{}];
                    params.encodings[0].maxBitrate = 512000;
                    sender.setParameters(params).catch(e => console.warn(e));
                }
            } else if (localStream) {
                localStream.getVideoTracks().forEach(track => pc.addTrack(track, localStream));
            }

            if (!localStream || localStream.getVideoTracks().length === 0) {
                 pc.addTransceiver('video', { direction: 'recvonly' });
            }

            if (!localStream || localStream.getAudioTracks().length === 0 || isDeafened) {
                 pc.addTransceiver('audio', { direction: 'recvonly' });
            }

            createPeerUI(userId, displayName, avatarUrl, remoteIsDeafened, isMuted);

            pc.ontrack = (event) => {

                if (peers[userId] !== pc) {
                    return;
                }

                let container = document.getElementById(`wrapper-${userId}`);
                let vid = document.getElementById(`vid-${userId}`);

                if (!container || !vid) {
                    createPeerUI(userId, displayName, avatarUrl, remoteIsDeafened, isMuted);
                    container = document.getElementById(`wrapper-${userId}`);
                    vid = document.getElementById(`vid-${userId}`);
                }

                if (!vid || !vid.srcObject) {
                    console.error('[ontrack] Video element or srcObject is null for', userId);
                    return;
                }

                const volControls = document.getElementById(`vol-controls-${userId}`);
                const mainStream = vid.srcObject;

                if (event.track.kind === 'video') {
                     mainStream.getVideoTracks().forEach(t => mainStream.removeTrack(t));
                     mainStream.addTrack(event.track);
                     vid.play().catch(e => console.error("Remote play err", e));

                     event.track.onmute = () => { checkActive(userId); };
                     event.track.onunmute = () => { checkActive(userId); };
                     event.track.onended = () => { checkActive(userId); };
                }

                if (event.track.kind === 'audio') {

                    const existingTracks = mainStream.getAudioTracks();
                    const trackAlreadyExists = existingTracks.some(t => t.id === event.track.id);

                    if (trackAlreadyExists) {

                        return;
                    }

                    const hintedMicTrackId = peerMicTrackId[userId];
                    const hintedScreenTrackId = peerScreenAudioTrackId[userId];
                    const isHintedScreenTrack = !!hintedScreenTrackId && event.track.id === hintedScreenTrackId;
                    const isHintedMicTrack = !!hintedMicTrackId && event.track.id === hintedMicTrackId;

                    if (isHintedScreenTrack && !isHintedMicTrack) {
                        peerScreenHasAudio[userId] = true;
                        const savedScreenVol = getVolumeSettings(userId, 'screen');

                        let screenAud = document.getElementById(`aud-screen-${userId}`);
                        if (!screenAud) {
                            screenAud = document.createElement('audio');
                            screenAud.id = `aud-screen-${userId}`;
                            screenAud.autoplay = true;
                            attachSinkId(screenAud, currentAudioOutputId);
                            screenAud.volume = savedScreenVol;
                            container.appendChild(screenAud);
                        }

                        const screenStream = new MediaStream([event.track]);
                        screenAud.srcObject = screenStream;
                        if (isDeafened) screenAud.muted = true;

                        if (!document.getElementById(`vol-row-screen-${userId}`)) {
                            const row = document.createElement('div');
                            row.className = 'vol-row';
                            row.id = `vol-row-screen-${userId}`;
                            row.innerHTML = `
                                <div class="flex items-center gap-2">
                                    <button class="text-white hover:text-blue-400" onclick="toggleMute('${userId}', 'screen')" id="mute-screen-${userId}">
                                        <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="4" y="2" width="16" height="14" rx="2" ry="2"></rect><line x1="12" y1="22" x2="12" y2="16"></line><path d="M5 12h14"></path><path d="M12 12v4"></path></svg>
                                    </button>
                                    <input type="range" min="0" max="1" step="0.05" value="${savedScreenVol}" oninput="setVolume('${userId}', 'screen', this.value)">
                                </div>
                            `;
                            volControls.appendChild(row);
                        }

                        (async () => { await setupAudioMonitor(screenStream, `wrapper-${userId}`); })();
                        event.track.onended = () => {
                            const row = document.getElementById(`vol-row-screen-${userId}`);
                            if (row) row.remove();
                            const aud = document.getElementById(`aud-screen-${userId}`);
                            if (aud) aud.remove();
                        };
                        return;
                    }

                    if (mainStream.getAudioTracks().length === 0 || isHintedMicTrack) {
                        mainStream.addTrack(event.track);
                        (async () => { await setupAudioMonitor(mainStream, `wrapper-${userId}`); })();

                    } else {

                        peerScreenHasAudio[userId] = true;

                        const savedScreenVol = getVolumeSettings(userId, 'screen');

                        let screenAud = document.getElementById(`aud-screen-${userId}`);
                        if (!screenAud) {
                            screenAud = document.createElement('audio');
                            screenAud.id = `aud-screen-${userId}`;
                            screenAud.autoplay = true;
                            attachSinkId(screenAud, currentAudioOutputId);
                            screenAud.volume = savedScreenVol;
                            container.appendChild(screenAud);
                        }

                        const screenStream = new MediaStream([event.track]);
                        screenAud.srcObject = screenStream;
                        if (isDeafened) screenAud.muted = true;

                        if (!document.getElementById(`vol-row-screen-${userId}`)) {
                            const row = document.createElement('div');
                            row.className = 'vol-row';
                            row.id = `vol-row-screen-${userId}`;
                            row.innerHTML = `
                                <div class="flex items-center gap-2">
                                    <button class="text-white hover:text-blue-400" onclick="toggleMute('${userId}', 'screen')" id="mute-screen-${userId}">
                                        <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="4" y="2" width="16" height="14" rx="2" ry="2"></rect><line x1="12" y1="22" x2="12" y2="16"></line><path d="M5 12h14"></path><path d="M12 12v4"></path></svg>
                                    </button>
                                    <input type="range" min="0" max="1" step="0.05" value="${savedScreenVol}" oninput="setVolume('${userId}', 'screen', this.value)">
                                </div>
                            `;
                            volControls.appendChild(row);
                        }

                        (async () => { await setupAudioMonitor(screenStream, `wrapper-${userId}`); })();

                        event.track.onended = () => {
                            screenAud.remove();
                            const row = document.getElementById(`vol-row-screen-${userId}`);
                            if (row) row.remove();
                        };
                    }
                }

                const checkActive = (uid) => {
                     const v = document.getElementById(`vid-${uid}`);
                     if (!v || !v.srcObject) return;

                     const isCamOff = peerCamStatus[uid] === false;
                     const isScreenOn = peerScreenStatus[uid] === true;

                     if (isScreenOn) {
                         v.classList.add('active');
                         v.style.objectFit = 'contain';
                         return;
                     }

                     if (isCamOff) {
                         v.classList.remove('active');
                         return;
                     }

                     const vTracks = v.srcObject.getVideoTracks();
                     let hasActiveVideo = false;
                     if (vTracks.length > 0) {
                         const t = vTracks[0];
                         if (t.enabled && !t.muted && t.readyState === 'live') {
                             hasActiveVideo = true;
                         }
                     }

                     if (hasActiveVideo) {
                         v.classList.add('active');
                         v.style.objectFit = 'contain';
                     } else {
                         v.classList.remove('active');
                     }
                };

                if (event.track.kind === 'video') {
                     vid.onloadedmetadata = () => checkActive(userId);
                     vid.onresize = () => checkActive(userId);
                }

                if (!container.dataset.interval) {
                    const intId = setInterval(() => checkActive(userId), 1000);
                    container.dataset.interval = intId;
                }
            };

            pc.onicecandidate = (event) => {
                if (event.candidate) {
                    sendSignal(userId, { type: 'candidate', candidate: event.candidate });
                }
            };

            pc.oniceconnectionstatechange = () => {
                const state = pc.iceConnectionState;
                console.log(`ICE connection state for ${userId.substr(0,4)}: ${state}`);

                if (state === 'failed' || state === 'disconnected' || state === 'closed') {
                    console.warn(`Peer ${userId.substr(0,4)} connection ${state}`);

                    updateConnectionStatus();
                } else if (state === 'connected') {
                    updateConnectionStatus();
                }
            };

            pc.onconnectionstatechange = () => {
                const state = pc.connectionState;
                console.log(`Connection state for ${userId.substr(0,4)}: ${state}`);

                if (state === 'disconnected') {

                    console.warn(`Peer ${userId.substr(0,4)} temporarily disconnected, waiting for recovery...`);
                    updateConnectionStatus();

                    if (!pc._disconnectTimeout) {
                        pc._disconnectTimeout = setTimeout(() => {
                            if (pc.connectionState === 'disconnected') {
                                console.warn(`Peer ${userId.substr(0,4)} did not recover, removing...`);
                                removePeer(userId);
                            }
                            pc._disconnectTimeout = null;
                        }, 15000);
                    }
                } else if (state === 'failed' || state === 'closed') {

                    if (pc._disconnectTimeout) {
                        clearTimeout(pc._disconnectTimeout);
                        pc._disconnectTimeout = null;
                    }
                    console.warn(`Peer ${userId.substr(0,4)} connection ${state}, removing...`);
                    removePeer(userId);
                } else if (state === 'connected') {

                    if (pc._disconnectTimeout) {
                        clearTimeout(pc._disconnectTimeout);
                        pc._disconnectTimeout = null;
                        console.log(`Peer ${userId.substr(0,4)} reconnected successfully`);
                    }
                    updateConnectionStatus();
                }
            };

            if (initiator) {
                negotiate(userId, pc);
            }
        }

        async function handleSignal(userId, data) {
            if (!peers[userId]) initPeer(userId, false, undefined, null);
            const pc = peers[userId];

            try {
                if (data.type === 'offer') {
                    await pc.setRemoteDescription(new RTCSessionDescription(data.sdp));
                    const answer = await pc.createAnswer();
                    answer.sdp = forceStereoAudio(answer.sdp);
                    await pc.setLocalDescription(answer);
                    sendSignal(userId, { type: 'answer', sdp: answer });
                } else if (data.type === 'answer') {
                    await pc.setRemoteDescription(new RTCSessionDescription(data.sdp));
                } else if (data.type === 'candidate') {
                    await pc.addIceCandidate(new RTCIceCandidate(data.candidate));
                }
            } catch (e) {
                console.error("Signaling error", e);
            }
        }

        function removePeer(userId) {
            if (peers[userId]) {

                if (peers[userId]._disconnectTimeout) {
                    clearTimeout(peers[userId]._disconnectTimeout);
                    peers[userId]._disconnectTimeout = null;
                }
                peers[userId].close();
                delete peers[userId];
            }
            const el = document.getElementById(`wrapper-${userId}`);
            if (el) el.remove();

            const screenAud = document.getElementById(`aud-screen-${userId}`);
            if (screenAud) screenAud.remove();
            const volRow = document.getElementById(`vol-row-screen-${userId}`);
            if (volRow) volRow.remove();

            const sidebarAvatar = document.querySelector(`.room-user-row[data-user-id="${userId}"] .mini-avatar`);
            if (sidebarAvatar) sidebarAvatar.classList.remove('speaking-glow');

            delete peerMicTrackId[userId];
            delete peerScreenAudioTrackId[userId];
            checkEmpty();
        }

        function sendSignal(toId, data) {
            ws.send(JSON.stringify({ type: 'signal', target: toId, data: data }));
        }

        window.toggleFullscreen = function(userId) {
            const el = document.getElementById(`wrapper-${userId}`);
            if (!el) return;

            if (!document.fullscreenElement) {
                el.requestFullscreen().catch(err => {
                    console.error(`Error attempting to enable fullscreen: ${err.message}`);
                });
            } else {
                document.exitFullscreen();
            }
        };

        window.toggleMute = function(userId, type) {
            let el;
            let btn;

            if (type === 'screen') {
                el = document.getElementById(`aud-screen-${userId}`);
                if (!el) {
                    el = document.getElementById(`vid-${userId}`);
                }
                btn = document.getElementById(`mute-screen-${userId}`);
            } else {
                el = document.getElementById(`vid-${userId}`);
                btn = document.getElementById(`mute-main-${userId}`);
            }

            if (el) {
                el.muted = !el.muted;
                const isMuted = el.muted;

                if (type === 'screen') {
                     if (isMuted) {
                        btn.innerHTML = `<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="4" y="2" width="16" height="14" rx="2" ry="2"></rect><line x1="23" y1="9" x2="17" y2="15"></line><line x1="17" y1="9" x2="23" y2="15"></line></svg>`;
                        btn.classList.add('text-red-500');
                    } else {
                        btn.innerHTML = `<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="4" y="2" width="16" height="14" rx="2" ry="2"></rect><line x1="12" y1="22" x2="12" y2="16"></line><path d="M5 12h14"></path><path d="M12 12v4"></path></svg>`;
                        btn.classList.remove('text-red-500');
                    }
                } else {
                    if (isMuted) {
                        btn.innerHTML = `<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5"></polygon><line x1="23" y1="9" x2="17" y2="15"></line><line x1="17" y1="9" x2="23" y2="15"></line></svg>`;
                        btn.classList.add('text-red-500');
                    } else {
                        btn.innerHTML = `<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5"></polygon><path d="M19.07 4.93a10 10 0 0 1 0 14.14M15.54 8.46a5 5 0 0 1 0 7.07"></path></svg>`;
                        btn.classList.remove('text-red-500');
                    }
                }
            }
        }

        window.setVolume = function(userId, type, val) {
             let el;
            if (type === 'screen') {
                el = document.getElementById(`aud-screen-${userId}`);
                if (!el) {
                    el = document.getElementById(`vid-${userId}`);
                }
            } else {
                el = document.getElementById(`vid-${userId}`);
            }
            if (el) {
                el.volume = val;
                saveVolumeSettings(userId, type, val);
            }
        }

        function saveVolumeSettings(userId, type, val) {
            sessionStorage.setItem(`rustrooms_vol_${userId}_${type}`, val);
        }

        function getVolumeSettings(userId, type) {
            const val = sessionStorage.getItem(`rustrooms_vol_${userId}_${type}`);
            return val ? parseFloat(val) : 1.0;
        }

        function leaveRoom() {

            hasLeftRoom = true;

            playNotificationSound('disconnect');

            if (localStream) {

                localStream.getVideoTracks().forEach(track => track.stop());
                if (localStream._originalStream) {
                    localStream._originalStream.getVideoTracks().forEach(track => track.stop());
                }

                const aTracks = localStream.getAudioTracks();
                aTracks.forEach(t => t.enabled = false);
                const origATracks = localStream._originalStream ? localStream._originalStream.getAudioTracks() : [];
                origATracks.forEach(t => t.enabled = false);

                setTimeout(() => {
                    aTracks.forEach(track => track.stop());
                    origATracks.forEach(track => track.stop());
                }, 800);

                localStream = null;
            }

            if (screenStream) {
                const sTracks = screenStream.getTracks();
                setTimeout(() => {
                    sTracks.forEach(track => track.stop());
                }, 800);
                screenStream = null;
            }

            Object.keys(peers).forEach(userId => {
                if (peers[userId]) {
                    peers[userId].close();
                    delete peers[userId];
                }

                const vid = document.getElementById(`vid-${userId}`);
                if (vid) {
                    vid.pause();
                    vid.srcObject = null;
                }

                const screenAud = document.getElementById(`aud-screen-${userId}`);
                if (screenAud) {
                    screenAud.pause();
                    screenAud.srcObject = null;
                    screenAud.remove();
                }

                const volRowScreen = document.getElementById(`vol-row-screen-${userId}`);
                if (volRowScreen) volRowScreen.remove();

                const el = document.getElementById(`wrapper-${userId}`);
                if (el) el.remove();
            });

            if (ws) {
                ws.close();
                ws = null;
            }

            if (audioContext && audioContext.state !== 'closed') {
                setTimeout(() => {
                    if (audioContext && audioContext.state !== 'closed') {
                        audioContext.close().catch(e => console.error('Error closing audio context:', e));
                        audioContext = null;
                    }
                }, 800);
            }

            const welcomeOverlay = document.getElementById('welcomeOverlay');
            const mainApp = document.querySelector('main');
            const taskbar = document.querySelector('.taskbar');
            const sidebar = document.getElementById('roomSidebar');
            const overlay = document.getElementById('sidebarOverlay');

            if (sidebar) {
                sidebar.style.transition = 'none';
                sidebar.classList.remove('open');
            }
            if (overlay) overlay.classList.remove('open');
            document.body.classList.remove('sidebar-open');

            if (welcomeOverlay) welcomeOverlay.style.display = 'flex';
            if (mainApp) mainApp.style.display = 'none';
            if (taskbar) taskbar.style.display = 'none';

            sessionStorage.removeItem('rustrooms_setup_done');
            history.replaceState(null, '', '/');
        }

        function toggleMic() {
            if (!localStream) return;
            const tracks = localStream.getAudioTracks();
            if (tracks.length > 0) {
                const track = tracks[0];

                if (isDeafened) {

                    return;
                }

                track.enabled = !track.enabled;
                const btn = document.getElementById('btnMic');
                if (!track.enabled) {
                    playNotificationSound('mute');
                    btn.classList.add('active-red');
                    btn.innerHTML = `<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="1" y1="1" x2="23" y2="23"></line><path d="M9 9v3a3 3 0 0 0 5.12 2.12M15 9.34V4a3 3 0 0 0-5.94-.6"></path><path d="M17 16.95A7 7 0 0 1 5 12v-2m14 0v2a7 7 0 0 1-.11 1.23"></path><line x1="12" x2="12" y1="19" y2="22"></line></svg>`;
                } else {
                    playNotificationSound('unmute');
                    btn.classList.remove('active-red');
                    btn.innerHTML = `<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 2a3 3 0 0 0-3 3v7a3 3 0 0 0 6 0V5a3 3 0 0 0-3-3Z"/><path d="M19 10v2a7 7 0 0 1-14 0v-2"/><line x1="12" x2="12" y1="19" y2="22"/></svg>`;
                }
                updateLocalLabel();

                if (track.enabled) {
                    const screenAudioTrack = screenStream?.getAudioTracks()[0];
                    for (const userId in peers) {
                        const pc = peers[userId];
                        const senders = pc.getSenders();
                        let micSender = null;

                        for (const sender of senders) {
                            if (sender.track && sender.track.kind === 'audio') {
                                const isScreenAudio = screenAudioTrack && sender.track.id === screenAudioTrack.id;
                                if (!isScreenAudio) {
                                    micSender = sender;
                                    break;
                                }
                            }
                        }

                        if (micSender) {
                            micSender.replaceTrack(track).catch(() => {});
                            if (micSender.transceiver && (micSender.transceiver.direction === 'recvonly' || micSender.transceiver.direction === 'inactive')) {
                                micSender.transceiver.direction = 'sendrecv';
                            }
                        } else {
                            let attachedToNullSender = false;
                            for (const sender of senders) {
                                if (!sender.track || sender.track === null) {
                                    sender.replaceTrack(track).catch(() => {});
                                    if (sender.transceiver && (sender.transceiver.direction === 'recvonly' || sender.transceiver.direction === 'inactive')) {
                                        sender.transceiver.direction = 'sendrecv';
                                    }
                                    attachedToNullSender = true;
                                    break;
                                }
                            }
                            if (!attachedToNullSender) {
                                pc.addTrack(track, localStream);
                            }
                        }

                        negotiate(userId, pc);
                    }
                }

                if (ws && ws.readyState === WebSocket.OPEN) {
                    ws.send(JSON.stringify({
                        type: 'update-user',
                        data: {
                            isMuted: !track.enabled,
                            isDeafened: isDeafened,
                            micTrackId: track ? track.id : null,
                            screenAudioTrackId: screenStream ? (screenStream.getAudioTracks()[0]?.id || null) : null
                        }
                    }));
                }
                savePreferences();
            }
        }

        function toggleDeafen() {
            isDeafened = !isDeafened;
            const btn = document.getElementById('btnDeafen');
            const btnMic = document.getElementById('btnMic');

            const deafenOnSvg = `<svg xmlns="http://www.w3.org/2000/svg" width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M3 18v-6a9 9 0 0 1 18 0v6"></path><path d="M21 19a2 2 0 0 1-2 2h-1a2 2 0 0 1-2-2v-3a2 2 0 0 1 2-2h3zM3 19a2 2 0 0 0 2 2h1a2 2 0 0 0 2-2v-3a2 2 0 0 0-2-2H3z"></path></svg>`;
            const deafenOffSvg = `<svg xmlns="http://www.w3.org/2000/svg" width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="1" y1="1" x2="23" y2="23"></line><path d="M21 14a2 2 0 0 0-2-2h-3a2 2 0 0 0-2 2v3a2 2 0 0 0 2 2h1a2 2 0 0 0 2-2V14z"></path><path d="M3 14a2 2 0 0 1 2-2h3a2 2 0 0 1 2 2v3a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V14z"></path><path d="M20.4 10.4C20.2 6.5 17 3.5 13 3.1"></path><path d="M6.5 5.5A9 9 0 0 0 3 12"></path></svg>`;

            const micAudioTrack = localStream?.getAudioTracks()[0];
            const screenAudioTrack = screenStream?.getAudioTracks()[0];

            if (isDeafened) {
                playNotificationSound('mute');
                btn.classList.add('active-red');
                btn.innerHTML = deafenOffSvg;

                if (micAudioTrack && micAudioTrack.enabled) {
                    btn.dataset.micWasEnabled = 'true';
                }

                if (btnMic) {
                    btnMic.disabled = true;

                    if (micAudioTrack && micAudioTrack.enabled) {
                        btnMic.classList.add('active-red');
                        btnMic.innerHTML = `<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="1" y1="1" x2="23" y2="23"></line><path d="M9 9v3a3 3 0 0 0 5.12 2.12M15 9.34V4a3 3 0 0 0-5.94-.6"></path><path d="M17 16.95A7 7 0 0 1 5 12v-2m14 0v2a7 7 0 0 1-.11 1.23"></path><line x1="12" x2="12" y1="19" y2="22"></line></svg>`;
                    }
                }

                if (micAudioTrack) {
                    micAudioTrack.enabled = false;
                }

                document.querySelectorAll('video, audio').forEach(el => {
                    if (el.id !== 'localVideo' && el.id !== 'previewVideo') {
                        el.dataset.wasMuted = el.muted;
                        el.muted = true;
                    }
                });
            } else {
                playNotificationSound('unmute');
                btn.classList.remove('active-red');
                btn.innerHTML = deafenOnSvg;

                if (btnMic) {
                    btnMic.disabled = false;
                }

                const shouldEnableMic = btn.dataset.micWasEnabled === 'true';

                if (micAudioTrack && shouldEnableMic) {
                    micAudioTrack.enabled = true;

                    if (btnMic) {
                        btnMic.classList.remove('active-red');
                        btnMic.innerHTML = `<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 2a3 3 0 0 0-3 3v7a3 3 0 0 0 6 0V5a3 3 0 0 0-3-3Z"/><path d="M19 10v2a7 7 0 0 1-14 0v-2"/><line x1="12" x2="12" y1="19" y2="22"/></svg>`;
                    }
                    delete btn.dataset.micWasEnabled;
                }

                if (micAudioTrack) {
                    for (const userId in peers) {
                        const pc = peers[userId];
                        const senders = pc.getSenders();
                        let changed = false;
                        let micSender = null;
                        for (const s of senders) {
                            if (s.track && s.track.kind === 'audio') {
                                const isScreenAudio = screenAudioTrack && s.track.id === screenAudioTrack.id;
                                if (!isScreenAudio) {
                                    micSender = s;
                                    break;
                                }
                            }
                        }

                        if (micSender) {
                            if (micSender.track !== micAudioTrack) {
                                micSender.replaceTrack(micAudioTrack).catch(() => {});
                                changed = true;
                            }
                        } else {
                            let nullSenderFound = false;
                            for (const s of senders) {
                                if (!s.track || s.track === null) {
                                    s.replaceTrack(micAudioTrack).catch(() => {});
                                    nullSenderFound = true;
                                    changed = true;
                                    break;
                                }
                            }

                            if (!nullSenderFound) {
                                pc.addTrack(micAudioTrack, localStream);
                                changed = true;
                            }
                        }

                        if (changed) {
                            negotiate(userId, pc);
                        }
                    }
                }

                document.querySelectorAll('video, audio').forEach(el => {
                    if (el.id !== 'localVideo' && el.id !== 'previewVideo') {
                        el.muted = el.dataset.wasMuted === 'true';
                    }
                });
            }

            updateLocalLabel();

            if (ws && ws.readyState === WebSocket.OPEN) {
                ws.send(JSON.stringify({
                    type: 'update-user',
                    data: {
                        isMuted: isDeafened || !micAudioTrack || !micAudioTrack.enabled,
                        isDeafened: isDeafened,
                        micTrackId: micAudioTrack ? micAudioTrack.id : null,
                        screenAudioTrackId: screenAudioTrack ? screenAudioTrack.id : null
                    }
                }));
            }
            savePreferences();
        }

        let camToggleInProgress = false;

        async function toggleCam() {
            if (camToggleInProgress || !isCameraReady) return;

            const btn = document.getElementById('btnCam');
            if (!localStream) return;

            camToggleInProgress = true;
            isCameraReady = false;
            btn.disabled = true;

            try {
                let tracks = localStream.getVideoTracks();

                let trackIsBroken = false;
                if (tracks.length > 0) {
                    const track = tracks[0];
                    if (track.readyState === 'ended' || track.muted || !track.enabled) {
                        trackIsBroken = true;
                        console.warn("Camera track is broken/disabled, cleaning up");
                        track.stop();
                        localStream.removeTrack(track);
                        tracks = [];
                    }
                }

                if (tracks.length === 0 || trackIsBroken) {

                    btn.innerHTML = `<svg class="spinner" xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M21 12a9 9 0 1 1-6.219-8.56"/></svg>`;

                    try {
                        const newStream = await navigator.mediaDevices.getUserMedia({ video: true });
                        const newTrack = newStream.getVideoTracks()[0];

                        if (!newTrack || newTrack.readyState !== 'live') {
                            console.warn("Camera track not properly initialized, retrying...");
                            newTrack?.stop();
                            await new Promise(r => setTimeout(r, 100));
                            const retryStream = await navigator.mediaDevices.getUserMedia({ video: true });
                            const retryTrack = retryStream.getVideoTracks()[0];
                            if (retryTrack) {
                                retryTrack.enabled = true;
                                localStream.addTrack(retryTrack);
                                retryStream.getTracks().forEach(t => { if (t !== retryTrack) t.stop(); });
                            }
                        } else {
                            newTrack.enabled = true;
                            localStream.addTrack(newTrack);
                        }

                        tracks = localStream.getVideoTracks();

                        if (!screenStream) {
                            for (const userId in peers) {
                                const pc = peers[userId];
                                const sender = pc.getSenders().find(s => s.track && s.track.kind === 'video');
                                if (sender) {
                                    sender.replaceTrack(newTrack);
                                } else {
                                    pc.addTrack(newTrack, localStream);
                                }
                                negotiate(userId, pc);
                            }
                        }

                        btn.classList.remove('active-red');
                        btn.innerHTML = `<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M14.5 4h-5L7 7H4a2 2 0 0 0-2 2v9a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2V9a2 2 0 0 0-2-2h-3l-2.5-3z"/><circle cx="12" cy="13" r="3"/></svg>`;

                        const localVideo = document.getElementById('localVideo');
                        if (localVideo) {
                            localVideo.srcObject = null;
                            localVideo.srcObject = localStream;
                        }

                        if (ws && ws.readyState === WebSocket.OPEN) {
                            ws.send(JSON.stringify({
                                type: 'cam-toggle',
                                data: { enabled: true }
                            }));
                        }

                        pendingCamToggle = false;

                        updateLocalAvatar();
                        savePreferences();
                        return;
                    } catch (e) {
                        console.error("Could not add camera", e);
                        alert("Could not access camera. Please check permissions.");
                        btn.classList.add('active-red');
                        btn.innerHTML = `<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M14.5 4h-5L7 7H4a2 2 0 0 0-2 2v9a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2V9a2 2 0 0 0-2-2h-3l-2.5-3z"/><circle cx="12" cy="13" r="3"/></svg>`;
                        updateLocalAvatar();
                        return;
                    }
                }

                const track = tracks[0];

                if (track.enabled) {

                    for (const userId in peers) {
                        const pc = peers[userId];
                        const sender = pc.getSenders().find(s => s.track && s.track.kind === 'video');
                        if (sender) {
                            pc.removeTrack(sender);
                        }
                    }

                    track.stop();
                    localStream.removeTrack(track);

                    const localVideo = document.getElementById('localVideo');
                    if (localVideo) {
                        localVideo.srcObject = null;
                    }

                    btn.classList.add('active-red');
                    btn.innerHTML = `<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="1" y1="1" x2="23" y2="23"></line><path d="M21 21l-3.5-3.5m-2-2l-2-2m-2-2l-2-2m-2-2l-3.5-3.5"></path><path d="M15 7h5a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2h-5"></path><path d="M4 8v8a2 2 0 0 0 2 2h4.5"></path></svg>`;

                    if (ws && ws.readyState === WebSocket.OPEN) {
                        ws.send(JSON.stringify({
                            type: 'cam-toggle',
                            data: { enabled: false }
                        }));
                    }

                    pendingCamToggle = true;
                } else {

                    btn.classList.remove('active-red');
                    btn.innerHTML = `<svg class="spinner" xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M21 12a9 9 0 1 1-6.219-8.56"/></svg>`;

                    try {
                        const oldTrack = localStream.getVideoTracks()[0];
                        if (oldTrack) {
                            oldTrack.stop();
                            localStream.removeTrack(oldTrack);
                        }

                        const newStream = await navigator.mediaDevices.getUserMedia({ video: true });
                        const newTrack = newStream.getVideoTracks()[0];

                        if (!newTrack || newTrack.readyState !== 'live') {
                            console.warn("Camera track not properly initialized, retrying...");
                            newTrack?.stop();
                            await new Promise(r => setTimeout(r, 100));
                            const retryStream = await navigator.mediaDevices.getUserMedia({ video: true });
                            const retryTrack = retryStream.getVideoTracks()[0];
                            if (retryTrack) {
                                retryTrack.enabled = true;
                                localStream.addTrack(retryTrack);
                                retryStream.getTracks().forEach(t => { if (t !== retryTrack) t.stop(); });
                            }
                        } else {
                            newTrack.enabled = true;
                            localStream.addTrack(newTrack);
                        }

                        if (!screenStream) {
                            for (const userId in peers) {
                                const pc = peers[userId];
                                const sender = pc.getSenders().find(s => s.track && s.track.kind === 'video');
                                if (sender) {
                                    sender.replaceTrack(newTrack);
                                } else {
                                    pc.addTrack(newTrack, localStream);
                                }
                                negotiate(userId, pc);
                            }
                        }

                        btn.innerHTML = `<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M14.5 4h-5L7 7H4a2 2 0 0 0-2 2v9a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2V9a2 2 0 0 0-2-2h-3l-2.5-3z"/><circle cx="12" cy="13" r="3"/></svg>`;

                        const localVideo = document.getElementById('localVideo');
                        if (localVideo) {
                            localVideo.srcObject = null;
                            localVideo.srcObject = localStream;
                        }

                        if (ws && ws.readyState === WebSocket.OPEN) {
                            ws.send(JSON.stringify({
                                type: 'cam-toggle',
                                data: { enabled: true }
                            }));
                        }

                        pendingCamToggle = false;
                    } catch (e) {
                        console.error("Could not re-add camera", e);
                        alert("Could not access camera. Please check permissions.");
                        btn.classList.add('active-red');
                        btn.innerHTML = `<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="1" y1="1" x2="23" y2="23"></line><path d="M21 21l-3.5-3.5m-2-2l-2-2m-2-2l-2-2m-2-2l-3.5-3.5"></path><path d="M15 7h5a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2h-5"></path><path d="M4 8v8a2 2 0 0 0 2 2h4.5"></path></svg>`;
                    }
                }

                updateLocalAvatar();
                savePreferences();
            } finally {
                camToggleInProgress = false;
                isCameraReady = true;
                btn.disabled = false;
            }
        }

        function isMobileDevice() {
            const ua = navigator.userAgent || navigator.vendor || window.opera;
            const isIOS = /iPad|iPhone|iPod/.test(ua) || (navigator.platform === 'MacIntel' && navigator.maxTouchPoints > 1);
            const isAndroid = /Android/.test(ua);
            const isMobile = /Mobile|Android|Silk/.test(ua) || isIOS || isAndroid;
            return isMobile;
        }

        async function toggleScreen() {

            if (isMobileDevice()) {
                alert('Screen sharing is not supported on mobile devices.');
                return;
            }

            const btn = document.getElementById('btnShare');

            if (screenStream) {
                let videoTrack = localStream ? localStream.getVideoTracks()[0] : null;
                const screenAudioTrack = screenStream.getAudioTracks()[0];

                screenStream.getTracks().forEach(t => t.stop());
                screenStream = null;
                btn.classList.remove('active-green');

                if (localStream) {
                    localVideo.srcObject = localStream;
                } else {
                    localVideo.srcObject = null;
                }

                if (ws && ws.readyState === WebSocket.OPEN) {
                    ws.send(JSON.stringify({
                        type: 'screen-toggle',
                        data: { enabled: false, hasAudio: false, screenAudioTrackId: null }
                    }));
                }

                for (const userId in peers) {
                    const pc = peers[userId];
                    const senders = pc.getSenders();
                    let shouldNegotiate = false;

                    const vidSender = senders.find(s => s.track && s.track.kind === 'video');
                    if (vidSender) {
                        if (videoTrack) {
                            vidSender.replaceTrack(videoTrack);
                        } else {
                            pc.removeTrack(vidSender);
                            shouldNegotiate = true;
                        }
                    }

                    if (screenAudioTrack) {
                        const audSender = senders.find(s => s.track && s.track.id === screenAudioTrack.id);
                        if (audSender) {
                            pc.removeTrack(audSender);
                            shouldNegotiate = true;
                        }
                    }

                    if (shouldNegotiate) {
                        negotiate(userId, pc);
                    }
                }

                updateLocalAvatar();

            } else {
                try {
                    screenStream = await navigator.mediaDevices.getDisplayMedia({
                        video: { cursor: true },
                        systemAudio: "include",
                        audio: {
                            echoCancellation: true,
                            noiseSuppression: false,
                            autoGainControl: false,
                            channelCount: 2,
                            sampleRate: 48000,
                            sampleSize: 16
                        }
                    });
                    const screenTrack = screenStream.getVideoTracks()[0];
                    const screenAudioTrack = screenStream.getAudioTracks()[0];

                    if (screenAudioTrack) {
                        screenAudioTrack.contentHint = "music";
                    }

                    localVideo.srcObject = screenStream;

                    updateLocalAvatar();

                    if (ws && ws.readyState === WebSocket.OPEN) {
                        ws.send(JSON.stringify({
                            type: 'screen-toggle',
                            data: {
                                enabled: true,
                                hasAudio: !!screenAudioTrack,
                                screenAudioTrackId: screenAudioTrack ? screenAudioTrack.id : null
                            }
                        }));
                    }

                    for (const userId in peers) {
                        const pc = peers[userId];
                        const senders = pc.getSenders();
                        const vidSender = senders.find(s => s.track && s.track.kind === 'video');
                        let shouldNegotiate = false;

                        if (vidSender) {
                            vidSender.replaceTrack(screenTrack);
                        } else {
                            if (localStream) {
                                pc.addTrack(screenTrack, localStream);
                            } else {
                                pc.addTrack(screenTrack, screenStream);
                            }
                            shouldNegotiate = true;
                        }

                        if (screenAudioTrack) {
                            let sender = pc.addTrack(screenAudioTrack, screenStream);

                            const params = sender.getParameters();
                            if (!params.encodings) params.encodings = [{}];
                            params.encodings[0].maxBitrate = 512000;
                            sender.setParameters(params).catch(e => console.warn(e));

                            shouldNegotiate = true;
                        }

                        if (shouldNegotiate) {
                            negotiate(userId, pc);
                        }
                    }

                    screenTrack.onended = () => { toggleScreen(); };
                    btn.classList.add('active-green');
                } catch (e) {
                    console.error("Screen share failed", e);
                }
            }
        }

        function updateLocalLabel() {
            const label = document.getElementById('localLabel');
            if (!label) return;

            let statusIcons = '';
            if (isDeafened) {
                statusIcons = `<span class="ml-1.5 inline-flex items-center text-red-500"><svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><line x1="1" y1="1" x2="23" y2="23"></line><path d="M21 14a2 2 0 0 0-2-2h-3a2 2 0 0 0-2 2v3a2 2 0 0 0 2 2h1a2 2 0 0 0 2-2V14z"></path><path d="M3 14a2 2 0 0 1 2-2h3a2 2 0 0 1 2 2v3a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V14z"></path><path d="M20.4 10.4C20.2 6.5 17 3.5 13 3.1"></path><path d="M6.5 5.5A9 9 0 0 0 3 12"></path></svg></span>`;
            } else {
                const audioTrack = localStream ? localStream.getAudioTracks()[0] : null;
                if (!audioTrack || !audioTrack.enabled) {
                    statusIcons = `<span class="ml-1.5 inline-flex items-center text-red-500"><svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><line x1="1" y1="1" x2="23" y2="23"></line><path d="M9 9v3a3 3 0 0 0 5.12 2.12M15 9.34V4a3 3 0 0 0-5.94-.6"></path><path d="M17 16.95A7 7 0 0 1 5 12v-2m14 0v2a7 7 0 0 1-.11 1.23"></path></svg></span>`;
                }
            }

            label.innerHTML = `<span class="flex items-center">${userNickname} (You)${statusIcons}</span>`;
        }

        function copyLink() {
            navigator.clipboard.writeText(window.location.href);

            const btn = document.getElementById('btnCopy');
            if (btn.classList.contains('bg-green-600')) return;

            const icon = document.getElementById('iconCopy');

            const originalHTML = btn.innerHTML;
            const originalClass = btn.className;

            btn.innerHTML = `<span class="text-xs md:text-sm font-medium text-white">Copied!</span><svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="20 6 9 17 4 12"/></svg>`;
            btn.classList.add('bg-green-600', 'hover:bg-green-700');
            btn.classList.remove('hover:bg-slate-700/50');

            setTimeout(() => {
                btn.innerHTML = originalHTML;
                btn.className = originalClass;
            }, 2000);
        }

        const settingsOverlay = document.getElementById('settingsOverlay');
        const settingsNicknameInput = document.getElementById('settingsNicknameInput');
        const settingsAvatarInput = document.getElementById('settingsAvatarInput');
        const settingsAvatarPreview = document.getElementById('settingsAvatarPreview');
        const settingsAvatarPlaceholder = document.getElementById('settingsAvatarPlaceholder');
        let newAvatarCandidate = null;
        let settingsInitialAudioId = '';
        let settingsInitialVideoId = '';
        let settingsInitialAudioOutputId = '';

        async function openSettings() {
            settingsNicknameInput.value = userNickname;
            newAvatarCandidate = userAvatar;

            if (userAvatar) {
                settingsAvatarPreview.src = userAvatar;
                settingsAvatarPreview.classList.remove('hidden');
                settingsAvatarPlaceholder.classList.add('hidden');
            } else {
                settingsAvatarPreview.classList.add('hidden');
                settingsAvatarPlaceholder.classList.remove('hidden');
            }

            await populateSettingsDeviceList();
            const settingsAudio = document.getElementById('settingsAudioSource');
            const settingsVideo = document.getElementById('settingsVideoSource');
            const settingsAudioOutput = document.getElementById('settingsAudioOutputSource');
            settingsInitialAudioId = settingsAudio ? settingsAudio.value : '';
            settingsInitialVideoId = settingsVideo ? settingsVideo.value : '';
            settingsInitialAudioOutputId = settingsAudioOutput ? settingsAudioOutput.value : '';
            settingsOverlay.classList.remove('hidden');
            if (localStream) {
                await setupVolumeMeter(localStream, 'settingsMicBar');
            }
        }

        function closeSettings() {
            settingsOverlay.classList.add('hidden');
            if (settingsMeterFrameId) cancelAnimationFrame(settingsMeterFrameId);
        }

        function handleSettingsAvatarUpload(input) {
            const file = input.files[0];
            if (!file) return;

            const reader = new FileReader();
            reader.onload = function(e) {
                openCropModal(e.target.result, 'settings');
            };
            reader.readAsDataURL(file);
            input.value = '';
        }

        async function saveSettings() {
            const newAudio = document.getElementById('settingsAudioSource').value;
            const newAudioOutput = document.getElementById('settingsAudioOutputSource').value;
            const newVideo = document.getElementById('settingsVideoSource').value;

            if (newAudio !== settingsInitialAudioId || newVideo !== settingsInitialVideoId) {
                await switchMediaStream(newAudio, newVideo);
            }

            if (newAudioOutput !== settingsInitialAudioOutputId) {
                await changeAudioOutput(newAudioOutput);
            }

            userNickname = settingsNicknameInput.value.trim() || "Guest";
            userAvatar = newAvatarCandidate;
            savePreferences();

            updateLocalLabel();
            updateLocalAvatar();

            if (ws && ws.readyState === WebSocket.OPEN) {
                 ws.send(JSON.stringify({
                    type: "update-user",
                    data: {
                        nickname: userNickname,
                        avatar: userAvatar
                    }
                }));
            }

            closeSettings();
        }

        function updateLocalAvatar() {
             const layer = document.getElementById('localAvatarLayer');
             const img = document.getElementById('localAvatarImg');
             const centerImg = document.getElementById('localAvatarCenterImg');
             const placeholder = document.getElementById('localAvatarPlaceholder');

             let camEnabled = false;
             if (localStream) {
                 const videoTrack = localStream.getVideoTracks()[0];
                 if (videoTrack && videoTrack.enabled) camEnabled = true;
             }

             if (screenStream || camEnabled) {
                 if (screenStream) {
                     layer.style.display = 'none';
                 } else {
                    layer.style.display = 'none';
                 }
             } else {
                 layer.style.display = 'flex';
                 if (userAvatar) {
                     img.classList.remove('hidden');

                     centerImg.src = userAvatar;
                     centerImg.classList.remove('hidden');
                     placeholder.classList.add('hidden');
                 } else {
                     img.classList.add('hidden');
                     centerImg.classList.add('hidden');
                     placeholder.classList.remove('hidden');
                 }
             }
        }

        (function() {
            const pip = document.getElementById('localPipWrapper');
            const taskbar = document.querySelector('.taskbar');
            const connectionDot = document.getElementById('connectionDot');
            const btnCopy = document.getElementById('btnCopy');
            const sidebar = document.getElementById('roomSidebar');

            let isDragging = false;
            let dragOffset = { x: 0, y: 0 };
            let dragBounds = null;
            let pendingFrame = false;
            let collisionRects = null;
            let lastX = 0;
            let lastY = 0;

            function startDrag(clientX, clientY) {
                isDragging = true;
                pip.style.cursor = 'grabbing';
                pip.style.transition = 'none';

                const rect = pip.getBoundingClientRect();
                const taskbarRect = taskbar.getBoundingClientRect();
                const sidebarRect = sidebar && sidebar.classList.contains('open') ? sidebar.getBoundingClientRect() : null;

                pip.style.bottom = 'auto';
                pip.style.right = 'auto';
                pip.style.left = rect.left + 'px';
                pip.style.top = rect.top + 'px';

                dragOffset.x = clientX - rect.left;
                dragOffset.y = clientY - rect.top;

                lastX = clientX;
                lastY = clientY;

                let minX = 16;
                let maxX = window.innerWidth - rect.width - 16;
                if (sidebarRect) {
                    minX = sidebarRect.right + 16;
                }

                dragBounds = {
                    minX: minX,
                    maxX: maxX,
                    minY: 16,
                    maxY: window.innerHeight - taskbarRect.height - rect.height - 16
                };

                const margin = 16;
                collisionRects = {
                    statusRect: connectionDot && connectionDot.parentElement ? connectionDot.parentElement.getBoundingClientRect() : null,
                    copyRect: btnCopy ? btnCopy.getBoundingClientRect() : null,
                    sidebarRect: sidebarRect,
                    margin: margin,
                    pipWidth: rect.width
                };
            }

            function onMouseDown(e) {
                if (e.target.closest('button') || e.target.closest('input')) return;

                e.preventDefault();

                startDrag(e.clientX, e.clientY);
                document.addEventListener('mousemove', onMouseMove);
                document.addEventListener('mouseup', onMouseUp);
            }

            function onTouchStart(e) {
                if (e.target.closest('button') || e.target.closest('input')) return;

                const touch = e.touches[0];
                startDrag(touch.clientX, touch.clientY);

                document.addEventListener('touchmove', onTouchMove, { passive: false });
                document.addEventListener('touchend', onTouchEnd);
                document.addEventListener('touchcancel', onTouchEnd);
            }

            function handleMove(clientX, clientY) {
                lastX = clientX;
                lastY = clientY;

                if (!isDragging || pendingFrame) return;

                pendingFrame = true;

                requestAnimationFrame(() => {
                    if (!isDragging) {
                        pendingFrame = false;
                        return;
                    }

                    let newX = lastX - dragOffset.x;
                    let newY = lastY - dragOffset.y;

                    if (dragBounds) {
                        newX = Math.max(dragBounds.minX, Math.min(newX, dragBounds.maxX));
                        newY = Math.max(dragBounds.minY, Math.min(newY, dragBounds.maxY));
                    }

                    if (collisionRects) {
                        const { statusRect, copyRect, sidebarRect, margin, pipWidth } = collisionRects;

                        if (statusRect) {
                            const dangerRight = statusRect.right + margin;
                            const dangerBottom = statusRect.bottom + margin;

                            if (newX < dangerRight && newY < dangerBottom) {
                                const distToRight = dangerRight - newX;
                                const distToBottom = dangerBottom - newY;
                                if (distToRight < distToBottom) newX = dangerRight;
                                else newY = dangerBottom;
                            }
                        }

                        if (copyRect) {
                            const dangerLeft = copyRect.left - margin - pipWidth;
                            const dangerBottom = copyRect.bottom + margin;

                            if (newX > dangerLeft && newY < dangerBottom) {
                                const distToLeft = newX - dangerLeft;
                                const distToBottom = dangerBottom - newY;
                                if (distToLeft < distToBottom) newX = dangerLeft;
                                else newY = dangerBottom;
                            }
                        }

                        if (sidebarRect) {
                            const dangerRight = sidebarRect.right + margin;
                            const dangerBottom = sidebarRect.bottom + margin;

                            if (newX < dangerRight && newY < dangerBottom) {
                                const distToRight = dangerRight - newX;
                                const distToBottom = dangerBottom - newY;
                                if (distToRight < distToBottom) newX = dangerRight;
                                else newY = dangerBottom;
                            }
                        }
                    }

                    pip.style.left = newX + 'px';
                    pip.style.top = newY + 'px';
                    pendingFrame = false;
                });
            }

            function onMouseMove(e) {
                handleMove(e.clientX, e.clientY);
            }

            function onTouchMove(e) {
                if (e.cancelable) e.preventDefault();
                const touch = e.touches[0];
                handleMove(touch.clientX, touch.clientY);
            }

            function onMouseUp() {
                isDragging = false;
                pip.style.cursor = 'grab';
                pip.style.transition = '';
                document.removeEventListener('mousemove', onMouseMove);
                document.removeEventListener('mouseup', onMouseUp);
            }

            function onTouchEnd() {
                isDragging = false;
                pip.style.cursor = 'grab';
                pip.style.transition = '';
                document.removeEventListener('touchmove', onTouchMove);
                document.removeEventListener('touchend', onTouchEnd);
                document.removeEventListener('touchcancel', onTouchEnd);
            }

            pip.addEventListener('mousedown', onMouseDown);
            pip.addEventListener('touchstart', onTouchStart, { passive: false });

            let lastOrientation = window.innerWidth > window.innerHeight ? 'landscape' : 'portrait';
            let resizeTimeoutId = null;
            window.addEventListener('resize', () => {
                if (resizeTimeoutId) clearTimeout(resizeTimeoutId);

                resizeTimeoutId = setTimeout(() => {
                    const currentOrientation = window.innerWidth > window.innerHeight ? 'landscape' : 'portrait';
                    const isScreenFlip = currentOrientation !== lastOrientation;
                    lastOrientation = currentOrientation;

                    pip.style.left = '';
                    pip.style.top = '';
                    pip.style.bottom = '';
                    pip.style.right = '';

                    if (isScreenFlip) {
                        return;
                    }

                }, 250);
            });
        })();

        let idleTimer = null;
        document.addEventListener('mousemove', () => {
            if (document.fullscreenElement && document.fullscreenElement.classList.contains('video-container')) {
                document.fullscreenElement.classList.remove('idle-fullscreen');
                clearTimeout(idleTimer);
                idleTimer = setTimeout(() => {
                    if (document.fullscreenElement && document.fullscreenElement.classList.contains('video-container')) {
                        document.fullscreenElement.classList.add('idle-fullscreen');
                    }
                }, 2500);
            }
        });

        document.addEventListener('fullscreenchange', () => {
            if (!document.fullscreenElement) {
                clearTimeout(idleTimer);
                document.querySelectorAll('.video-container.idle-fullscreen').forEach(el => el.classList.remove('idle-fullscreen'));
            }
        });

        let currentCroppie = null;
        let currentCropTarget = null;

        function openCropModal(imageUrl, target) {
            currentCropTarget = target;
            const modal = document.getElementById('cropModal');
            const wrapper = document.getElementById('cropWrapper');
            wrapper.innerHTML = '';
            modal.classList.remove('hidden');

            currentCroppie = new Croppie(wrapper, {
                viewport: { width: 200, height: 200, type: 'square' },
                boundary: { width: '100%', height: 250 },
                showZoomer: true,
                enableOrientation: true
            });
            currentCroppie.bind({ url: imageUrl, zoom: 0 });
        }

        function closeCropModal() {
            document.getElementById('cropModal').classList.add('hidden');
            if (currentCroppie) {
                currentCroppie.destroy();
                currentCroppie = null;
            }
        }

        function applyCrop() {
            if (!currentCroppie) return;
            currentCroppie.result({
                type: 'base64',
                size: { width: 400, height: 400 },
                format: 'jpeg',
                quality: 0.8
            }).then(function(base64) {
                if (currentCropTarget === 'setup') {
                    userAvatar = base64;
                    avatarPreview.src = userAvatar;
                    avatarPreview.classList.remove('hidden');
                    avatarPlaceholder.classList.add('hidden');
                } else if (currentCropTarget === 'settings') {
                    newAvatarCandidate = base64;
                    settingsAvatarPreview.src = newAvatarCandidate;
                    settingsAvatarPreview.classList.remove('hidden');
                    settingsAvatarPlaceholder.classList.add('hidden');
                }
                closeCropModal();
            });
        }
    </script>

    <div id="cropModal" class="fixed inset-0 z-[250] flex items-center justify-center p-4 hidden" style="background: rgba(0, 0, 0, 0.95); backdrop-filter: blur(20px) saturate(140%);">
        <div class="glass-panel p-6 md:p-8 rounded-3xl w-full max-w-md max-h-[95vh] flex flex-col items-center relative z-10">
            <h3 class="text-xl font-bold tracking-tight mb-4" style="color: var(--text-primary);">Crop Your Avatar</h3>
            <div id="cropWrapper" class="w-full relative"></div>
            <div class="flex gap-4 w-full mt-2">
                <button onclick="closeCropModal()" class="btn-secondary flex-1 py-3 text-white rounded-xl font-medium transition-all">Cancel</button>
                <button onclick="applyCrop()" class="btn-primary flex-1 py-3 text-white rounded-xl font-medium transition-all">Crop & Save</button>
            </div>
        </div>
    </div>
</body>
</html>
"###;
    html.replace("{{TURN_URL}}", turn_url)
        .replace("{{TURN_USERNAME}}", turn_username)
        .replace("{{TURN_CREDENTIAL}}", turn_credential)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UserStatus {
    pub nickname: String,
    pub avatar: Option<String>,
    pub is_muted: bool,
    pub is_deafened: bool,
    pub is_screen_sharing: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RoomStatus {
    name: String,
    users: HashMap<String, UserStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SignalMessage {
    #[serde(rename = "type")]
    pub msg_type: String,
    pub target: Option<String>,
    pub data: Option<serde_json::Value>,
    #[serde(rename = "userId")]
    pub user_id: Option<String>,
}

type UserTx = tokio::sync::mpsc::Sender<Result<Message, axum::Error>>;
type ChannelMap = HashMap<String, HashMap<String, (UserTx, UserStatus)>>;
type RoomMap = Arc<Mutex<HashMap<String, ChannelMap>>>;
type RoomCleanupMap = Arc<Mutex<HashMap<String, u64>>>;
type RemoteUsersMap = Arc<Mutex<HashMap<String, HashMap<String, HashMap<String, UserStatus>>>>>;
const ROOM_EMPTY_GRACE_SECS: u64 = 120;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ClusterMessage {
    #[serde(rename = "type")]
    msg_type: String,
    room_id: String,
    channel_id: String,
    user_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    status: Option<UserStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    signal_msg: Option<String>,
}

#[derive(Clone)]
struct AppState {
    rooms: RoomMap,
    room_cleanup_generations: RoomCleanupMap,
    room_creation_password: Option<String>,
    cluster_tx: tokio::sync::broadcast::Sender<String>,
    remote_users: RemoteUsersMap,
    cluster_key: Option<String>,
    connected_peers: Arc<Mutex<HashSet<String>>>,
    node_id: String,
}

#[tokio::main]
async fn main() {
    let rooms: RoomMap = Arc::new(Mutex::new(HashMap::new()));
    let room_cleanup_generations: RoomCleanupMap = Arc::new(Mutex::new(HashMap::new()));

    let room_creation_password = std::env::var("ROOM_CREATION_PASSWORD").ok().filter(|s| !s.is_empty());
    let cluster_key = std::env::var("KEY").ok().filter(|s| !s.is_empty());
    let (cluster_tx, _) = tokio::sync::broadcast::channel::<String>(10000);
    let remote_users: RemoteUsersMap = Arc::new(Mutex::new(HashMap::new()));

    let node_id = {
        let mut rng = rand::thread_rng();
        let bytes: [u8; 4] = rng.r#gen();
        format!("{:02X}{:02X}{:02X}{:02X}", bytes[0], bytes[1], bytes[2], bytes[3])
    };
    println!("NODE ID: {}", node_id);

    if cluster_key.is_some() {
        println!("CLUSTER: Enabled via KEY env var (DHT discovery)");
    }

    let state = AppState {
        rooms,
        room_cleanup_generations,
        room_creation_password,
        cluster_tx,
        remote_users,
        cluster_key,
        connected_peers: Arc::new(Mutex::new(HashSet::new())),
        node_id,
    };

    let app = Router::new()
        .route("/", get(index))
        .route("/new", get(new_room))
        .route("/new/", get(redirect_new_trailing_slash))
        .route("/{room_id}", get(index))
        .route("/{room_id}/", get(redirect_room_trailing_slash))
        .route("/{room_id}/{channel_id}", get(index))
        .route("/{room_id}/{channel_id}/", get(redirect_channel_trailing_slash))
        .route("/rnnoise.js", get(rnnoise_js))
        .route("/rnnoise_processor.js", get(rnnoise_processor_js))
        .route("/manifest.json", get(manifest_json))
        .route("/service-worker.js", get(service_worker_js))
        .route("/icon.svg", get(icon_svg))
        .route("/assets/tailwind.js", get(tailwind_js))
        .route("/assets/croppie.min.js", get(croppie_js))
        .route("/assets/croppie.min.css", get(croppie_css))
        .route("/assets/inter.css", get(inter_css))
        .route("/fonts/inter-cyrillic-ext.woff2", get(inter_cyrillic_ext_woff2))
        .route("/fonts/inter-cyrillic.woff2", get(inter_cyrillic_woff2))
        .route("/fonts/inter-greek-ext.woff2", get(inter_greek_ext_woff2))
        .route("/fonts/inter-greek.woff2", get(inter_greek_woff2))
        .route("/fonts/inter-vietnamese.woff2", get(inter_vietnamese_woff2))
        .route("/fonts/inter-latin-ext.woff2", get(inter_latin_ext_woff2))
        .route("/fonts/inter-latin.woff2", get(inter_latin_woff2))
        .route("/ws/{room_id}/{channel_id}", get(ws_handler))
        .route("/ws/{room_id}/{channel_id}/", get(redirect_ws_trailing_slash))
        .route("/cluster-ws", get(cluster_ws_handler))
        .route("/node-id", get(node_id_handler))
        .route("/ping", get(ping_handler))
        .route("/cluster-peers", get(cluster_peers_handler))
        .with_state(state.clone());

    let port = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(3000);

    let listener = match tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port)).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("ERROR: Failed to bind to port {}: {}", port, e);
            eprintln!("Is the server already running? Try killing the process using this port.");
            std::process::exit(1);
        }
    };
    println!("SERVER RUNNING ON PORT {}", port);

    if state.cluster_key.is_some() {
        spawn_dht_discovery(state.clone(), port);
    }

    axum::serve(listener, app).await.unwrap();
}

async fn new_room(
    State(_state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Redirect, (axum::http::StatusCode, &'static str)> {
    if let Ok(password) = std::env::var("ROOM_CREATION_PASSWORD") {
        if !password.is_empty() {
             match params.get("password") {
                 Some(p) if p == &password => {},
                 _ => return Err((axum::http::StatusCode::UNAUTHORIZED, "Unauthorized")),
             }
        }
    }

    let room_id = if let Some(custom_name) = params.get("name") {
        if custom_name.is_empty() {
            Uuid::new_v4().to_string()
        } else {
            custom_name.clone()
        }
    } else {
        Uuid::new_v4().to_string()
    };

    Ok(Redirect::to(&format!("/{}/General", room_id)))
}

async fn redirect_room_trailing_slash(Path(room_id): Path<String>) -> Redirect {
    Redirect::to(&format!("/{}", room_id))
}

async fn redirect_channel_trailing_slash(Path((room_id, channel_id)): Path<(String, String)>) -> Redirect {
    Redirect::to(&format!("/{}/{}", room_id, channel_id))
}

async fn redirect_new_trailing_slash() -> Redirect {
    Redirect::to("/new")
}

async fn redirect_ws_trailing_slash(Path((room_id, channel_id)): Path<(String, String)>) -> Redirect {
    Redirect::to(&format!("/ws/{}/{}", room_id, channel_id))
}

async fn index(State(_state): State<AppState>) -> impl IntoResponse {
    let turn_url = std::env::var("TURN_URL").unwrap_or_default();
    let turn_username = std::env::var("TURN_USERNAME").unwrap_or_default();
    let turn_credential = std::env::var("TURN_CREDENTIAL").unwrap_or_default();

    let html = get_html_page(&turn_url, &turn_username, &turn_credential);
    (
        [(
            header::CONTENT_SECURITY_POLICY,
            "default-src 'self'; script-src 'self' 'unsafe-inline' 'wasm-unsafe-eval'; style-src 'self' 'unsafe-inline'; font-src 'self'; img-src 'self' data: https: blob:; connect-src 'self' wss: ws:; media-src 'self' blob:; object-src 'none'; frame-ancestors 'none';"
        )],
        Html(html)
    )
}

async fn ws_handler(
    Path((room_id, channel_id)): Path<(String, String)>,
    Query(_params): Query<HashMap<String, String>>,
    ws: WebSocketUpgrade,
    headers: axum::http::HeaderMap,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let channel_id = channel_id.chars().take(32).collect::<String>();
    if let (Some(origin), Some(host)) = (headers.get("origin"), headers.get("host")) {
        if let (Ok(origin_str), Ok(host_str)) = (origin.to_str(), host.to_str()) {
             if !origin_str.ends_with(host_str) {
                  return (axum::http::StatusCode::FORBIDDEN, "Forbidden Origin").into_response();
             }
        }
    }

    ws.max_message_size(8 * 1024 * 1024)
        .on_upgrade(move |socket| handle_socket(socket, room_id, channel_id, state))
}

async fn cluster_ws_handler(
    Query(params): Query<HashMap<String, String>>,
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let key = params.get("key").cloned().unwrap_or_default();
    if let Some(ref cluster_key) = state.cluster_key {
        if key != *cluster_key {
            return (axum::http::StatusCode::FORBIDDEN, "Invalid cluster key").into_response();
        }
    } else {
        return (axum::http::StatusCode::FORBIDDEN, "Clustering not enabled").into_response();
    }
    ws.on_upgrade(move |socket| handle_inbound_cluster(socket, state))
}

async fn node_id_handler(State(state): State<AppState>) -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "application/json")],
        format!("{{\"nodeId\":\"{}\"}}", state.node_id),
    )
}

async fn ping_handler() -> impl IntoResponse {
    "pong"
}

async fn cluster_peers_handler(State(state): State<AppState>) -> impl IntoResponse {
    let peers: Vec<String> = state.connected_peers.lock().await.iter().cloned().collect();
    let peers_json: Vec<String> = peers.iter().map(|p| format!("\"{}\"", p)).collect();
    (
        [(header::CONTENT_TYPE, "application/json")],
        format!("{{\"peers\":[{}],\"nodeId\":\"{}\"}}", peers_json.join(","), state.node_id),
    )
}

async fn handle_inbound_cluster(socket: WebSocket, state: AppState) {
    let (mut ws_tx, mut ws_rx) = socket.split();
    let (write_tx, mut write_rx) = tokio::sync::mpsc::channel::<String>(5000);

    let writer = tokio::spawn(async move {
        while let Some(msg) = write_rx.recv().await {
            if ws_tx.send(Message::Text(msg.into())).await.is_err() { break; }
        }
    });

    let mut cluster_rx = state.cluster_tx.subscribe();

    {
        let rooms_lock = state.rooms.lock().await;
        for (room_id, room) in rooms_lock.iter() {
            for (channel_id, channel) in room.iter() {
                for (user_id, (_, status)) in channel.iter() {
                    let cm = ClusterMessage {
                        msg_type: "user-joined".into(),
                        room_id: room_id.clone(),
                        channel_id: channel_id.clone(),
                        user_id: user_id.clone(),
                        status: Some(status.clone()),
                        data: Some(serde_json::json!({
                            "nickname": status.nickname,
                            "avatar": status.avatar,
                            "isMuted": status.is_muted,
                            "isDeafened": status.is_deafened,
                            "screenEnabled": status.is_screen_sharing,
                        })),
                        signal_msg: None,
                    };
                    if let Ok(json) = serde_json::to_string(&cm) {
                        let _ = write_tx.send(json).await;
                    }
                }
            }
        }
    }

    let write_tx_fwd = write_tx.clone();
    let forwarder = tokio::spawn(async move {
        while let Ok(msg) = cluster_rx.recv().await {
            if write_tx_fwd.send(msg).await.is_err() { break; }
        }
    });

    let rooms = state.rooms.clone();
    let remote_users = state.remote_users.clone();
    let peer_users: Arc<Mutex<HashSet<(String, String, String)>>> = Arc::new(Mutex::new(HashSet::new()));
    let peer_users_cleanup = peer_users.clone();

    while let Some(Ok(msg)) = ws_rx.next().await {
        if let Message::Text(text) = msg {
            if let Ok(cm) = serde_json::from_str::<ClusterMessage>(&text) {
                if cm.msg_type == "user-joined" {
                    peer_users.lock().await.insert((cm.room_id.clone(), cm.channel_id.clone(), cm.user_id.clone()));
                } else if cm.msg_type == "user-left" || cm.msg_type == "user-kicked" {
                    peer_users.lock().await.remove(&(cm.room_id.clone(), cm.channel_id.clone(), cm.user_id.clone()));
                }
                handle_cluster_message(&cm, &rooms, &remote_users).await;
            }
        }
    }

    forwarder.abort();
    writer.abort();
    let dead = peer_users_cleanup.lock().await.clone();
    cleanup_dead_remote_users(&dead, &rooms, &remote_users).await;
}

fn spawn_dht_discovery(state: AppState, port: u16) {
    let key = state.cluster_key.clone().unwrap_or_default();
    tokio::spawn(async move {

        let info_hash = {
            let hash = Sha1::digest(key.as_bytes());
            let mut bytes = [0u8; 20];
            bytes.copy_from_slice(&hash);
            mainline::Id::from_bytes(bytes).expect("SHA1 always produces 20 bytes")
        };
        println!("CLUSTER: DHT infohash = {:?}", info_hash);

        let dht = match tokio::task::spawn_blocking(|| mainline::Dht::client()).await {
            Ok(Ok(d)) => d,
            Ok(Err(e)) => {
                eprintln!("CLUSTER: Failed to start DHT client: {}", e);
                return;
            }
            Err(e) => {
                eprintln!("CLUSTER: DHT task panicked: {}", e);
                return;
            }
        };
        println!("CLUSTER: DHT client started, waiting for bootstrap...");

        let dht_clone = dht.clone();
        let bootstrapped = tokio::task::spawn_blocking(move || dht_clone.bootstrapped()).await.unwrap_or(false);
        if bootstrapped {
            println!("CLUSTER: DHT bootstrapped successfully");
        } else {
            eprintln!("CLUSTER: DHT bootstrap failed, continuing anyway...");
        }

        loop {

            let dht_announce = dht.clone();
            let announce_port = port;
            let announce_hash = info_hash;
            if let Err(e) = tokio::task::spawn_blocking(move || {
                dht_announce.announce_peer(announce_hash, Some(announce_port))
            }).await.unwrap_or(Err(mainline::errors::PutQueryError::NoClosestNodes)) {
                eprintln!("CLUSTER: DHT announce error: {:?}", e);
            } else {
                println!("CLUSTER: Announced on DHT (port {})", port);
            }

            let dht_lookup = dht.clone();
            let lookup_hash = info_hash;
            let peers_result = tokio::task::spawn_blocking(move || {
                let mut all_peers = Vec::new();
                for peers in dht_lookup.get_peers(lookup_hash) {
                    all_peers.extend(peers);
                }
                all_peers
            }).await;

            if let Ok(peers) = peers_result {

                let unique_peers: HashSet<String> = peers.iter()
                    .filter(|p| p.port() != port)
                    .map(|p| p.to_string())
                    .collect();
                if !unique_peers.is_empty() {
                    println!("CLUSTER: DHT found {} unique peer(s)", unique_peers.len());
                }
                for addr_str in unique_peers {

                    {
                        let mut cp = state.connected_peers.lock().await;
                        if cp.contains(&addr_str) {
                            continue;
                        }
                        cp.insert(addr_str.clone());
                    }
                    println!("CLUSTER: Discovered new peer: {}", addr_str);
                    let state_clone = state.clone();
                    let addr_clone = addr_str.clone();
                    tokio::spawn(async move {
                        let url = format!("ws://{}/cluster-ws", addr_clone);
                        let mut failures = 0u32;
                        loop {
                            match connect_to_peer(&url, &state_clone).await {
                                Ok(_) => {
                                    println!("CLUSTER: Connection to {} closed", addr_clone);
                                    failures = 0;
                                }
                                Err(e) => {
                                    failures += 1;
                                    println!("CLUSTER: Connection to {} failed ({}/3): {}", addr_clone, failures, e);
                                    if failures >= 3 {
                                        println!("CLUSTER: Giving up on {} (will retry if re-discovered)", addr_clone);
                                        break;
                                    }
                                }
                            }
                            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                        }

                        state_clone.connected_peers.lock().await.remove(&addr_clone);
                    });
                }
            }

            tokio::time::sleep(std::time::Duration::from_secs(30)).await;
        }
    });
}

async fn connect_to_peer(url: &str, state: &AppState) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let cluster_key = state.cluster_key.as_ref().ok_or("No cluster key")?;
    let sep = if url.contains('?') { "&" } else { "?" };
    let full_url = format!("{}{}key={}", url, sep, cluster_key);

    let (ws_stream, _) = connect_async(&full_url).await?;
    println!("CLUSTER: Connected to peer {}", url);

    let (mut write, mut read) = ws_stream.split();
    let (write_tx, mut write_rx) = tokio::sync::mpsc::channel::<String>(5000);

    let writer = tokio::spawn(async move {
        while let Some(msg) = write_rx.recv().await {
            if write.send(WsMessage::Text(msg.into())).await.is_err() { break; }
        }
    });

    let mut cluster_rx = state.cluster_tx.subscribe();

    {
        let rooms_lock = state.rooms.lock().await;
        for (room_id, room) in rooms_lock.iter() {
            for (channel_id, channel) in room.iter() {
                for (user_id, (_, status)) in channel.iter() {
                    let cm = ClusterMessage {
                        msg_type: "user-joined".into(),
                        room_id: room_id.clone(),
                        channel_id: channel_id.clone(),
                        user_id: user_id.clone(),
                        status: Some(status.clone()),
                        data: Some(serde_json::json!({
                            "nickname": status.nickname,
                            "avatar": status.avatar,
                            "isMuted": status.is_muted,
                            "isDeafened": status.is_deafened,
                            "screenEnabled": status.is_screen_sharing,
                        })),
                        signal_msg: None,
                    };
                    if let Ok(json) = serde_json::to_string(&cm) {
                        let _ = write_tx.send(json).await;
                    }
                }
            }
        }
    }

    let write_tx_fwd = write_tx.clone();
    let forwarder = tokio::spawn(async move {
        while let Ok(msg) = cluster_rx.recv().await {
            if write_tx_fwd.send(msg).await.is_err() { break; }
        }
    });

    let rooms = state.rooms.clone();
    let remote_users = state.remote_users.clone();
    let peer_users: Arc<Mutex<HashSet<(String, String, String)>>> = Arc::new(Mutex::new(HashSet::new()));
    let peer_users_cleanup = peer_users.clone();

    while let Some(Ok(msg)) = read.next().await {
        if let WsMessage::Text(text) = msg {
            let text_str: String = text.to_string();
            if let Ok(cm) = serde_json::from_str::<ClusterMessage>(&text_str) {
                if cm.msg_type == "user-joined" {
                    peer_users.lock().await.insert((cm.room_id.clone(), cm.channel_id.clone(), cm.user_id.clone()));
                } else if cm.msg_type == "user-left" || cm.msg_type == "user-kicked" {
                    peer_users.lock().await.remove(&(cm.room_id.clone(), cm.channel_id.clone(), cm.user_id.clone()));
                }
                handle_cluster_message(&cm, &rooms, &remote_users).await;
            }
        }
    }

    forwarder.abort();
    writer.abort();
    let dead = peer_users_cleanup.lock().await.clone();
    cleanup_dead_remote_users(&dead, &rooms, &remote_users).await;
    Ok(())
}

async fn cleanup_dead_remote_users(
    dead: &HashSet<(String, String, String)>,
    rooms: &RoomMap,
    remote_users: &RemoteUsersMap,
) {
    let mut affected_rooms = HashSet::new();
    for (room_id, channel_id, user_id) in dead {
        {
            let mut remote_lock = remote_users.lock().await;
            if let Some(room) = remote_lock.get_mut(room_id) {
                if let Some(channel) = room.get_mut(channel_id) {
                    channel.remove(user_id);
                }
            }
        }
        {
            let rooms_lock = rooms.lock().await;
            if let Some(room) = rooms_lock.get(room_id) {
                if let Some(channel) = room.get(channel_id) {
                    let notify = serde_json::to_string(&SignalMessage {
                        msg_type: "user-left".into(),
                        user_id: Some(user_id.clone()),
                        target: None,
                        data: None,
                    }).unwrap();
                    for (_, (tx, _)) in channel.iter() {
                        let _ = tx.try_send(Ok(Message::Text(notify.clone().into())));
                    }
                }
            }
        }
        affected_rooms.insert(room_id.clone());
    }
    for room_id in &affected_rooms {
        broadcast_channel_list(rooms, remote_users, room_id).await;
    }
}

async fn handle_cluster_message(
    msg: &ClusterMessage,
    rooms: &RoomMap,
    remote_users: &RemoteUsersMap,
) {
    match msg.msg_type.as_str() {
        "user-joined" => {
            if let Some(ref status) = msg.status {
                {
                    let mut rl = remote_users.lock().await;
                    rl.entry(msg.room_id.clone()).or_default()
                      .entry(msg.channel_id.clone()).or_default()
                      .insert(msg.user_id.clone(), status.clone());
                }
                {
                    let rooms_lock = rooms.lock().await;
                    if let Some(room) = rooms_lock.get(&msg.room_id) {
                        if let Some(channel) = room.get(&msg.channel_id) {
                            let notify = serde_json::to_string(&SignalMessage {
                                msg_type: "user-joined".into(),
                                user_id: Some(msg.user_id.clone()),
                                target: None,
                                data: msg.data.clone(),
                            }).unwrap();
                            for (_, (tx, _)) in channel.iter() {
                                let _ = tx.try_send(Ok(Message::Text(notify.clone().into())));
                            }
                        }
                    }
                }
                broadcast_channel_list(rooms, remote_users, &msg.room_id).await;
            }
        }
        "user-left" | "user-kicked" => {
            {
                let mut rl = remote_users.lock().await;
                if let Some(room) = rl.get_mut(&msg.room_id) {
                    if let Some(channel) = room.get_mut(&msg.channel_id) {
                        channel.remove(&msg.user_id);
                    }
                }
            }
            {
                let mtype = if msg.msg_type == "user-kicked" { "user-kicked" } else { "user-left" };
                let rooms_lock = rooms.lock().await;
                if let Some(room) = rooms_lock.get(&msg.room_id) {
                    if let Some(channel) = room.get(&msg.channel_id) {
                        let notify = serde_json::to_string(&SignalMessage {
                            msg_type: mtype.into(),
                            user_id: Some(msg.user_id.clone()),
                            target: None,
                            data: None,
                        }).unwrap();
                        for (_, (tx, _)) in channel.iter() {
                            let _ = tx.try_send(Ok(Message::Text(notify.clone().into())));
                        }
                    }
                }
            }
            broadcast_channel_list(rooms, remote_users, &msg.room_id).await;
        }
        "user-update" => {
            if let Some(ref status) = msg.status {
                {
                    let mut rl = remote_users.lock().await;
                    if let Some(room) = rl.get_mut(&msg.room_id) {
                        if let Some(channel) = room.get_mut(&msg.channel_id) {
                            if let Some(existing) = channel.get_mut(&msg.user_id) {
                                *existing = status.clone();
                            }
                        }
                    }
                }
                {
                    let rooms_lock = rooms.lock().await;
                    if let Some(room) = rooms_lock.get(&msg.room_id) {
                        if let Some(channel) = room.get(&msg.channel_id) {
                            let full_data = serde_json::to_value(status).unwrap();
                            let notify = serde_json::to_string(&SignalMessage {
                                msg_type: "user-update".into(),
                                user_id: Some(msg.user_id.clone()),
                                target: None,
                                data: Some(full_data),
                            }).unwrap();
                            for (_, (tx, _)) in channel.iter() {
                                let _ = tx.try_send(Ok(Message::Text(notify.clone().into())));
                            }
                        }
                    }
                }
                broadcast_channel_list(rooms, remote_users, &msg.room_id).await;
            }
        }
        "cam-toggle" | "screen-toggle" => {
            if msg.msg_type == "screen-toggle" {
                if let Some(enabled) = msg.data.as_ref().and_then(|d| d.get("enabled")).and_then(|v| v.as_bool()) {
                    let mut rl = remote_users.lock().await;
                    if let Some(room) = rl.get_mut(&msg.room_id) {
                        if let Some(channel) = room.get_mut(&msg.channel_id) {
                            if let Some(s) = channel.get_mut(&msg.user_id) {
                                s.is_screen_sharing = enabled;
                            }
                        }
                    }
                }
            }
            {
                let rooms_lock = rooms.lock().await;
                if let Some(room) = rooms_lock.get(&msg.room_id) {
                    if let Some(channel) = room.get(&msg.channel_id) {
                        let notify = serde_json::to_string(&SignalMessage {
                            msg_type: msg.msg_type.clone(),
                            user_id: Some(msg.user_id.clone()),
                            target: None,
                            data: msg.data.clone(),
                        }).unwrap();
                        for (_, (tx, _)) in channel.iter() {
                            let _ = tx.try_send(Ok(Message::Text(notify.clone().into())));
                        }
                    }
                }
            }
            if msg.msg_type == "screen-toggle" {
                broadcast_channel_list(rooms, remote_users, &msg.room_id).await;
            }
        }
        "rename-channel" => {
            if let Some(ref data) = msg.data {
                let new_name = data.get("newName").and_then(|v| v.as_str()).unwrap_or("").to_string();
                if !new_name.is_empty() {
                    let mut rl = remote_users.lock().await;
                    if let Some(room) = rl.get_mut(&msg.room_id) {
                        if let Some(channel_data) = room.remove(&msg.channel_id) {
                            room.insert(new_name, channel_data);
                        }
                    }
                    drop(rl);
                    broadcast_channel_list(rooms, remote_users, &msg.room_id).await;
                }
            }
        }
        "delete-channel" => {
            let mut rl = remote_users.lock().await;
            if let Some(room) = rl.get_mut(&msg.room_id) {
                room.remove(&msg.channel_id);
            }
            drop(rl);
            broadcast_channel_list(rooms, remote_users, &msg.room_id).await;
        }
        "signal" => {
            if let Some(ref signal_json) = msg.signal_msg {
                if let Ok(signal) = serde_json::from_str::<SignalMessage>(signal_json) {
                    let target_uid = signal.target.as_ref().cloned().unwrap_or_default();
                    if !target_uid.is_empty() {
                        let rooms_lock = rooms.lock().await;
                        if let Some(room) = rooms_lock.get(&msg.room_id) {
                            if let Some(channel) = room.get(&msg.channel_id) {
                                if let Some((target_tx, _)) = channel.get(&target_uid) {
                                    let forwarded = serde_json::to_string(&signal).unwrap();
                                    let _ = target_tx.try_send(Ok(Message::Text(forwarded.into())));
                                }
                            }
                        }
                    }
                }
            }
        }
        _ => {}
    }
}

fn cluster_broadcast(cluster_tx: &tokio::sync::broadcast::Sender<String>, msg: &ClusterMessage) {
    if let Ok(json) = serde_json::to_string(msg) {
        let _ = cluster_tx.send(json);
    }
}

async fn broadcast_channel_list(rooms: &RoomMap, remote_users: &RemoteUsersMap, room_id: &str) {
    let rooms_lock = rooms.lock().await;
    let remote_lock = remote_users.lock().await;

    let local_room = rooms_lock.get(room_id);
    let remote_room = remote_lock.get(room_id);

    if local_room.is_none() && remote_room.is_none() {
        return;
    }

    let mut channel_list: HashMap<String, RoomStatus> = HashMap::new();

    if let Some(room) = local_room {
        for (cid, users) in room.iter() {
            let mut user_map = HashMap::new();
            for (user_id, (_, status)) in users.iter() {
                user_map.insert(user_id.clone(), status.clone());
            }
            channel_list.insert(cid.clone(), RoomStatus {
                name: cid.clone(),
                users: user_map,
            });
        }
    }

    if let Some(remote_room) = remote_room {
        for (cid, users) in remote_room.iter() {
            let entry = channel_list.entry(cid.clone()).or_insert_with(|| RoomStatus {
                name: cid.clone(),
                users: HashMap::new(),
            });
            for (user_id, status) in users.iter() {
                entry.users.insert(user_id.clone(), status.clone());
            }
        }
    }

    let msg = serde_json::to_string(&SignalMessage {
        msg_type: "room-list".into(),
        user_id: None,
        target: None,
        data: Some(serde_json::to_value(channel_list).unwrap()),
    }).unwrap();

    if let Some(room) = local_room {
        for users in room.values() {
            for (tx, _) in users.values() {
                let _ = tx.try_send(Ok(Message::Text(msg.clone().into())));
            }
        }
    }
}

async fn handle_socket(socket: WebSocket, room_id: String, channel_id: String, state: AppState) {
    let rooms = state.rooms.clone();
    let room_cleanup_generations = state.room_cleanup_generations.clone();
    let (mut user_ws_tx, mut user_ws_rx) = socket.split();
    let (tx, mut rx) = tokio::sync::mpsc::channel(5000);

    let mut user_id = String::new();
    let mut is_joined = false;

    tokio::spawn(async move {
        while let Some(result) = rx.recv().await {
            if let Ok(msg) = result {
                if user_ws_tx.send(msg).await.is_err() {
                    break;
                }
            }
        }
    });

    while let Some(result) = user_ws_rx.next().await {
        if let Ok(msg) = result {
            if let Message::Text(text) = msg {
                if let Ok(parsed) = serde_json::from_str::<SignalMessage>(&text) {
                    if parsed.msg_type == "ping" {
                        let pong_msg = serde_json::to_string(&SignalMessage {
                            msg_type: "pong".into(),
                            user_id: None,
                            target: None,
                            data: None,
                        }).unwrap();
                        let _ = tx.try_send(Ok(Message::Text(pong_msg.into())));
                        continue;
                    }

                    if !is_joined {
                        if parsed.msg_type == "join" {
                            user_id = if let Some(ref data) = parsed.data {
                                data.get("userId")
                                    .and_then(|v| v.as_str())
                                    .map(|s| s.to_string())
                                    .unwrap_or_else(|| Uuid::new_v4().to_string())
                            } else {
                                Uuid::new_v4().to_string()
                            };

                            let nickname = parsed.data.as_ref()
                                .and_then(|d| d.get("nickname"))
                                .and_then(|v| v.as_str())
                                .unwrap_or("Guest")
                                .to_string();

                            let mut avatar = parsed.data.as_ref()
                                .and_then(|d| d.get("avatar"))
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string());

                            let is_muted = parsed.data.as_ref()
                                .and_then(|d| d.get("isMuted"))
                                .and_then(|v| v.as_bool())
                                .unwrap_or(false);

                            let is_deafened = parsed.data.as_ref()
                                .and_then(|d| d.get("isDeafened"))
                                .and_then(|v| v.as_bool())
                                .unwrap_or(false);

                            let is_screen_sharing = parsed.data.as_ref()
                                .and_then(|d| d.get("screenEnabled"))
                                .and_then(|v| v.as_bool())
                                .unwrap_or(false);

                            if let Some(ref a) = avatar {
                                if a.len() > 7_000_000 {
                                    avatar = None;
                                }
                            }

                             {
                                let mut rooms_lock = rooms.lock().await;

                                if let Some(ref required_pass) = state.room_creation_password {
                                    if !rooms_lock.contains_key(&room_id) {
                                         let pass_match = if let Some(ref data) = parsed.data {
                                             data.get("password")
                                                 .and_then(|v| v.as_str())
                                                 .map(|p| p == required_pass)
                                                 .unwrap_or(false)
                                         } else {
                                             false
                                         };

                                         if !pass_match {
                                             let error_msg = serde_json::to_string(&SignalMessage {
                                                 msg_type: "error".into(),
                                                 user_id: None,
                                                 target: None,
                                                 data: Some(serde_json::json!({
                                                     "code": "PASSWORD_REQUIRED",
                                                     "message": "Room creation requires a password."
                                                 })),
                                             }).unwrap();
                                             let _ = tx.send(Ok(Message::Text(error_msg.into()))).await;

                                             tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                                             return;
                                         }
                                    }
                                }

                                let room = rooms_lock.entry(room_id.clone()).or_insert_with(HashMap::new);
                                room.entry("General".to_string()).or_insert_with(HashMap::new);
                                let channel = room.entry(channel_id.clone()).or_insert_with(HashMap::new);

                                if channel.contains_key(&user_id) {
                                    let leave_msg = serde_json::to_string(&SignalMessage {
                                        msg_type: "user-left".into(),
                                        user_id: Some(user_id.clone()),
                                        target: None,
                                        data: None,
                                    }).unwrap();

                                    for (uid, (tx, _)) in channel.iter() {
                                        if *uid != user_id {
                                            let _ = tx.try_send(Ok(Message::Text(leave_msg.clone().into())));
                                        }
                                    }
                                    channel.remove(&user_id);
                                }

                                channel.insert(user_id.clone(), (tx.clone(), UserStatus {
                                    nickname: nickname.clone(),
                                    avatar: avatar.clone(),
                                    is_muted,
                                    is_deafened,
                                    is_screen_sharing,
                                }));
                             }

                            if room_cleanup_generations.lock().await.remove(&room_id).is_some() {
                                println!("CLEANUP: Canceled pending deletion for room '{}'", room_id);
                            }
                            is_joined = true;

                             let mut notify_data = parsed.data.clone();
                             if let Some(serde_json::Value::Object(ref mut map)) = notify_data {
                                 if let Some(serde_json::Value::String(avatar)) = map.get("avatar") {
                                     if avatar.len() > 7_000_000 {
                                         map.remove("avatar");
                                     }
                                 }
                                 map.remove("userId");
                             }

                             let notify_msg = serde_json::to_string(&SignalMessage {
                                msg_type: "user-joined".into(),
                                user_id: Some(user_id.clone()),
                                target: None,
                                data: notify_data.clone(),
                            }).unwrap();

                            {
                                let rooms_lock = rooms.lock().await;
                                if let Some(room) = rooms_lock.get(&room_id) {
                                    if let Some(channel) = room.get(&channel_id) {
                                        for (uid, (tx, _)) in channel.iter() {
                                            if *uid != user_id {
                                                let _ = tx.try_send(Ok(Message::Text(notify_msg.clone().into())));
                                            }
                                        }
                                    }
                                }
                            }
                            cluster_broadcast(&state.cluster_tx, &ClusterMessage {
                                msg_type: "user-joined".into(),
                                room_id: room_id.clone(),
                                channel_id: channel_id.clone(),
                                user_id: user_id.clone(),
                                status: Some(UserStatus { nickname: nickname.clone(), avatar: avatar.clone(), is_muted, is_deafened, is_screen_sharing }),
                                data: notify_data.clone(),
                                signal_msg: None,
                            });
                            broadcast_channel_list(&rooms, &state.remote_users, &room_id).await;
                        }
                    } else {
                        if parsed.msg_type == "update-user" {
                            let data = parsed.data.as_ref().and_then(|d| d.as_object());

                            let mut full_status = None;
                            {
                                let mut rooms_lock = rooms.lock().await;
                                if let Some(room) = rooms_lock.get_mut(&room_id) {
                                    if let Some(channel) = room.get_mut(&channel_id) {
                                        if let Some((_, status)) = channel.get_mut(&user_id) {
                                            if let Some(d) = data {
                                                if let Some(n) = d.get("nickname").and_then(|v| v.as_str()) {
                                                    status.nickname = n.to_string();
                                                }
                                                if let Some(a) = d.get("avatar").and_then(|v| v.as_str()) {
                                                    if a.len() <= 7_000_000 {
                                                        status.avatar = Some(a.to_string());
                                                    }
                                                }
                                                if let Some(m) = d.get("isMuted").and_then(|v| v.as_bool()) {
                                                    status.is_muted = m;
                                                }
                                                if let Some(d) = d.get("isDeafened").and_then(|v| v.as_bool()) {
                                                    status.is_deafened = d;
                                                }
                                            }
                                            full_status = Some(status.clone());
                                        }

                                        if let Some(ref status) = full_status {
                                            let full_data = serde_json::to_value(&status).unwrap();

                                            let notify_msg = serde_json::to_string(&SignalMessage {
                                                msg_type: "user-update".into(),
                                                user_id: Some(user_id.clone()),
                                                target: None,
                                                data: Some(full_data),
                                            }).unwrap();

                                            for (uid, (tx, _)) in channel.iter() {
                                                if *uid != user_id {
                                                    let _ = tx.try_send(Ok(Message::Text(notify_msg.clone().into())));
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            if let Some(ref s) = full_status {
                                cluster_broadcast(&state.cluster_tx, &ClusterMessage {
                                    msg_type: "user-update".into(),
                                    room_id: room_id.clone(),
                                    channel_id: channel_id.clone(),
                                    user_id: user_id.clone(),
                                    status: Some(s.clone()),
                                    data: None,
                                    signal_msg: None,
                                });
                            }
                            broadcast_channel_list(&rooms, &state.remote_users, &room_id).await;
                        }
 else if parsed.msg_type == "cam-toggle" {
                            let rooms_lock = rooms.lock().await;
                            if let Some(room) = rooms_lock.get(&room_id) {
                                if let Some(channel) = room.get(&channel_id) {
                                    let notify_msg = serde_json::to_string(&SignalMessage {
                                        msg_type: "cam-toggle".into(),
                                        user_id: Some(user_id.clone()),
                                        target: None,
                                        data: parsed.data.clone(),
                                    }).unwrap();

                                    for (uid, (tx, _)) in channel.iter() {
                                        if *uid != user_id {
                                            let _ = tx.try_send(Ok(Message::Text(notify_msg.clone().into())));
                                        }
                                    }
                                }
                            }
                            cluster_broadcast(&state.cluster_tx, &ClusterMessage {
                                msg_type: "cam-toggle".into(),
                                room_id: room_id.clone(),
                                channel_id: channel_id.clone(),
                                user_id: user_id.clone(),
                                status: None,
                                data: parsed.data.clone(),
                                signal_msg: None,
                            });
                        } else if parsed.msg_type == "screen-toggle" {
                            {
                                let mut rooms_lock = rooms.lock().await;
                                if let Some(room) = rooms_lock.get_mut(&room_id) {
                                    if let Some(channel) = room.get_mut(&channel_id) {
                                        if let Some((_, status)) = channel.get_mut(&user_id) {
                                            if let Some(enabled) = parsed.data.as_ref()
                                                .and_then(|d| d.get("enabled"))
                                                .and_then(|v| v.as_bool())
                                            {
                                                status.is_screen_sharing = enabled;
                                            }
                                        }

                                        let notify_msg = serde_json::to_string(&SignalMessage {
                                            msg_type: "screen-toggle".into(),
                                            user_id: Some(user_id.clone()),
                                            target: None,
                                            data: parsed.data.clone(),
                                        }).unwrap();

                                        for (uid, (tx, _)) in channel.iter() {
                                            if *uid != user_id {
                                                let _ = tx.try_send(Ok(Message::Text(notify_msg.clone().into())));
                                            }
                                        }
                                    }
                                }
                            }

                            cluster_broadcast(&state.cluster_tx, &ClusterMessage {
                                msg_type: "screen-toggle".into(),
                                room_id: room_id.clone(),
                                channel_id: channel_id.clone(),
                                user_id: user_id.clone(),
                                status: None,
                                data: parsed.data.clone(),
                                signal_msg: None,
                            });
                            broadcast_channel_list(&rooms, &state.remote_users, &room_id).await;
                        } else if parsed.msg_type == "kick-user" {
                            let target_user_id = parsed.data.as_ref()
                                .and_then(|d| d.get("userId"))
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string());

                            if let Some(kick_uid) = target_user_id {
                                let mut rooms_lock = rooms.lock().await;
                                let mut kicked = false;
                                let mut kicked_tx = None;

                                if let Some(room) = rooms_lock.get_mut(&room_id) {
                                    if let Some(channel) = room.get_mut(&channel_id) {
                                        if let Some((tx, _)) = channel.remove(&kick_uid) {
                                            kicked = true;
                                            kicked_tx = Some(tx);
                                        }
                                    }
                                }

                                if kicked {
                                    let kick_notify_msg = serde_json::to_string(&SignalMessage {
                                        msg_type: "user-kicked".into(),
                                        user_id: Some(kick_uid.clone()),
                                        target: None,
                                        data: None,
                                    }).unwrap();

                                    if let Some(room) = rooms_lock.get(&room_id) {
                                        if let Some(channel) = room.get(&channel_id) {
                                            for (_uid, (tx, _)) in channel.iter() {
                                                let _ = tx.try_send(Ok(Message::Text(kick_notify_msg.clone().into())));
                                            }
                                        }
                                    }

                                    drop(rooms_lock);

                                    if let Some(kicked_tx) = kicked_tx {
                                        let _ = kicked_tx.try_send(Ok(Message::Text(kick_notify_msg.into())));

                                        let _ = kicked_tx.try_send(Ok(Message::Close(None)));
                                    }

                                    cluster_broadcast(&state.cluster_tx, &ClusterMessage {
                                        msg_type: "user-kicked".into(),
                                        room_id: room_id.clone(),
                                        channel_id: channel_id.clone(),
                                        user_id: kick_uid.clone(),
                                        status: None,
                                        data: None,
                                        signal_msg: None,
                                    });
                                    broadcast_channel_list(&rooms, &state.remote_users, &room_id).await;
                                }
                            }
                        } else if parsed.msg_type == "rename-channel" {
                            let target_channel_id = parsed.data.as_ref()
                                .and_then(|d| d.get("channelId"))
                                .and_then(|v| v.as_str())
                                .unwrap_or(&channel_id)
                                .to_string();

                             let new_name = parsed.data.as_ref()
                                .and_then(|d| d.get("newName"))
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string());

                            if let Some(new_name_str) = new_name {
                                let mut rooms_lock = rooms.lock().await;

                                let can_rename = if let Some(room) = rooms_lock.get(&room_id) {
                                    if let Some(target_channel) = room.get(&target_channel_id) {
                                        target_channel.is_empty()
                                    } else {
                                        false
                                    }
                                } else {
                                    false
                                };

                                if can_rename {
                                     if let Some(room) = rooms_lock.get_mut(&room_id) {
                                         if let Some(channel) = room.remove(&target_channel_id) {
                                             room.insert(new_name_str.clone(), channel);
                                         }
                                     }
                                     drop(rooms_lock);
                                     cluster_broadcast(&state.cluster_tx, &ClusterMessage {
                                         msg_type: "rename-channel".into(),
                                         room_id: room_id.clone(),
                                         channel_id: target_channel_id.clone(),
                                         user_id: user_id.clone(),
                                         status: None,
                                         data: Some(serde_json::json!({ "newName": new_name_str })),
                                         signal_msg: None,
                                     });
                                     broadcast_channel_list(&rooms, &state.remote_users, &room_id).await;
                                }
                            }
                        } else if parsed.msg_type == "delete-channel" {
                            let target_channel_id = parsed.data.as_ref()
                                .and_then(|d| d.get("channelId"))
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string();

                            if !target_channel_id.is_empty() && !target_channel_id.eq_ignore_ascii_case("general") {
                                let mut rooms_lock = rooms.lock().await;

                                let can_delete = if let Some(room) = rooms_lock.get(&room_id) {
                                    if let Some(target_channel) = room.get(&target_channel_id) {
                                        target_channel.is_empty()
                                    } else {
                                        false
                                    }
                                } else {
                                    false
                                };

                                if can_delete {
                                    if let Some(room) = rooms_lock.get_mut(&room_id) {
                                        room.remove(&target_channel_id);
                                    }
                                    drop(rooms_lock);
                                    cluster_broadcast(&state.cluster_tx, &ClusterMessage {
                                        msg_type: "delete-channel".into(),
                                        room_id: room_id.clone(),
                                        channel_id: target_channel_id.clone(),
                                        user_id: user_id.clone(),
                                        status: None,
                                        data: None,
                                        signal_msg: None,
                                    });
                                    broadcast_channel_list(&rooms, &state.remote_users, &room_id).await;
                                }
                            }
                        } else if let Some(ref target_id) = parsed.target {

                            let mut found = false;
                            {
                                let rooms_lock = rooms.lock().await;
                                if let Some(room) = rooms_lock.get(&room_id) {
                                    if let Some(channel) = room.get(&channel_id) {
                                        if let Some((target_tx, _)) = channel.get(target_id) {
                                            let mut forwarded_msg = parsed.clone();
                                            forwarded_msg.user_id = Some(user_id.clone());
                                            let forwarded_text = serde_json::to_string(&forwarded_msg).unwrap();
                                            let _ = target_tx.try_send(Ok(Message::Text(forwarded_text.into())));
                                            found = true;
                                        }
                                    }
                                }
                            }

                            if !found {

                                let is_remote = {
                                    let rl = state.remote_users.lock().await;
                                    rl.get(&room_id)
                                        .and_then(|r| r.get(&channel_id))
                                        .map(|c| c.contains_key(target_id))
                                        .unwrap_or(false)
                                };
                                if is_remote {
                                    let mut forwarded_msg = parsed.clone();
                                    forwarded_msg.user_id = Some(user_id.clone());
                                    let forwarded_text = serde_json::to_string(&forwarded_msg).unwrap();
                                    cluster_broadcast(&state.cluster_tx, &ClusterMessage {
                                        msg_type: "signal".into(),
                                        room_id: room_id.clone(),
                                        channel_id: channel_id.clone(),
                                        user_id: user_id.clone(),
                                        status: None,
                                        data: None,
                                        signal_msg: Some(forwarded_text),
                                    });
                                }
                            }
                        }
                    }
                }
            } else if let Message::Close(_) = msg {
                break;
            }
        } else {
            break;
        }
    }

    let mut schedule_room_cleanup = false;
    {
        let mut rooms_lock = rooms.lock().await;

        if is_joined {
            if let Some(room) = rooms_lock.get_mut(&room_id) {
                let mut removed = false;

                if let Some(channel) = room.get_mut(&channel_id) {
                    if let Some((stored_tx, _)) = channel.get(&user_id) {
                        if stored_tx.same_channel(&tx) {
                            channel.remove(&user_id);
                            removed = true;

                            if !channel.is_empty() {
                                let notify_msg = serde_json::to_string(&SignalMessage {
                                    msg_type: "user-left".into(),
                                    user_id: Some(user_id.clone()),
                                    target: None,
                                    data: None,
                                }).unwrap();

                                for (_, (tx, _)) in channel.iter() {
                                    let _ = tx.try_send(Ok(Message::Text(notify_msg.clone().into())));
                                }
                            }
                        }
                    }
                }

                if !removed {
                    for (_, channel) in room.iter_mut() {
                        if let Some((stored_tx, _)) = channel.get(&user_id) {
                            if stored_tx.same_channel(&tx) {
                                channel.remove(&user_id);
                                removed = true;

                                if !channel.is_empty() {
                                    let notify_msg = serde_json::to_string(&SignalMessage {
                                        msg_type: "user-left".into(),
                                        user_id: Some(user_id.clone()),
                                        target: None,
                                        data: None,
                                    }).unwrap();

                                    for (_, (tx, _)) in channel.iter() {
                                        let _ = tx.try_send(Ok(Message::Text(notify_msg.clone().into())));
                                    }
                                }
                                break;
                            }
                        }
                    }
                }

                if removed && room.values().all(|c| c.is_empty()) {
                    schedule_room_cleanup = true;
                }
            }
        }
    }

    if schedule_room_cleanup {
        let next_generation = {
            let mut cleanup_lock = room_cleanup_generations.lock().await;
            let next = cleanup_lock.get(&room_id).copied().unwrap_or(0) + 1;
            cleanup_lock.insert(room_id.clone(), next);
            next
        };
        println!(
            "CLEANUP: Room '{}' became empty; scheduling deletion in {}s (generation {})",
            room_id, ROOM_EMPTY_GRACE_SECS, next_generation
        );

        let rooms_clone = rooms.clone();
        let cleanup_clone = room_cleanup_generations.clone();
        let room_id_clone = room_id.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(ROOM_EMPTY_GRACE_SECS)).await;

            let generation_still_current = cleanup_clone
                .lock()
                .await
                .get(&room_id_clone)
                .copied()
                .map(|g| g == next_generation)
                .unwrap_or(false);
            if !generation_still_current {
                return;
            }

            let removed_room = {
                let mut rooms_lock = rooms_clone.lock().await;
                let should_remove_room = rooms_lock
                    .get(&room_id_clone)
                    .map(|room| room.values().all(|c| c.is_empty()))
                    .unwrap_or(false);
                if should_remove_room {
                    rooms_lock.remove(&room_id_clone);
                    true
                } else {
                    false
                }
            };

            if removed_room {
                let mut cleanup_lock = cleanup_clone.lock().await;
                if cleanup_lock.get(&room_id_clone).copied() == Some(next_generation) {
                    cleanup_lock.remove(&room_id_clone);
                }
                println!("CLEANUP: Removed empty room '{}' after {}s empty", room_id_clone, ROOM_EMPTY_GRACE_SECS);
            }
        });
    }
    if is_joined {
        cluster_broadcast(&state.cluster_tx, &ClusterMessage {
            msg_type: "user-left".into(),
            room_id: room_id.clone(),
            channel_id: channel_id.clone(),
            user_id: user_id.clone(),
            status: None,
            data: None,
            signal_msg: None,
        });
    }
    broadcast_channel_list(&rooms, &state.remote_users, &room_id).await;
}

