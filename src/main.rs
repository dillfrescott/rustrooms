use axum::{
    extract::{
        ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade},
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
    collections::{HashMap, HashSet, VecDeque},
    sync::Arc,
};
use tokio::sync::Mutex;
use uuid::Uuid;

use tokio_tungstenite::{connect_async, tungstenite::protocol::Message as WsMessage};
use sha1::{Sha1, Digest};

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
    "background_color": "#000000",
    "theme_color": "#000000",
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
    <rect width="512" height="512" rx="128" ry="128" fill="#000000"/>
    <circle cx="256" cy="256" r="180" fill="#4f70f4" fill-opacity="0.15"/>
    <circle cx="256" cy="256" r="140" fill="#4f70f4" fill-opacity="0.3"/>
    <circle cx="256" cy="256" r="100" fill="#4f70f4"/>
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
    <meta name="theme-color" content="#000000">
    <script src="/assets/tailwind.js"></script>
    <script>
        tailwind.config = {
            future: {
                hoverOnlyWhenSupported: true,
            }
        }
    </script>
    <link href="/assets/inter.css" rel="stylesheet">
    <style>
        :root {
            --bg-primary: #000000;
            --bg-secondary: #000000;
            --bg-tertiary: #0a0a0a;
            --bg-elevated: #0f0f0f;
            --bg-elevated-strong: #141414;

            --border-subtle: rgba(255, 255, 255, 0.1);
            --border-medium: rgba(255, 255, 255, 0.16);
            --border-strong: rgba(255, 255, 255, 0.22);
            --border-accent: rgba(79, 112, 244, 0.25);

            --text-primary: #f0f0f2;
            --text-secondary: #8b8b94;
            --text-muted: #52525b;

            --accent: #4f70f4;
            --accent-hover: #6e8af6;
            --accent-glow: rgba(79, 112, 244, 0.2);
            --accent-blue: #4f70f4;
            --accent-dark-blue: #3b59f1;

            --accent-green: #22c55e;
            --accent-green-hover: #4ade80;
            --accent-red: #ef4444;
            --accent-red-hover: #f87171;
            --accent-dark-red: #dc2626;
            --accent-yellow: #eab308;

            --success: #22c55e;
            --success-glow: rgba(34, 197, 94, 0.2);
            --danger: #ef4444;
            --danger-glow: rgba(239, 68, 68, 0.2);
            --warning: #eab308;
            --warning-glow: rgba(234, 179, 8, 0.2);

            --shadow-sm: 0 0 0 rgba(0, 0, 0, 0);
            --shadow-md: 0 1px 2px rgba(0, 0, 0, 0.2), 0 1px 3px rgba(0, 0, 0, 0.12);
            --shadow-lg: 0 4px 12px rgba(0, 0, 0, 0.25), 0 2px 4px rgba(0, 0, 0, 0.15);
            --shadow-xl: 0 8px 24px rgba(0, 0, 0, 0.3), 0 4px 8px rgba(0, 0, 0, 0.2);
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
            -webkit-font-smoothing: antialiased;
            -moz-osx-font-smoothing: grayscale;
        }

        * {
            -webkit-font-smoothing: antialiased;
            -moz-osx-font-smoothing: grayscale;
        }

        img, video, canvas {
            filter: none;
        }

        ::selection {
            background: var(--accent);
            color: var(--text-primary);
        }

        ::-webkit-scrollbar { width: 8px; height: 8px; }
        ::-webkit-scrollbar-track { background: var(--bg-primary); }
        ::-webkit-scrollbar-thumb {
            background: var(--border-strong);
            border-radius: 4px;
            transition: background 0.2s ease;
        }
        ::-webkit-scrollbar-thumb:hover {
            background: var(--border-accent);
        }

        .glass-panel {
            background: var(--bg-elevated);
            border: 1px solid var(--border-subtle);
            box-shadow: var(--shadow-lg);
            border-radius: 16px;
        }

        .video-container {
            position: relative;
            background: var(--bg-secondary);
            border-radius: 12px;
            overflow: hidden;
            border: 1px solid var(--border-subtle);
            transition: all 0.35s cubic-bezier(0.4, 0, 0.2, 1);
            display: flex;
            flex-direction: column;
            width: 100%;
            height: 100%;
            box-shadow: none;
        }

        .video-container:hover {
            border-color: var(--border-medium);
            box-shadow: none;
            transform: none;
        }

        .video-container video {
            width: 100%;
            height: 100%;
            object-fit: contain;
            background: transparent;
        }

        .grid-expand {
            grid-auto-rows: minmax(150px, 1fr);
        }
        @media (min-width: 768px) {
            .grid-expand {
                grid-auto-rows: minmax(200px, 1fr);
            }
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
            filter: blur(40px) saturate(1.5);
            opacity: 0.2;
            pointer-events: none;
            -webkit-user-drag: none;
            user-drag: none;
        }

        .avatar-center {
            position: relative;
            width: 120px;
            height: 120px;
            border-radius: 12px;
            overflow: hidden;
            border: 2px solid var(--border-subtle);
            background: var(--bg-tertiary);
            transition: all 0.3s cubic-bezier(0.4, 0, 0.2, 1);
        }

        .avatar-center:hover {
            border-color: var(--border-medium);
            box-shadow: none;
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
            border-radius: 14px;
            border: 1px solid var(--border-subtle);
            cursor: pointer;
            display: flex;
            align-items: center;
            justify-content: center;
            transition: all 0.2s cubic-bezier(0.4, 0, 0.2, 1);
            background: var(--bg-elevated);
            color: var(--text-primary);
            width: 52px;
            height: 52px;
            overflow: hidden;
            position: relative;
            box-shadow: none;
        }

        @media (hover: hover) {
            .control-btn:hover {
                background: var(--bg-elevated-strong);
                border-color: var(--border-medium);
                transform: translateY(-1px);
                box-shadow: var(--shadow-md);
            }
            .control-btn.active-red:hover {
                background: var(--accent-red-hover);
                border-color: rgba(248, 113, 113, 0.3);
                box-shadow: 0 0 16px rgba(239, 68, 68, 0.2);
                transform: translateY(-1px);
            }
            .control-btn.active-green:hover {
                background: var(--accent-green-hover);
                border-color: rgba(74, 222, 128, 0.3);
                box-shadow: 0 0 16px rgba(34, 197, 94, 0.2);
                transform: translateY(-1px);
            }
            .control-btn:disabled:hover {
                background: var(--bg-elevated);
                border-color: var(--border-subtle);
                transform: none;
                box-shadow: none;
            }
            .control-btn:disabled:hover::before {
                opacity: 0;
            }
        }

        .control-btn:active {
            transform: scale(0.96) translateY(0);
            transition: transform 0.1s ease;
        }

        .control-btn.active-red:active {
            background: var(--danger);
        }

        .control-btn.active-red {
            background: var(--danger);
            border-color: rgba(239, 68, 68, 0.3);
            box-shadow: 0 0 12px rgba(239, 68, 68, 0.15);
        }

        .control-btn.active-green {
            background: var(--success);
            border-color: rgba(34, 197, 94, 0.3);
            box-shadow: 0 0 12px rgba(34, 197, 94, 0.15);
        }

        .control-btn.active-green:active {
            background: var(--success);
        }

        .control-btn:disabled {
            opacity: 0.35;
            cursor: not-allowed;
            pointer-events: none;
            -webkit-pointer-events: none;
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
            bottom: 200px;
            right: 16px;
            cursor: grab;
            touch-action: none;
            width: 160px;
            aspect-ratio: 16/9;
            border-radius: 12px;
            border: 1px solid var(--border-subtle);
            overflow: hidden;
            z-index: 75;
            transition: all 0.3s cubic-bezier(0.4, 0, 0.2, 1);
            background: var(--bg-elevated);
            box-shadow: var(--shadow-lg);
        }

        @media (hover: hover) {
            .pip-wrapper:hover {
                border-color: var(--border-medium);
                box-shadow: var(--shadow-xl);
                transform: translateY(-2px);
            }
        }

        @media (max-width: 400px) {
            .pip-wrapper {
                width: 120px;
                right: 10px;
            }
        }

        @media (max-height: 500px) {
            .pip-wrapper {
                width: 120px;
                bottom: 120px;
                right: 10px;
            }
        }

        .connection-dot {
            width: 8px;
            height: 8px;
            background-color: var(--danger);
            border-radius: 50%;
            display: inline-block;
            transition: background-color 0.3s, box-shadow 0.3s;
            box-shadow: 0 0 8px rgba(239, 68, 68, 0.3);
        }
        .connection-dot.connected {
            background-color: var(--success);
            box-shadow: 0 0 8px rgba(34, 197, 94, 0.3);
        }
        .connection-dot.connecting {
            background-color: var(--warning);
            box-shadow: 0 0 8px rgba(234, 179, 8, 0.3);
            animation: pulse 2s infinite;
        }

        @keyframes pulse {
            0%, 100% { opacity: 1; }
            50% { opacity: 0.7; }
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
            height: 14px;
        }

        .ping-bar {
            width: 3px;
            background-color: var(--border-strong);
            border-radius: 1.5px;
            transition: background-color 0.3s, height 0.3s;
        }

        .ping-bar-1 { height: 5px; }
        .ping-bar-2 { height: 9px; }
        .ping-bar-3 { height: 13px; }

        .ping-good .ping-bar { background-color: var(--success); }
        .ping-fair .ping-bar-1, .ping-fair .ping-bar-2 { background-color: var(--warning); }
        .ping-poor .ping-bar-1 { background-color: var(--danger); }

        .status-pill {
            cursor: pointer;
            user-select: none;
            -webkit-user-select: none;
        }

        .status-pill-wrapper {
            position: relative;
            display: inline-block;
        }

        .stats-window {
            position: fixed;
            width: 320px;
            max-width: calc(100vw - 32px);
            max-height: calc(100vh - 32px);
            background: var(--bg-elevated-strong);
            border: 1px solid var(--border-medium);
            border-radius: 14px;
            box-shadow: var(--shadow-xl);
            z-index: 9999;
            opacity: 0;
            visibility: hidden;
            transform: translateY(-8px) scale(0.98);
            transition: all 0.25s cubic-bezier(0.4, 0, 0.2, 1);
            overflow: hidden;
            overflow-y: auto;
        }

        .stats-window.visible {
            opacity: 1;
            visibility: visible;
            transform: translateY(0) scale(1);
        }

        .stats-header {
            display: flex;
            align-items: center;
            justify-content: space-between;
            padding: 14px 18px;
            border-bottom: 1px solid var(--border-subtle);
            background: var(--bg-elevated);
        }

        .stats-title {
            font-size: 0.8rem;
            font-weight: 600;
            color: var(--text-primary);
            display: flex;
            align-items: center;
            gap: 8px;
            letter-spacing: 0.01em;
        }

        .stats-close {
            width: 28px;
            height: 28px;
            border-radius: 8px;
            display: flex;
            align-items: center;
            justify-content: center;
            cursor: pointer;
            transition: all 0.15s ease;
            color: var(--text-muted);
        }

        @media (hover: hover) {
            .stats-close:hover {
                background: rgba(255, 255, 255, 0.08);
                color: var(--text-primary);
            }
        }

        .stats-content {
            padding: 14px 18px;
        }

        .stats-grid {
            display: grid;
            grid-template-columns: 1fr 1fr;
            gap: 12px;
        }

        .stat-item {
            display: flex;
            flex-direction: column;
            gap: 4px;
        }

        .stat-label {
            font-size: 0.65rem;
            color: var(--text-muted);
            text-transform: uppercase;
            letter-spacing: 0.06em;
            font-weight: 600;
        }

        .stat-value {
            font-size: 0.85rem;
            color: var(--text-primary);
            font-weight: 500;
            font-family: 'Inter', -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
        }

        .stat-value.good { color: var(--success); }
        .stat-value.fair { color: var(--warning); }
        .stat-value.poor { color: var(--danger); }

        .stats-section {
            margin-top: 14px;
            padding-top: 14px;
            border-top: 1px solid var(--border-subtle);
        }

        .stats-section:first-child {
            margin-top: 0;
            padding-top: 0;
            border-top: none;
        }

        .stats-section-title {
            font-size: 0.7rem;
            font-weight: 600;
            color: var(--text-secondary);
            margin-bottom: 10px;
            display: flex;
            align-items: center;
            gap: 6px;
            letter-spacing: 0.02em;
        }

        .stats-row {
            display: flex;
            align-items: center;
            justify-content: space-between;
            padding: 5px 0;
            font-size: 0.78rem;
        }

        .stats-row-label {
            color: var(--text-muted);
        }

        .stats-row-value {
            color: var(--text-primary);
            font-weight: 500;
        }

        .stats-refresh {
            font-size: 0.65rem;
            color: var(--text-muted);
            text-align: center;
            padding: 10px;
            border-top: 1px solid var(--border-subtle);
        }

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
            transition: transform 0.15s cubic-bezier(0.4, 0, 0.2, 1);
            box-shadow: 0 1px 4px rgba(0, 0, 0, 0.3);
        }
        @media (hover: hover) {
            input[type=range]::-webkit-slider-thumb:hover {
                transform: scale(1.15);
            }
        }
        input[type=range]::-webkit-slider-runnable-track {
            width: 100%;
            height: 4px;
            cursor: pointer;
            background: rgba(255, 255, 255, 0.1);
            border-radius: 2px;
        }

        .volume-controls {
            position: absolute;
            bottom: 12px;
            right: 12px;
            background: #0a0a0a;
            padding: 10px 14px;
            border-radius: 12px;
            display: flex;
            flex-direction: column;
            gap: 10px;
            opacity: 0;
            transition: all 0.3s cubic-bezier(0.4, 0, 0.2, 1);
            align-items: flex-end;
            border: 1px solid var(--border-subtle);
            box-shadow: var(--shadow-xl);
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
            border-radius: 8px;
            transition: all 0.15s cubic-bezier(0.4, 0, 0.2, 1);
        }
        @media (hover: hover) {
            .vol-row button:hover {
                background: rgba(255, 255, 255, 0.1);
                transform: scale(1.05);
            }
        }

        .speaking-glow {
            border: 3px solid var(--accent) !important;
            box-shadow: 0 0 24px rgba(79, 112, 244, 0.45), inset 0 0 24px rgba(79, 112, 244, 0.1) !important;
            transition: border 0.2s ease-in-out, box-shadow 0.2s ease-in-out;
            z-index: 50;
        }

        #localPipWrapper.speaking-glow {
            border: 3px solid var(--accent) !important;
            box-shadow: 0 0 16px rgba(79, 112, 244, 0.45) !important;
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
            transform: scale(1.03) translate3d(0, 0, 0);
            pointer-events: none;
            opacity: 0.92;
            will-change: transform;
            transition: none;
            user-select: none;
            -webkit-user-select: none;
            outline: none;
            box-shadow: var(--shadow-xl);
        }

        .video-container.is-dragging * {
            user-select: none;
            -webkit-user-select: none;
            outline: none;
        }

        .video-container.drag-placeholder {
            opacity: 0.2;
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
            transition: transform 0.3s cubic-bezier(0.2, 0, 0, 1);
            user-select: none;
            -webkit-user-select: none;
        }

        .video-container:fullscreen, .video-container:-webkit-full-screen {
            border-radius: 0;
            background: #000;
            display: flex;
            align-items: center;
            justify-content: center;
            width: 100vw;
            height: 100vh;
        }

        .video-container:fullscreen video, .video-container:-webkit-full-screen video {
            max-height: 100vh;
            max-width: 100vw;
            height: 100%;
            width: 100%;
            object-fit: contain;
        }

        .video-container:fullscreen .volume-controls, .video-container:-webkit-full-screen .volume-controls {
            bottom: 40px;
            right: 40px;
            transform: scale(1.1);
            transform-origin: bottom right;
            padding: 14px 18px;
            gap: 10px;
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
            transform: scale(1.03) translate3d(0, 0, 0);
            pointer-events: none;
            opacity: 0.92;
            will-change: transform;
            transition: none;
            user-select: none;
            -webkit-user-select: none;
            outline: none;
            box-shadow: var(--shadow-xl);
        }

        .video-container.is-dragging * {
            user-select: none;
            -webkit-user-select: none;
            outline: none;
        }

        .video-container.drag-placeholder {
            opacity: 0.15;
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
            transition: transform 0.3s cubic-bezier(0.2, 0, 0, 1);
            user-select: none;
            -webkit-user-select: none;
        }

        .video-container:fullscreen video, .video-container:-webkit-full-screen video {
            max-height: 100vh;
            max-width: 100vw;
            height: 100%;
            width: 100%;
            object-fit: contain;
        }

        .video-container:fullscreen .volume-controls, .video-container:-webkit-full-screen .volume-controls {
            bottom: 40px;
            right: 40px;
            transform: scale(1.1);
            transform-origin: bottom right;
            padding: 14px 18px;
            gap: 10px;
        }

        .mic-meter {
            width: 100%;
            height: 4px;
            background: rgba(255, 255, 255, 0.06);
            border-radius: 2px;
            overflow: hidden;
            margin-top: 10px;
        }
        .mic-bar {
            height: 100%;
            width: 0%;
            background: linear-gradient(90deg, var(--accent) 0%, var(--accent-hover) 100%);
            border-radius: 2px;
            transition: width 0.04s linear;
        }

        .taskbar {
            background: #000000;
            border-top: 1px solid var(--border-subtle);
            padding-bottom: env(safe-area-inset-bottom);
            box-shadow: none;
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

        @media (hover: hover) and (pointer: fine) {
            #btnSwitchCam {
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
            border-radius: 10px;
            box-shadow: none;
        }

        input[type="text"]:focus,
        input[type="password"]:focus,
        select:focus {
            outline: none;
            border-color: var(--accent);
            background: var(--bg-secondary);
            box-shadow: 0 0 0 3px var(--accent-glow);
        }

        select option {
            background-color: var(--bg-tertiary);
            color: var(--text-primary);
        }

        input[type="text"]::placeholder,
        input[type="password"]::placeholder {
            color: var(--text-muted);
            opacity: 0.7;
        }

        .btn-primary {
            background: var(--accent);
            transition: all 0.2s cubic-bezier(0.4, 0, 0.2, 1);
            border-radius: 10px;
            box-shadow: 0 1px 2px rgba(0, 0, 0, 0.15);
        }
        @media (hover: hover) {
            .btn-primary:hover {
                background: var(--accent-hover);
                transform: translateY(-1px);
                box-shadow: 0 4px 12px rgba(79, 112, 244, 0.25);
            }
        }

        .btn-primary:active {
            transform: translateY(0);
            transition: transform 0.1s ease;
        }

        .btn-secondary {
            background: var(--bg-elevated);
            border: 1px solid var(--border-subtle);
            transition: all 0.2s cubic-bezier(0.4, 0, 0.2, 1);
            border-radius: 10px;
            box-shadow: none;
            touch-action: manipulation;
            -webkit-tap-highlight-color: transparent;
            -webkit-touch-callout: none;
            -webkit-user-select: none;
            user-select: none;
        }
        @media (hover: hover) {
            .btn-secondary:hover {
                background: var(--bg-elevated-strong);
                border-color: var(--border-medium);
                transform: translateY(-1px);
                box-shadow: var(--shadow-md);
            }
        }
        .btn-secondary:focus {
            outline: none;
        }
        .btn-secondary:active, .btn-secondary.is-pressed {
            transform: scale(0.97) translateY(0);
            transition: transform 0.1s ease;
        }
        @media (hover: none) {
            .btn-secondary:active, .btn-secondary.is-pressed {
                background: var(--bg-elevated);
                border-color: var(--border-subtle);
                transform: scale(0.97);
                box-shadow: none;
            }
            .btn-secondary.active-red:active, .btn-secondary.active-red.is-pressed {
                background: var(--danger);
                border-color: var(--danger);
                transform: scale(0.97);
                box-shadow: none;
            }
        }

        .btn-secondary.active-red {
            background: var(--danger);
            border-color: rgba(239, 68, 68, 0.3);
            box-shadow: 0 0 12px rgba(239, 68, 68, 0.15);
        }

        .btn-icon-test:active, .btn-icon-test.is-pressed {
            transform: scale(0.94);
            transition: transform 0.1s ease;
        }

        @media (hover: hover) {
            .btn-secondary.active-red:hover {
                background: var(--accent-red-hover);
                border-color: rgba(248, 113, 113, 0.3);
                box-shadow: 0 0 16px rgba(239, 68, 68, 0.2);
                transform: translateY(-1px);
            }
        }

        .status-pill {
            background: var(--bg-elevated);
            border: 1px solid var(--border-subtle);
            border-radius: 999px !important;
            transition: all 0.2s cubic-bezier(0.4, 0, 0.2, 1);
            box-shadow: none;
        }

        @media (hover: hover) {
            .status-pill:hover {
                background: var(--bg-elevated-strong);
                border-color: var(--border-medium);
                box-shadow: none;
            }
        }

        .label-text {
            color: var(--text-secondary);
            font-size: 0.7rem;
            font-weight: 600;
            letter-spacing: 0.04em;
            text-transform: uppercase;
        }

        .empty-state-icon {
            color: var(--text-muted);
            opacity: 0.4;
        }

        .fadeIn {
            animation: fadeIn 0.4s cubic-bezier(0.4, 0, 0.2, 1);
        }

        @keyframes fadeOut {
            0% { opacity: 1; visibility: visible; }
            100% { opacity: 0; visibility: hidden; }
        }

        @keyframes fadeIn {
            0% { opacity: 0; transform: translateY(12px); }
            100% { opacity: 1; transform: translateY(0); }
        }

        #particleCanvas, #particleCanvasConfig, #particleCanvasInvite {
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
            left: -340px;
            top: 0;
            bottom: 0;
            width: 340px;
            z-index: 100;
            transition: transform 0.4s cubic-bezier(0.4, 0, 0.2, 1);
            background: var(--bg-secondary);
            border-right: 1px solid var(--border-subtle);
            display: flex;
            flex-direction: column;
            box-shadow: none;
        }

        #roomSidebar.open {
            transform: translateX(340px);
        }

        @media (min-width: 768px) {
            body.sidebar-open #appLayout {
                margin-left: 340px;
                width: calc(100% - 340px);
                transition: margin-left 0.4s cubic-bezier(0.4, 0, 0.2, 1), width 0.4s cubic-bezier(0.4, 0, 0.2, 1);
            }
        }

        .app-topbar,
        .sidebar-header {
            box-sizing: border-box;
            height: 52px;
        }

        @media (min-width: 640px) {
            .app-topbar,
            .sidebar-header {
                height: 60px;
            }
        }

        @media (min-width: 768px) {
            .app-topbar,
            .sidebar-header {
                height: 76px;
            }
        }

        .sidebar-header {
            padding: 12px 20px;
            border-bottom: 1px solid var(--border-subtle);
            display: flex;
            align-items: center;
            justify-content: space-between;
            background: var(--bg-secondary);
        }

        .sidebar-header h2 {
            margin: 0;
            line-height: 1.2;
        }

        .sidebar-header button {
            margin: 0;
            padding: 0;
            width: 28px;
            height: 28px;
            display: flex;
            align-items: center;
            justify-content: center;
            flex-shrink: 0;
            background: transparent;
            border: 0;
            border-radius: 8px;
            transition: all 0.15s ease;
        }

        @media (hover: hover) {
            .sidebar-header button:hover {
                background: rgba(255, 255, 255, 0.08);
            }
        }

        @media (min-width: 640px) {
            .sidebar-header {
                padding-top: 16px;
                padding-bottom: 16px;
            }
        }

        @media (min-width: 768px) {
            .sidebar-header {
                padding-top: 20px;
                padding-bottom: 20px;
            }
        }

        .sidebar-content {
            flex: 1;
            overflow-y: auto;
            padding: 16px 14px;
        }

        .room-item {
            background: var(--bg-tertiary);
            border: 1px solid var(--border-subtle);
            border-radius: 12px;
            padding: 14px;
            margin-bottom: 8px;
            transition: all 0.2s cubic-bezier(0.4, 0, 0.2, 1);
            cursor: pointer;
            box-shadow: none;
        }

        @media (hover: hover) {
            .room-item:hover {
                border-color: var(--border-medium);
                background: var(--bg-elevated);
                transform: translateY(-1px);
                box-shadow: var(--shadow-md);
            }
        }

        .room-item.active {
            border-color: var(--accent);
            background: rgba(79, 112, 244, 0.06);
            box-shadow: 0 0 0 1px var(--border-accent);
        }

        .room-name {
            font-weight: 600;
            font-size: 0.9rem;
            color: var(--text-primary);
            margin-bottom: 8px;
            display: flex;
            align-items: center;
            justify-content: space-between;
        }

        .user-count {
            font-size: 0.65rem;
            color: var(--text-secondary);
            background: var(--bg-primary);
            padding: 2px 8px;
            border-radius: 99px;
            border: 1px solid var(--border-subtle);
            font-weight: 500;
        }

        .room-users {
            display: flex;
            flex-wrap: wrap;
            gap: 6px;
        }

        .mini-avatar {
            width: 26px;
            height: 26px;
            border-radius: 8px;
            background: var(--bg-primary);
            border: 1px solid var(--border-subtle);
            overflow: hidden;
            display: flex;
            align-items: center;
            justify-content: center;
            transition: all 0.2s ease;
        }

        @media (hover: hover) {
            .mini-avatar:hover {
                border-color: var(--border-medium);
                transform: scale(1.05);
            }
        }

        .mini-avatar img {
            width: 100%;
            height: 100%;
            object-fit: cover;
        }

        .mini-avatar-placeholder {
            font-size: 11px;
            font-weight: 600;
            color: var(--text-muted);
        }

        .mini-avatar.speaking-glow {
            border: 3px solid var(--accent) !important;
            box-shadow: 0 0 10px rgba(79, 112, 244, 0.5) !important;
            transition: border 0.2s ease-in-out, box-shadow 0.2s ease-in-out;
        }

        .sidebar-overlay {
            position: fixed;
            inset: 0;
            background: rgba(0, 0, 0, 0.5);
            z-index: 90;
            opacity: 0;
            pointer-events: none;
            transition: opacity 0.35s ease;
            will-change: opacity;
        }

        .sidebar-overlay.open {
            opacity: 1;
            pointer-events: auto;
            backdrop-filter: blur(8px);
            -webkit-backdrop-filter: blur(8px);
            will-change: auto;
        }

        @media (min-width: 768px) {
            body.sidebar-open .sidebar-overlay {
                display: none !important;
            }
        }

        .modal-overlay {
            position: fixed;
            inset: 0;
            background: rgba(0, 0, 0, 0.7);
            backdrop-filter: blur(8px);
            -webkit-backdrop-filter: blur(8px);
            z-index: 300;
            display: flex;
            align-items: center;
            justify-content: center;
            opacity: 0;
            pointer-events: none;
            transition: all 0.3s cubic-bezier(0.4, 0, 0.2, 1);
        }

        .modal-overlay.open {
            opacity: 1;
            pointer-events: auto;
        }

        .modal-content {
            background: var(--bg-elevated);
            border: 1px solid var(--border-medium);
            border-radius: 16px;
            width: 90%;
            max-width: 420px;
            padding: 36px 28px;
            transform: scale(0.95) translateY(12px);
            transition: all 0.35s cubic-bezier(0.16, 1, 0.3, 1);
            box-shadow: var(--shadow-xl);
        }

        .modal-overlay.open .modal-content {
            transform: scale(1) translateY(0);
        }

        .room-user-row {
            display: flex;
            align-items: center;
            gap: 10px;
            padding: 8px 10px;
            border-radius: 10px;
            transition: all 0.2s cubic-bezier(0.4, 0, 0.2, 1);
        }

        @media (hover: hover) {
            .room-user-row:hover {
                background: rgba(255, 255, 255, 0.04);
            }
        }

        .room-user-name {
            font-size: 0.82rem;
            color: var(--text-secondary);
            font-weight: 500;
            overflow: hidden;
            text-overflow: ellipsis;
            white-space: nowrap;
        }

        .status-indicators {
            display: flex;
            gap: 6px;
            margin-left: auto;
            align-items: center;
        }

        .status-icon {
            color: var(--text-muted);
            opacity: 0.5;
            transition: all 0.2s ease;
        }

        .status-icon.active {
            color: #ef4444;
            opacity: 1;
        }

        .user-volume-menu {
            position: fixed;
            z-index: 200;
            min-width: 220px;
            max-width: 260px;
            background: var(--bg-elevated-strong);
            border: 1px solid var(--border-medium);
            border-radius: 14px;
            padding: 14px 16px;
            box-shadow: var(--shadow-xl);
            opacity: 0;
            transform: scale(0.95) translateY(-4px);
            pointer-events: none;
            transition: opacity 0.2s cubic-bezier(0.4, 0, 0.2, 1), transform 0.2s cubic-bezier(0.4, 0, 0.2, 1);
        }

        .user-volume-menu.open {
            opacity: 1;
            transform: scale(1) translateY(0);
            pointer-events: auto;
        }

        .user-volume-menu .uvm-header {
            display: flex;
            align-items: center;
            gap: 10px;
            margin-bottom: 12px;
            padding-bottom: 10px;
            border-bottom: 1px solid var(--border-subtle);
        }

        .user-volume-menu .uvm-name {
            font-size: 0.82rem;
            font-weight: 600;
            color: var(--text-primary);
            white-space: nowrap;
            overflow: hidden;
            text-overflow: ellipsis;
            flex: 1;
        }

        .user-volume-menu .uvm-close {
            background: none;
            border: none;
            color: var(--text-muted);
            cursor: pointer;
            padding: 4px;
            border-radius: 6px;
            transition: all 0.15s ease;
            display: flex;
            align-items: center;
            justify-content: center;
        }

        @media (hover: hover) {
            .user-volume-menu .uvm-close:hover {
                color: var(--text-primary);
                background: rgba(255, 255, 255, 0.08);
            }
        }

        .user-volume-menu .uvm-section {
            display: flex;
            flex-direction: column;
            gap: 6px;
        }

        .user-volume-menu .uvm-section + .uvm-section {
            margin-top: 10px;
            padding-top: 10px;
            border-top: 1px solid var(--border-subtle);
        }

        .user-volume-menu .uvm-label {
            font-size: 0.65rem;
            font-weight: 600;
            color: var(--text-muted);
            text-transform: uppercase;
            letter-spacing: 0.06em;
        }

        .user-volume-menu .uvm-slider-row {
            display: flex;
            align-items: center;
            gap: 10px;
        }

        .user-volume-menu .uvm-slider-row button {
            background: none;
            border: none;
            color: var(--text-primary);
            cursor: pointer;
            padding: 4px;
            border-radius: 6px;
            transition: all 0.15s ease;
            display: flex;
            align-items: center;
            justify-content: center;
            flex-shrink: 0;
        }

        @media (hover: hover) {
            .user-volume-menu .uvm-slider-row button:hover {
                background: rgba(255, 255, 255, 0.1);
                transform: scale(1.1);
            }
        }

        .user-volume-menu .uvm-slider-row button.muted {
            color: var(--accent-red);
        }

        .user-volume-menu .uvm-slider-row input[type=range] {
            flex: 1;
            min-width: 0;
        }

        .user-volume-menu .uvm-vol-pct {
            font-size: 0.65rem;
            color: var(--text-muted);
            min-width: 30px;
            text-align: right;
            font-variant-numeric: tabular-nums;
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
        .video-container:fullscreen .name-tag,
        .video-container:-webkit-full-screen .volume-controls,
        .video-container:-webkit-full-screen .name-tag {
            transition: opacity 0.2s ease-in;
        }

        @keyframes otg-pulse {
            0% {
                transform: scale(1.02);
                box-shadow: 0 0 0 0 rgba(59, 130, 246, 0.5);
            }
            70% {
                transform: scale(1.08);
                box-shadow: 0 0 0 15px rgba(59, 130, 246, 0);
            }
            100% {
                transform: scale(1.02);
                box-shadow: 0 0 0 0 rgba(59, 130, 246, 0);
            }
        }
        .otg-speaking-pulse {
            animation: otg-pulse 1.8s infinite cubic-bezier(0.4, 0, 0.6, 1);
            border-color: #3b82f6 !important;
        }

        @keyframes otgRotateDevice {
            0% {
                transform: rotate(0deg);
            }
            30%, 100% {
                transform: rotate(-90deg);
            }
        }
        .otg-rotate-anim {
            animation: otgRotateDevice 2.5s ease-in-out infinite;
            transform-origin: center;
        }

        @media (orientation: landscape) and (max-width: 1024px) {
            #otgOrientationWarning {
                display: flex !important;
            }
        }

        /* Height-based responsive scaling for On-the-go Mode to fit any phone screen without scrolling */
        @media (max-height: 760px) {
            #onTheGoOverlay {
                padding: 1rem !important;
                gap-y: 1rem !important;
            }
            #onTheGoOverlay .mt-4 {
                margin-top: 0.5rem !important;
            }
            #onTheGoOverlay .pt-4 {
                padding-top: 0.5rem !important;
            }
            #onTheGoOverlay .pt-6 {
                padding-top: 0.5rem !important;
            }
            #onTheGoOverlay .space-y-4 > :not([hidden]) ~ :not([hidden]) {
                --tw-space-y-reverse: 0 !important;
                margin-top: 0.75rem !important;
                margin-bottom: 0px !important;
            }
            #onTheGoOverlay .space-y-3 > :not([hidden]) ~ :not([hidden]) {
                --tw-space-y-reverse: 0 !important;
                margin-top: 0.5rem !important;
                margin-bottom: 0px !important;
            }
            #onTheGoAvatarWrapper {
                width: 6rem !important;
                height: 6rem !important;
            }
            #onTheGoAvatarPlaceholder {
                font-size: 3rem !important;
            }
            #onTheGoSpeakingName {
                font-size: 1.125rem !important;
            }
            #onTheGoOverlay button {
                padding-top: 0.875rem !important;
                padding-bottom: 0.875rem !important;
                border-radius: 1rem !important;
            }
            #onTheGoOverlay button svg {
                width: 1.5rem !important;
                height: 1.5rem !important;
            }
            #onTheGoOverlay button span {
                font-size: 0.875rem !important;
            }
        }

        @media (max-height: 640px) {
            #onTheGoOverlay {
                padding: 0.75rem !important;
                gap-y: 0.75rem !important;
            }
            #onTheGoOverlay .mt-4, #onTheGoOverlay .mt-8 {
                margin-top: 0.25rem !important;
            }
            #onTheGoOverlay .pt-4, #onTheGoOverlay .pt-6 {
                padding-top: 0.25rem !important;
            }
            #onTheGoOverlay .space-y-4 > :not([hidden]) ~ :not([hidden]) {
                --tw-space-y-reverse: 0 !important;
                margin-top: 0.5rem !important;
                margin-bottom: 0px !important;
            }
            #onTheGoOverlay .space-y-3 > :not([hidden]) ~ :not([hidden]) {
                --tw-space-y-reverse: 0 !important;
                margin-top: 0.25rem !important;
                margin-bottom: 0px !important;
            }
            #onTheGoAvatarWrapper {
                width: 4.5rem !important;
                height: 4.5rem !important;
            }
            #onTheGoAvatarPlaceholder {
                font-size: 2.25rem !important;
            }
            #onTheGoSpeakingName {
                font-size: 1rem !important;
            }
            #onTheGoOverlay button {
                padding-top: 0.625rem !important;
                padding-bottom: 0.625rem !important;
                border-radius: 0.75rem !important;
            }
            #onTheGoOverlay button svg {
                width: 1.25rem !important;
                height: 1.25rem !important;
            }
            #onTheGoOverlay button span {
                font-size: 0.8rem !important;
            }
        }

        @media (max-height: 540px) {
            #onTheGoOverlay {
                padding: 0.5rem !important;
                gap-y: 0.5rem !important;
            }
            #onTheGoOverlay .mt-4, #onTheGoOverlay .mt-8 {
                margin-top: 0px !important;
            }
            #onTheGoOverlay .pt-4, #onTheGoOverlay .pt-6 {
                padding-top: 0px !important;
            }
            #onTheGoOverlay .space-y-4 > :not([hidden]) ~ :not([hidden]) {
                --tw-space-y-reverse: 0 !important;
                margin-top: 0.25rem !important;
                margin-bottom: 0px !important;
            }
            #onTheGoOverlay .space-y-3 > :not([hidden]) ~ :not([hidden]) {
                --tw-space-y-reverse: 0 !important;
                margin-top: 0.125rem !important;
                margin-bottom: 0px !important;
            }
            #onTheGoAvatarWrapper {
                width: 3.5rem !important;
                height: 3.5rem !important;
            }
            #onTheGoAvatarPlaceholder {
                font-size: 1.75rem !important;
            }
            #onTheGoSpeakingName {
                font-size: 0.875rem !important;
            }
            #onTheGoOverlay button {
                padding-top: 0.5rem !important;
                padding-bottom: 0.5rem !important;
                border-radius: 0.5rem !important;
            }
            #onTheGoOverlay button svg {
                width: 1.125rem !important;
                height: 1.125rem !important;
            }
            #onTheGoOverlay button span {
                font-size: 0.75rem !important;
            }
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

    <div id="userVolumeMenu" class="user-volume-menu"></div>

    <div id="nameModal" class="modal-overlay">
        <div class="modal-content text-center space-y-5">
            <h3 id="modalTitle" class="text-xl font-bold text-white">Name Channel</h3>
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
        <div class="modal-content text-center space-y-5">
            <h3 id="passwordModalTitle" class="text-xl font-bold text-white break-words">Password Required</h3>
            <p id="passwordModalMessage" class="text-zinc-300 text-sm break-words"></p>
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
        <div class="modal-content text-center space-y-5">
            <h3 id="alertTitle" class="text-xl font-bold text-white break-words">Alert</h3>
            <p id="alertMessage" class="text-zinc-300 text-sm break-words"></p>
            <button onclick="closeCustomAlert()" class="btn-primary w-full py-3 text-white rounded-xl font-medium transition-all">OK</button>
        </div>
    </div>

    <div id="confirmModal" class="modal-overlay">
        <div class="modal-content text-center space-y-5">
            <h3 id="confirmTitle" class="text-xl font-bold text-white break-words">Confirm</h3>
            <p id="confirmMessage" class="text-zinc-300 text-sm break-words"></p>
            <div class="flex gap-3">
                <button onclick="closeCustomConfirm()" class="btn-secondary flex-1 py-3 text-white rounded-xl font-medium transition-all">Cancel</button>
                <button id="confirmSubmit" class="btn-primary flex-1 py-3 text-white rounded-xl font-medium transition-all">Confirm</button>
            </div>
        </div>
    </div>

    <div id="kickModal" class="modal-overlay">
        <div class="modal-content text-center space-y-5">
            <h3 id="kickTitle" class="text-xl font-bold text-white break-words">Kick User</h3>
            <p id="kickMessage" class="text-zinc-300 text-sm break-words"></p>
            <div class="flex gap-3">
                <button onclick="closeKickModal()" class="btn-secondary flex-1 py-3 text-white rounded-xl font-medium transition-all">Cancel</button>
                <button id="kickSubmit" class="btn-primary flex-1 py-3 text-white rounded-xl font-medium transition-all" style="background: var(--danger);">Kick</button>
            </div>
        </div>
    </div>

    <div id="welcomeOverlay" class="fixed inset-0 z-[70] flex flex-col items-center justify-center p-4" style="display: none; background: var(--bg-primary);">
        <canvas id="particleCanvas"></canvas>
        <div class="text-center space-y-8 max-w-md w-full relative z-10">
            <div class="space-y-4" id="welcomeTitleContainer">
                <h1 class="text-5xl md:text-7xl font-bold tracking-tight" style="color: #ffffff; font-weight: 800; letter-spacing: -0.04em;">Rust Rooms</h1>
                <p style="color: var(--text-secondary);" class="text-base md:text-lg font-normal opacity-70">Simple, secure, and fast video conferencing.</p>
            </div>

            <div id="startActionContainer" class="relative min-h-[72px] flex justify-center items-center">
                 <button id="btnStartRoom" onclick="createRoom()" class="btn-primary absolute w-full md:w-auto px-12 py-4 text-white rounded-2xl font-semibold text-lg transition-all">
                    Start Room
                </button>

                <div id="passwordInputContainer" class="absolute w-full max-w-xs transition-all duration-300 transform translate-y-4 opacity-0 pointer-events-none flex gap-2">
                     <input type="password" id="roomPasswordInput" placeholder="Password required" class="flex-1 rounded-xl px-4 py-3 text-white bg-[var(--bg-tertiary)] border border-[var(--border-subtle)] focus:border-[var(--accent)] outline-none transition-all" onkeypress="if(event.key==='Enter') submitPassword()">
                     <button onclick="submitPassword()" class="btn-primary px-5 py-3 text-white rounded-xl font-medium transition-all flex items-center justify-center">
                        <svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="20 6 9 17 4 12"/></svg>
                     </button>
                </div>
            </div>
        </div>
    </div>

    <div id="inviteWelcomeOverlay" class="fixed inset-0 z-[80] flex flex-col items-center justify-center p-4 transition-opacity duration-300 hidden opacity-0" style="background: var(--bg-primary);">
        <canvas id="particleCanvasInvite"></canvas>
        <div class="glass-panel p-8 md:p-12 rounded-3xl max-w-lg w-full relative z-10 text-center space-y-8 overflow-hidden">
            <div class="space-y-3">
                <div class="inline-flex items-center gap-2 px-3 py-1 rounded-full bg-blue-500/10 border border-blue-500/20 text-blue-400 text-xs font-semibold uppercase tracking-wider mb-2">
                    <span class="relative flex h-2 w-2">
                      <span class="animate-ping absolute inline-flex h-full w-full rounded-full bg-blue-400 opacity-75"></span>
                      <span class="relative inline-flex rounded-full h-2 w-2 bg-blue-500"></span>
                    </span>
                    Live Call
                </div>
                <h1 id="inviteChannelName" class="text-3xl md:text-4xl font-bold tracking-tight text-white break-words"># General</h1>
                <p id="inviteCallDuration" class="text-zinc-400 text-sm font-medium">Running for 00:00:00</p>
            </div>

            <div class="space-y-4">
                <h3 class="text-xs font-bold text-zinc-500 uppercase tracking-widest text-left">Currently in call</h3>
                <div id="inviteUserList" class="flex flex-wrap justify-center gap-3 max-h-[200px] overflow-y-auto pr-2 custom-scrollbar">
                    <!-- Users will be injected here -->
                </div>
            </div>

            <div class="pt-4">
                <button onclick="proceedToSetup()" class="btn-primary w-full py-4 text-white rounded-2xl font-bold text-lg transition-all shadow-lg hover:shadow-blue-500/20 group">
                    Join Conversation
                    <svg class="inline-block ml-2 w-5 h-5 transition-transform group-hover:translate-x-1" xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><line x1="5" y1="12" x2="19" y2="12"></line><polyline points="12 5 19 12 12 19"></polyline></svg>
                </button>
            </div>
        </div>
    </div>

    <div id="configOverlay" class="fixed inset-0 z-[60] flex flex-col items-center justify-center p-4 transition-opacity duration-300 hidden opacity-0" style="background: var(--bg-primary);">
        <canvas id="particleCanvasConfig" class="absolute inset-0 pointer-events-none" style="z-index: 1;"></canvas>
        <div id="configPanel" class="glass-panel p-8 md:p-10 rounded-2xl max-w-5xl w-full max-h-[95vh] overflow-y-auto relative z-10">
            <div class="text-center space-y-2 mb-8">
                <h1 class="text-3xl md:text-4xl font-bold tracking-tight" style="color: var(--text-primary);">Setup</h1>
                <p style="color: var(--text-secondary);" class="text-sm font-normal opacity-80">Configure your camera and microphone.</p>
            </div>

            <div class="flex flex-col lg:flex-row gap-6 lg:gap-8">

                <div class="lg:w-1/2 flex flex-col gap-4">
                    <div class="relative aspect-video rounded-lg overflow-hidden flex-shrink-0 bg-[var(--bg-secondary)] border border-[var(--border-subtle)] shadow-lg">
                        <video id="previewVideo" autoplay playsinline muted class="w-full h-full object-contain"></video>
                        <div class="absolute inset-0 flex items-center justify-center pointer-events-none" id="previewPlaceholder" style="color: var(--text-muted);">
                            <span>Camera Off</span>
                        </div>
                        <div class="absolute bottom-4 left-4 px-3 py-1.5 rounded-lg text-xs font-medium bg-black/60 border border-[var(--border-subtle)]" style="color: var(--text-primary);">
                            Preview
                        </div>
                    </div>

                    <div class="flex gap-3">
                        <button onclick="togglePreviewMic()" id="btnPreviewMic" disabled class="btn-secondary flex-1 py-3 text-white rounded-lg font-medium transition-all flex items-center justify-center gap-2 opacity-50 cursor-not-allowed">
                            Mute
                        </button>
                        <button onclick="togglePreviewCam()" id="btnPreviewCam" disabled class="btn-secondary flex-1 py-3 text-white rounded-lg font-medium transition-all flex items-center justify-center gap-2 opacity-50 cursor-not-allowed">
                            Stop Cam
                        </button>
                    </div>
                </div>

                <div class="lg:w-1/2 space-y-4">
                    <div class="flex flex-col sm:flex-row gap-4">
                        <div class="flex-shrink-0 flex justify-center sm:justify-start">
                            <div class="text-center">
                                <label class="label-text block mb-2">Avatar</label>
                                <div onclick="document.getElementById('avatarInput').click()" class="w-20 h-20 rounded-lg cursor-pointer overflow-hidden flex items-center justify-center transition-all group relative mx-auto" style="background: var(--bg-secondary); border: 2px solid var(--border-subtle);">
                                    <img id="avatarPreview" src="" class="hidden w-full h-full object-cover" draggable="false">
                                    <span id="avatarPlaceholder" class="text-3xl" style="color: var(--text-muted);">👤</span>
                                    <div class="absolute inset-0 flex items-center justify-center opacity-0 group-hover:opacity-100 transition-opacity text-xs font-semibold" style="background: rgba(0, 0, 0, 0.7); color: var(--text-primary);">Edit</div>
                                </div>
                                <button id="btnRemoveSetupAvatar" onclick="removeSetupAvatar()" class="hidden mt-1 text-xs font-medium px-2 py-0.5 rounded-lg transition-all" style="color: var(--text-muted); background: var(--bg-tertiary); border: 1px solid var(--border-subtle);" onmouseover="this.style.color='#ef4444'" onmouseout="this.style.color='var(--text-muted)'">Remove</button>
                                <div class="mt-1 text-center" style="font-size: 0.6rem; color: var(--text-muted); opacity: 0.7;">Images & GIFs</div>
                                <input type="file" id="avatarInput" hidden accept="image/*" onchange="handleAvatarUpload(this)">
                            </div>
                        </div>

                        <div class="flex-1">
                            <label class="label-text block mb-2">Nickname</label>
                            <input type="text" id="nicknameInput" placeholder="Enter your name" class="w-full rounded-lg px-4 py-2.5 text-white transition-all" style="font-size: 0.875rem;" maxlength="32">
                        </div>
                    </div>

                    <div class="grid grid-cols-1 gap-3">
                        <div>
                            <label class="label-text block mb-2">Microphone</label>
                            <select id="audioSource" onchange="startPreview()" class="w-full rounded-lg px-3 py-2.5 text-sm text-white transition-all">
                                <option value="">Default</option>
                            </select>
                            <div class="mic-meter"><div id="setupMicBar" class="mic-bar"></div></div>
                        </div>
                        <div>
                            <label class="label-text block mb-2">Speaker</label>
                            <div class="flex gap-2">
                                <select id="audioOutputSource" onchange="changeAudioOutput(this.value)" class="flex-1 min-w-0 rounded-lg px-3 py-2.5 text-sm text-white transition-all">
                                    <option value="default">Default</option>
                                </select>
                                <button onclick="testSpeaker('audioOutputSource')" class="btn-icon-test p-2.5 rounded-lg transition-all" style="background: var(--bg-tertiary); color: var(--text-primary); border: 1px solid var(--border-subtle);" title="Test Speaker">
                                    <svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5"></polygon><path d="M19.07 4.93a10 10 0 0 1 0 14.14M15.54 8.46a5 5 0 0 1 0 7.07"></path></svg>
                                </button>
                            </div>
                        </div>
                        <div>
                            <label class="label-text block mb-2">Camera</label>
                            <select id="videoSource" onchange="startPreview()" class="w-full rounded-lg px-3 py-2.5 text-sm text-white transition-all">
                                <option value="">Default</option>
                            </select>
                        </div>
                    </div>

                    <!-- Connection & Quality Settings -->
                    <div class="border-t border-[var(--border-subtle)] pt-4 mt-4 space-y-3 text-left">
                        <div class="flex items-center justify-between p-3 rounded-lg bg-[var(--bg-secondary)] border border-[var(--border-subtle)]">
                            <div class="flex flex-col">
                                <span class="text-sm font-semibold text-white">Low Bandwidth Mode</span>
                                <span class="text-xs text-zinc-400">Reduces audio & video quality to save data</span>
                            </div>
                            <label class="relative inline-flex items-center cursor-pointer">
                                <input type="checkbox" id="setupLowBandwidth" onchange="handleLowBandwidthChange(this.checked)" class="sr-only peer">
                                <div class="w-11 h-6 bg-zinc-700 peer-focus:outline-none rounded-full peer peer-checked:after:translate-x-full peer-checked:after:border-white after:content-[''] after:absolute after:top-[2px] after:left-[2px] after:bg-white after:border-zinc-300 after:border after:rounded-full after:h-5 after:w-5 after:transition-all peer-checked:bg-blue-600"></div>
                            </label>
                        </div>
                        <div id="setupOnTheGoRow" class="hidden flex items-center justify-between p-3 rounded-lg bg-[var(--bg-secondary)] border border-[var(--border-subtle)]">
                            <div class="flex flex-col">
                                <span class="text-sm font-semibold text-white">On the Go Mode</span>
                                <span class="text-xs text-zinc-400">Simplify UI with big buttons and hide video</span>
                            </div>
                            <label class="relative inline-flex items-center cursor-pointer">
                                <input type="checkbox" id="setupOnTheGo" onchange="handleOnTheGoChange(this.checked)" class="sr-only peer">
                                <div class="w-11 h-6 bg-zinc-700 peer-focus:outline-none rounded-full peer peer-checked:after:translate-x-full peer-checked:after:border-white after:content-[''] after:absolute after:top-[2px] after:left-[2px] after:bg-white after:border-zinc-300 after:border after:rounded-full after:h-5 after:w-5 after:transition-all peer-checked:bg-blue-600"></div>
                            </label>
                        </div>
                    </div>

                    <button id="btnJoin" onclick="joinRoom()" disabled class="btn-primary w-full py-3.5 text-white rounded-lg font-semibold transition-all disabled:opacity-50 disabled:cursor-not-allowed">
                        Loading...
                    </button>
                </div>
            </div>
        </div>
    </div>

    <div id="settingsOverlay" class="fixed inset-0 z-[200] flex items-center justify-center p-4 hidden" style="background: var(--bg-primary);" onclick="if(event.target === this) closeSettings()">
        <div class="glass-panel p-8 md:p-10 rounded-2xl max-w-5xl w-full max-h-[95vh] overflow-y-auto relative z-10">
             <button onclick="closeSettings()" class="absolute top-6 right-6 transition-all p-2 rounded-lg hover:bg-white/10" style="color: var(--text-muted);" title="Close">
                <svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="18" y1="6" x2="6" y2="18"></line><line x1="6" y1="6" x2="18" y2="18"></line></svg>
            </button>

            <div class="text-center space-y-2 mb-8">
                <h2 class="text-3xl md:text-4xl font-bold tracking-tight" style="color: var(--text-primary);">Settings</h2>
                <p style="color: var(--text-secondary);" class="text-sm font-normal opacity-70">Update your profile and devices.</p>
            </div>

            <div class="flex flex-col lg:flex-row gap-6 lg:gap-8">

                <div class="lg:w-1/2 space-y-4">
                    <div class="flex flex-col items-center gap-5 p-6 rounded-lg bg-[var(--bg-secondary)] border border-[var(--border-subtle)] shadow-md">
                        <label class="label-text">Avatar</label>
                        <div class="flex flex-col items-center gap-4">
                            <div onclick="document.getElementById('settingsAvatarInput').click()" class="w-32 h-32 rounded-xl cursor-pointer overflow-hidden flex items-center justify-center transition-all relative bg-[var(--bg-tertiary)] border-2 border-[var(--border-subtle)] hover:border-[var(--accent)] group shadow-lg">
                                <img id="settingsAvatarPreview" src="" class="hidden w-full h-full object-cover" draggable="false">
                                <span id="settingsAvatarPlaceholder" class="text-6xl" style="color: var(--text-muted);">👤</span>
                                <div class="absolute inset-0 flex items-center justify-center opacity-0 group-hover:opacity-100 transition-opacity text-sm font-semibold bg-black/80" style="color: var(--text-primary);">Change</div>
                            </div>
                            <button id="btnRemoveSettingsAvatar" onclick="removeSettingsAvatar()" class="hidden text-xs font-medium px-3 py-1.5 rounded-lg transition-all bg-[var(--bg-primary)] border border-[var(--border-subtle)] hover:border-[var(--danger)]" style="color: var(--text-muted);" onmouseover="this.style.color='#ef4444'" onmouseout="this.style.color='var(--text-muted)'">Remove Avatar</button>
                            <div style="font-size: 0.65rem; color: var(--text-muted); opacity: 0.7;">Images & GIFs</div>
                            <input type="file" id="settingsAvatarInput" hidden accept="image/*" onchange="handleSettingsAvatarUpload(this)">
                        </div>
                    </div>
                    <div>
                        <label class="label-text block mb-2">Nickname</label>
                        <input type="text" id="settingsNicknameInput" placeholder="Enter your name" class="w-full rounded-lg px-4 py-3 text-white transition-all" style="font-size: 0.875rem;" maxlength="32" oninput="handleSettingsNicknameInput()">
                    </div>
                </div>

                <div class="lg:w-1/2 space-y-4">
                    <div class="grid grid-cols-1 gap-4">
                         <div>
                            <label class="label-text block mb-2">Microphone</label>
                            <select id="settingsAudioSource" onchange="handleSettingsMicChange(this.value)" class="w-full rounded-lg px-3 py-2.5 text-sm text-white transition-all">
                            </select>
                            <div class="mic-meter"><div id="settingsMicBar" class="mic-bar"></div></div>
                        </div>
                         <div>
                            <label class="label-text block mb-2">Speaker</label>
                            <div class="flex gap-2">
                                <select id="settingsAudioOutputSource" onchange="changeAudioOutput(this.value)" class="flex-1 min-w-0 rounded-lg px-3 py-2.5 text-sm text-white transition-all">
                                </select>
                                <button onclick="testSpeaker('settingsAudioOutputSource')" class="btn-icon-test p-2.5 rounded-lg transition-all" style="background: var(--bg-tertiary); color: var(--text-primary); border: 1px solid var(--border-subtle);" title="Test Speaker">
                                    <svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5"></polygon><path d="M19.07 4.93a10 10 0 0 1 0 14.14M15.54 8.46a5 5 0 0 1 0 7.07"></path></svg>
                                </button>
                            </div>
                        </div>
                        <div>
                            <label class="label-text block mb-2">Camera</label>
                            <select id="settingsVideoSource" onchange="handleSettingsCamChange(this.value)" class="w-full rounded-lg px-3 py-2.5 text-sm text-white transition-all">
                            </select>
                        </div>
                        <div class="border-t border-[var(--border-subtle)] pt-4 mt-4 space-y-3 text-left">
                            <div class="flex items-center justify-between p-3 rounded-lg bg-[var(--bg-secondary)] border border-[var(--border-subtle)]">
                                <div class="flex flex-col">
                                    <span class="text-sm font-semibold text-white">Low Bandwidth Mode</span>
                                    <span class="text-xs text-zinc-400">Reduces audio & video quality to save data</span>
                                </div>
                                <label class="relative inline-flex items-center cursor-pointer">
                                    <input type="checkbox" id="settingsLowBandwidth" onchange="handleLowBandwidthChange(this.checked)" class="sr-only peer">
                                    <div class="w-11 h-6 bg-zinc-700 peer-focus:outline-none rounded-full peer peer-checked:after:translate-x-full peer-checked:after:border-white after:content-[''] after:absolute after:top-[2px] after:left-[2px] after:bg-white after:border-zinc-300 after:border after:rounded-full after:h-5 after:w-5 after:transition-all peer-checked:bg-blue-600"></div>
                                </label>
                            </div>
                            <div id="settingsOnTheGoRow" class="hidden flex items-center justify-between p-3 rounded-lg bg-[var(--bg-secondary)] border border-[var(--border-subtle)]">
                                <div class="flex flex-col">
                                    <span class="text-sm font-semibold text-white">On the Go Mode</span>
                                    <span class="text-xs text-zinc-400">Simplify UI with big buttons and hide video</span>
                                </div>
                                <label class="relative inline-flex items-center cursor-pointer">
                                    <input type="checkbox" id="settingsOnTheGo" onchange="handleOnTheGoChange(this.checked)" class="sr-only peer">
                                    <div class="w-11 h-6 bg-zinc-700 peer-focus:outline-none rounded-full peer peer-checked:after:translate-x-full peer-checked:after:border-white after:content-[''] after:absolute after:top-[2px] after:left-[2px] after:bg-white after:border-zinc-300 after:border after:rounded-full after:h-5 after:w-5 after:transition-all peer-checked:bg-blue-600"></div>
                                </label>
                            </div>
                        </div>
                    </div>
                </div>
            </div>

            <div class="pt-2 mt-2">
                <button onclick="closeSettings()" class="btn-primary w-full py-3.5 text-white rounded-lg font-semibold transition-all">
                    Close Settings
                </button>
            </div>
        </div>
    </div>

    <!-- On-the-go Mode Overlay -->
    <div id="onTheGoOverlay" class="fixed inset-0 z-[150] flex flex-col justify-between p-4 sm:p-6 gap-y-8 overflow-y-auto hidden" style="background: #000000;">
        <!-- Orientation Warning Overlay (only visible in landscape mode on mobile/tablet) -->
        <div id="otgOrientationWarning" class="fixed inset-0 z-[200] flex-col items-center justify-center p-6 bg-black text-center hidden">
            <div class="mb-6 otg-rotate-anim">
                <svg class="w-20 h-20 text-zinc-400 mx-auto" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="1.5">
                    <path stroke-linecap="round" stroke-linejoin="round" d="M10.5 1.5H8.25A2.25 2.25 0 0 0 6 3.75v16.5a2.25 2.25 0 0 0 2.25 2.25h7.5A2.25 2.25 0 0 0 18 20.25V3.75a2.25 2.25 0 0 0-2.25-2.25H13.5m-3 0V3h3V1.5m-3 0h3m-6 15h9" />
                </svg>
            </div>
            <h2 class="text-xl font-bold text-white mb-2">Rotate to Portrait</h2>
            <p class="text-zinc-500 text-sm max-w-xs mx-auto">On-the-go mode is optimized exclusively for portrait orientation. Please rotate your device.</p>
        </div>

        <!-- Top section: Status / Speaking Info -->
        <div class="flex flex-col items-center text-center mt-4 sm:mt-8 space-y-4 flex-shrink-0">
            <div class="status-pill-wrapper" id="onTheGoStatusPillWrapper">
                <div class="status-pill px-3 md:px-4 py-1.5 md:py-2 rounded-full flex items-center justify-center gap-2 md:gap-2.5 flex-shrink-0 h-8 md:h-10">
                    <div id="onTheGoConnectionDot" class="connection-dot"></div>
                    <!-- Orange lightning icon -->
                    <svg id="onTheGoLowBandwidthLightning" class="w-4 h-4 text-amber-500 hidden animate-pulse" fill="currentColor" viewBox="0 0 24 24" style="color: #f59e0b;" title="Low Bandwidth Mode Active">
                        <path d="M13 10V3L4 14h7v7l9-11h-7z" />
                    </svg>
                    <span id="onTheGoStatusText" class="text-xs md:text-sm font-medium" style="color: var(--text-primary);">Connected</span>
                    <button id="onTheGoBtnReconnect" onclick="event.stopPropagation(); retryConnection()" class="hidden ml-1 p-1 rounded-lg transition-all hover:bg-white/10" style="color: var(--text-muted);" title="Retry Connection">
                        <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M21 12a9 9 0 0 0-9-9 9.75 9.75 0 0 0-6.74 2.74L3 8"/><path d="M3 3v5h5"/><path d="M3 12a9 9 0 0 0 9 9 9.75 9.75 0 0 0 6.74-2.74L21 16"/><path d="M16 16h5v5"/></svg>
                    </button>
                    <div id="onTheGoPingContainer" class="ping-container hidden !ml-0 !pl-1.5 md:!ml-0 md:!pl-2 border-l !border-[var(--border-subtle)]">
                        <span id="onTheGoPingText" class="tabular-nums shrink-0 mr-1 md:mr-1.5">0ms</span>
                        <div id="onTheGoPingBars" class="ping-bars">
                            <div class="ping-bar ping-bar-1"></div>
                            <div class="ping-bar ping-bar-2"></div>
                            <div class="ping-bar ping-bar-3"></div>
                        </div>
                    </div>
                </div>
            </div>
            <div class="flex flex-col items-center space-y-3 pt-4 sm:pt-6">
                <!-- Large voice activity / speaking avatar container -->
                <div id="onTheGoAvatarWrapper" class="w-28 h-28 sm:w-32 sm:h-32 rounded-full flex items-center justify-center relative bg-zinc-800 border-2 border-zinc-700 shadow-2xl transition-all duration-300">
                    <img id="onTheGoAvatar" src="" class="w-full h-full object-cover rounded-full hidden" draggable="false">
                    <span id="onTheGoAvatarPlaceholder" class="text-5xl sm:text-6xl text-zinc-400">👤</span>
                    <!-- Speaking indicator wave/glow -->
                    <div id="onTheGoSpeakingGlow" class="absolute inset-0 rounded-full border-4 border-blue-500 scale-100 opacity-0 transition-all duration-300"></div>
                </div>
                <div class="space-y-1">
                    <h3 id="onTheGoSpeakingName" class="text-xl font-bold text-white">No one speaking</h3>
                    <p class="text-zinc-500 text-sm">Tap buttons below to control</p>
                </div>
            </div>
        </div>

        <!-- Bottom section: Big buttons taking up the whole lower portion of the screen -->
        <div class="grid grid-cols-2 gap-3 mb-4 sm:mb-8 w-full max-w-md mx-auto flex-shrink-0">
            <!-- Huge Mic Toggle Button -->
            <button id="btnOnTheGoMic" onclick="toggleMic()" class="flex flex-col items-center justify-center text-center gap-1.5 py-4 px-2 rounded-2xl font-bold text-sm sm:text-base text-white transition-all bg-zinc-800 hover:bg-zinc-700 border border-zinc-700 active:scale-[0.98]">
                <div id="onTheGoMicIconWrapper">
                    <svg class="w-7 h-7" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><path d="M12 2a3 3 0 0 0-3 3v7a3 3 0 0 0 6 0V5a3 3 0 0 0-3-3Z"/><path d="M19 10v2a7 7 0 0 1-14 0v-2"/><line x1="12" x2="12" y1="19" y2="22"/></svg>
                </div>
                <span id="onTheGoMicText" class="truncate w-full px-1">Mute</span>
            </button>

            <!-- Huge Deafen/Speaker Toggle Button -->
            <button id="btnOnTheGoDeafen" onclick="toggleDeafen()" class="flex flex-col items-center justify-center text-center gap-1.5 py-4 px-2 rounded-2xl font-bold text-sm sm:text-base text-white transition-all bg-zinc-800 hover:bg-zinc-700 border border-zinc-700 active:scale-[0.98]">
                <div id="onTheGoDeafenIconWrapper">
                    <svg class="w-7 h-7" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><path d="M3 18v-6a9 9 0 0 1 18 0v6"></path><path d="M21 19a2 2 0 0 1-2 2h-1a2 2 0 0 1-2-2v-3a2 2 0 0 1 2-2h3zM3 19a2 2 0 0 0 2 2h1a2 2 0 0 0 2-2v-3a2 2 0 0 0-2-2H3z"></path></svg>
                </div>
                <span id="onTheGoDeafenText" class="truncate w-full px-1">Deafen</span>
            </button>

            <!-- Huge Low Bandwidth Toggle Button -->
            <button id="btnOnTheGoLowBandwidth" onclick="toggleOnTheGoLowBandwidth()" class="flex flex-col items-center justify-center text-center gap-1.5 py-4 px-2 rounded-2xl font-bold text-sm sm:text-base text-white transition-all bg-zinc-800 hover:bg-zinc-700 border border-zinc-700 active:scale-[0.98]">
                <div id="onTheGoLowBandwidthIconWrapper">
                    <svg class="w-7 h-7" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><path d="M13 10V3L4 14h7v7l9-11h-7z" /></svg>
                </div>
                <span id="onTheGoLowBandwidthText" class="truncate w-full px-1">Low Bandwidth Mode</span>
            </button>

            <!-- Huge Copy Invite Link Button -->
            <button id="btnOnTheGoCopy" onclick="copyLink()" class="flex flex-col items-center justify-center text-center gap-1.5 py-4 px-2 rounded-2xl font-bold text-sm sm:text-base text-white transition-all bg-zinc-800 hover:bg-zinc-700 border border-zinc-700 active:scale-[0.98]">
                <div id="onTheGoCopyIconWrapper">
                    <svg class="w-7 h-7" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><rect width="14" height="14" x="8" y="8" rx="2" ry="2"/><path d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2"/></svg>
                </div>
                <span id="onTheGoCopyText" class="truncate w-full px-1">Copy Invite Link</span>
            </button>

            <!-- Exit On-the-go -->
            <button onclick="toggleOnTheGoMode(false)" class="flex flex-col items-center justify-center py-4 rounded-2xl font-bold text-sm text-zinc-300 transition-all bg-zinc-800 hover:bg-zinc-700 border border-zinc-700 active:scale-[0.98]">
                <span>Exit On-the-go</span>
            </button>

            <!-- Leave Call -->
            <button onclick="leaveRoom()" class="flex flex-col items-center justify-center py-4 rounded-2xl font-bold text-sm text-white transition-all bg-red-600 hover:bg-red-700 active:scale-[0.98] shadow-lg shadow-red-600/20">
                <span>Leave Call</span>
            </button>
        </div>
    </div>

    <div id="appLayout" class="hidden flex-col h-full w-full">
        <div class="app-topbar flex-none p-3 sm:p-4 md:p-5 z-40 flex justify-between items-center gap-2 md:gap-4 pl-3 md:pl-4" style="background: #000000; border-bottom: 1px solid var(--border-subtle);">
            <div class="flex items-center gap-2 md:gap-3 flex-1 min-w-0">
                <button id="sidebarToggle" onclick="toggleSidebar()" class="control-btn shadow-lg hidden !w-10 !h-10 md:!w-12 md:!h-12 flex-shrink-0" title="Channels">
                    <svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="3" y1="12" x2="21" y2="12"></line><line x1="3" y1="6" x2="21" y2="6"></line><line x1="3" y1="18" x2="21" y2="18"></line></svg>
                </button>
                <div id="currentChannelName" class="text-white font-semibold text-lg md:text-xl truncate min-w-0 drop-shadow-md"></div>
            </div>

            <div class="flex items-center justify-end gap-2 md:gap-3 flex-shrink-0">
                <div class="status-pill-wrapper" id="statusPillWrapper">
                    <div class="status-pill px-3 md:px-4 py-1.5 md:py-2 rounded-full flex items-center justify-center gap-2 md:gap-2.5 flex-shrink-0 h-8 md:h-10" onclick="toggleStatsWindow()">
                        <div id="connectionDot" class="connection-dot"></div>
                        <!-- Orange lightning icon -->
                        <svg id="lowBandwidthLightning" class="w-4 h-4 text-amber-500 hidden animate-pulse" fill="currentColor" viewBox="0 0 24 24" style="color: #f59e0b;" title="Low Bandwidth Mode Active">
                            <path d="M13 10V3L4 14h7v7l9-11h-7z" />
                        </svg>
                        <span id="statusText" class="text-xs md:text-sm font-medium hidden sm:inline-block" style="color: var(--text-primary);">Waiting...</span>
                        <button id="btnReconnect" onclick="event.stopPropagation(); retryConnection()" class="hidden ml-1 p-1 rounded-lg transition-all hover:bg-white/10" style="color: var(--text-muted);" title="Retry Connection">
                            <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M21 12a9 9 0 0 0-9-9 9.75 9.75 0 0 0-6.74 2.74L3 8"/><path d="M3 3v5h5"/><path d="M3 12a9 9 0 0 0 9 9 9.75 9.75 0 0 0 6.74-2.74L21 16"/><path d="M16 16h5v5"/></svg>
                        </button>
                        <div id="pingContainer" class="ping-container hidden !ml-0 !pl-1.5 md:!ml-0 md:!pl-2 border-l !border-[var(--border-subtle)]">
                            <span id="pingText" class="tabular-nums shrink-0 mr-1 md:mr-1.5">0ms</span>
                            <div id="pingBars" class="ping-bars">
                                <div class="ping-bar ping-bar-1"></div>
                                <div class="ping-bar ping-bar-2"></div>
                                <div class="ping-bar ping-bar-3"></div>
                            </div>
                        </div>
                    </div>
                </div>

                <div id="btnCopy" class="status-pill px-3 md:px-4 py-1.5 md:py-2 rounded-full cursor-pointer transition-all flex items-center justify-center gap-2 hover:border-opacity-30 flex-shrink-0 h-8 md:h-10" onclick="copyLink()" title="Invite Link">
                    <span class="text-xs md:text-sm font-medium hidden md:inline-block" style="color: var(--text-primary);">Invite</span>
                    <svg id="iconCopy" xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect width="14" height="14" x="8" y="8" rx="2" ry="2"/><path d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2"/></svg>
                </div>
            </div>
        </div>

        <main class="flex-1 w-full relative min-h-0">
            <div class="absolute inset-0 pb-4 md:pb-5 px-4 pt-1 md:pt-2 overflow-y-auto flex justify-center">
                 <div id="remoteGrid" class="grid gap-3 md:gap-4 w-full h-full max-w-[1600px] transition-all duration-500 grid-expand my-auto"></div>
            </div>

            <div id="emptyState" class="hidden absolute top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 text-center pointer-events-none">
                <div class="mb-6">
                    <svg class="mx-auto h-20 w-20 empty-state-icon" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="0.5" d="M17 20h5v-2a3 3 0 00-5.356-1.857M17 20H7m10 0v-2c0-.656-.126-1.283-.356-1.857M7 20H2v-2a3 3 0 015.356-1.857M7 20v-2c0-.656.126-1.283.356-1.857m0 0a5.002 5.002 0 019.288 0M15 7a3 3 0 11-6 0 3 3 0 016 0zm6 3a2 2 0 11-4 0 2 2 0 014 0zM7 10a2 2 0 11-4 0 2 2 0 014 0z" />
                    </svg>
                </div>
                <p class="text-lg font-semibold" style="color: var(--text-secondary);">Waiting for others to join...</p>
                <p class="text-sm mt-2" style="color: var(--text-muted); opacity: 0.7;">Share the invite link to get started.</p>
            </div>

            <div class="pip-wrapper" id="localPipWrapper">
                 <div class="w-full h-full relative flex flex-col">
                    <div id="localAvatarLayer" class="absolute inset-0 z-20 flex items-center justify-center" style="display: none; background: var(--bg-secondary);">
                        <img id="localAvatarImg" src="" class="absolute inset-0 w-full h-full object-cover blur-xl opacity-30 hidden" draggable="false">
                        <div class="relative w-14 h-14 md:w-20 md:h-20 rounded-lg flex items-center justify-center overflow-hidden z-10" style="background: var(--bg-secondary); border: 2px solid var(--border-subtle);">
                             <img id="localAvatarCenterImg" src="" class="w-full h-full object-cover hidden" draggable="false">
                             <div id="localAvatarPlaceholder" class="text-2xl md:text-3xl flex items-center justify-center w-full h-full" style="color: var(--text-muted); line-height: 1;">👤</div>
                        </div>
                    </div>

                    <video id="localVideo" autoplay playsinline muted class="w-full h-full object-cover z-10"></video>
                    <div id="localLabel" class="name-tag absolute bottom-2 left-2 px-2.5 py-1 rounded-lg text-[10px] md:text-xs font-medium z-30" style="background: rgba(0, 0, 0, 0.55); backdrop-filter: blur(12px); -webkit-backdrop-filter: blur(12px); color: var(--text-primary); border: 1px solid var(--border-subtle);">
                        You
                    </div>
                </div>
            </div>
        </main>

        <footer class="flex-none taskbar w-full z-50">
            <div class="flex justify-center items-center py-4 md:py-5 gap-2 md:gap-2.5 px-4">
                <button class="control-btn" id="btnMic" onclick="toggleMic()" title="Toggle Microphone">
                    <svg xmlns="http://www.w3.org/2000/svg" width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 2a3 3 0 0 0-3 3v7a3 3 0 0 0 6 0V5a3 3 0 0 0-3-3Z"/><path d="M19 10v2a7 7 0 0 1-14 0v-2"/><line x1="12" x2="12" y1="19" y2="22"/></svg>
                </button>
                <button class="control-btn" id="btnDeafen" onclick="toggleDeafen()" title="Deafen (D)">
                    <svg xmlns="http://www.w3.org/2000/svg" width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M3 18v-6a9 9 0 0 1 18 0v6"></path><path d="M21 19a2 2 0 0 1-2 2h-1a2 2 0 0 1-2-2v-3a2 2 0 0 1 2-2h3zM3 19a2 2 0 0 0 2 2h1a2 2 0 0 0 2-2v-3a2 2 0 0 0-2-2H3z"></path></svg>
                </button>
                <button class="control-btn" id="btnCam" onclick="toggleCam()" title="Toggle Camera" disabled>
                    <svg xmlns="http://www.w3.org/2000/svg" width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M14.5 4h-5L7 7H4a2 2 0 0 0-2 2v9a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2V9a2 2 0 0 0-2-2h-3l-2.5-3z"/><circle cx="12" cy="13" r="3"/></svg>
                </button>
                <button class="control-btn hidden" id="btnSwitchCam" onclick="switchCamera()" title="Switch Camera">
                    <svg xmlns="http://www.w3.org/2000/svg" width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M11 19H4a2 2 0 0 1-2-2V7a2 2 0 0 1 2-2h5"/><path d="M13 5h7a2 2 0 0 1 2 2v10a2 2 0 0 1-2 2h-5"/><path d="m15 3-3 3 3 3"/><path d="m9 21 3-3-3-3"/></svg>
                </button>
                <button class="control-btn" id="btnShare" onclick="toggleScreen()" title="Share Screen">
                    <svg xmlns="http://www.w3.org/2000/svg" width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect width="20" height="14" x="2" y="3" rx="2"/><line x1="8" x2="16" y1="21" y2="21"/><line x1="12" x2="12" y1="17" y2="21"/></svg>
                </button>
                <button class="control-btn hidden" id="btnOnTheGo" onclick="toggleOnTheGoMode(true)" title="On the Go">
                    <svg xmlns="http://www.w3.org/2000/svg" width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="5" y="2" width="14" height="20" rx="2" ry="2"></rect><line x1="12" y1="18" x2="12.01" y2="18"></line></svg>
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
            const overlay = document.getElementById('welcomeOverlay');
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
                if (particles.length === 0) init();

                ctx.clearRect(0, 0, canvas.width, canvas.height);
                particles.forEach(p => {
                    p.update();
                    p.draw();
                });
                animationId = requestAnimationFrame(animate);
            }

            function checkVisibility() {
                const style = window.getComputedStyle(overlay);
                const isVisible = style.display !== 'none' && 
                                  style.visibility !== 'hidden' && 
                                  style.opacity !== '0' && 
                                  !document.hidden;
                if (isVisible) {
                    if (!animationId) {
                        animate();
                    }
                } else {
                    if (animationId) {
                        cancelAnimationFrame(animationId);
                        animationId = null;
                    }
                    particles = [];
                    ctx.clearRect(0, 0, canvas.width, canvas.height);
                }
            }

            if (overlay) {
                checkVisibility();

                const observer = new MutationObserver(() => {
                    checkVisibility();
                });
                observer.observe(overlay, { attributes: true, attributeFilter: ['style', 'class'] });

                document.addEventListener('visibilitychange', checkVisibility);
            }
        })();

        (function() {
            const canvas = document.getElementById('particleCanvasConfig');
            const ctx = canvas.getContext('2d');
            const overlay = document.getElementById('configOverlay');
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
                if (particles.length === 0) init();

                ctx.clearRect(0, 0, canvas.width, canvas.height);
                particles.forEach(p => {
                    p.update();
                    p.draw();
                });
                animationId = requestAnimationFrame(animate);
            }

            function checkVisibility() {
                const style = window.getComputedStyle(overlay);
                const isVisible = style.display !== 'none' && 
                                  style.visibility !== 'hidden' && 
                                  style.opacity !== '0' && 
                                  !document.hidden;
                if (isVisible) {
                    if (!animationId) {
                        animate();
                    }
                } else {
                    if (animationId) {
                        cancelAnimationFrame(animationId);
                        animationId = null;
                    }
                    particles = [];
                    ctx.clearRect(0, 0, canvas.width, canvas.height);
                }
            }

            if (overlay) {
                checkVisibility();

                const observer = new MutationObserver(() => {
                    checkVisibility();
                });
                observer.observe(overlay, { attributes: true, attributeFilter: ['style', 'class'] });

                document.addEventListener('visibilitychange', checkVisibility);
            }
        })();

        // Particle background for invite welcome overlay
        (function() {
            const canvas = document.getElementById('particleCanvasInvite');
            if (!canvas) return;
            const ctx = canvas.getContext('2d');
            const overlay = document.getElementById('inviteWelcomeOverlay');
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
                if (particles.length === 0) init();

                ctx.clearRect(0, 0, canvas.width, canvas.height);
                particles.forEach(p => {
                    p.update();
                    p.draw();
                });
                animationId = requestAnimationFrame(animate);
            }

            function checkVisibility() {
                const style = window.getComputedStyle(overlay);
                const isVisible = style.display !== 'none' && 
                                  style.visibility !== 'hidden' && 
                                  style.opacity !== '0' && 
                                  !document.hidden;
                if (isVisible) {
                    if (!animationId) {
                        animate();
                    }
                } else {
                    if (animationId) {
                        cancelAnimationFrame(animationId);
                        animationId = null;
                    }
                    particles = [];
                    ctx.clearRect(0, 0, canvas.width, canvas.height);
                }
            }

            if (overlay) {
                checkVisibility();

                const observer = new MutationObserver(() => {
                    checkVisibility();
                });
                observer.observe(overlay, { attributes: true, attributeFilter: ['style', 'class'] });

                document.addEventListener('visibilitychange', checkVisibility);
            }
        })();
    </script>
    <script>
        let parts = window.location.pathname.split('/').filter(p => p !== '');
        let roomId = parts[0] || '';
        let channelId = decodeURIComponent(parts[1] || '') || (roomId ? 'General' : '');
        if (channelId.toLowerCase() === 'general') {
            channelId = 'General';
        }
        if (channelId.length > 32) channelId = channelId.substring(0, 32);

        const initialChannelNameEl = document.getElementById('currentChannelName');
        if (initialChannelNameEl && channelId) {
            initialChannelNameEl.innerText = `# ${channelId}`;
        }

        const currentPath = window.location.pathname;
        const newPath = `/${roomId}${channelId && channelId.toLowerCase() !== 'general' ? '/' + encodeURIComponent(channelId) : ''}`;
        if (currentPath !== newPath && roomId) {
            window.history.replaceState({ roomId, channelId }, "", newPath);
        }

        const wsProtocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
        let wsUrl = roomId ? `${wsProtocol}//${window.location.host}/ws/${roomId}/${encodeURIComponent(channelId)}` : '';

        const isIOS = (/iPad|iPhone|iPod/.test(navigator.userAgent) && !window.MSStream) || (navigator.platform === 'MacIntel' && navigator.maxTouchPoints > 1);
        let ws;
        let localStream;
        let screenStream;
        let peers = {};
        let peerCamStatus = {};
        let peerScreenStatus = {};
        let peerScreenHasAudio = {};
        let peerMicTrackId = {};
        let peerScreenAudioTrackId = {};
        let pendingCandidates = {};
        let userNickname = "Guest";
        let userAvatar = null;
        let userAvatarIsGif = false;
        let userAvatarStaticFrame = null;
        let sidebarOpen = false;
        let globalRoomList = {};
        let isConfigured = false;
        let audioContext;
        let wakeLock = null;
        let currentAudioOutputId = 'default';
        let currentAudioInputId = null;
        let currentVideoInputId = null;
        let currentFacingMode = 'user';
        let isDeafened = false;
        let roomCreationPassword = sessionStorage.getItem('rustrooms_room_password');
        let workletLoadingPromise = null;
        let isLowBandwidthMode = false;
        let isOnTheGoMode = false;
        let activeSpeakers = {};
        let peerLowBandwidthStatus = {};
        let peerOnTheGoStatus = {};

        let persistentUserId = localStorage.getItem('rustrooms_user_id');
        if (!persistentUserId) {
            persistentUserId = crypto.randomUUID();
            localStorage.setItem('rustrooms_user_id', persistentUserId);
        }

        let reconnectionAttempts = 0;
        const maxReconnectionAttempts = isIOS ? 50 : 20;
        const baseReconnectionDelay = 1000;
        const maxReconnectionDelay = isIOS ? 15000 : 30000;
        let hasLeftRoom = true;
        let isReconnecting = false;
        let awaitingPassword = false;
        let desktopSlowRetryCount = 0;
        let desktopSlowRetryTimer = null;

        const tabId = crypto.randomUUID();
        let tabHeartbeatInterval = null;
        let activeTabSessionKey = null;
        let isUnloading = false;

        // Intercept getUserMedia to apply appropriate bandwidth constraints
        (function() {
            if (navigator.mediaDevices && navigator.mediaDevices.getUserMedia) {
                const originalGetUserMedia = navigator.mediaDevices.getUserMedia.bind(navigator.mediaDevices);
                navigator.mediaDevices.getUserMedia = function(constraints) {
                    if (constraints && constraints.video) {
                        if (isLowBandwidthMode) {
                            if (typeof constraints.video === 'boolean') {
                                constraints.video = {
                                    width: { max: 320 },
                                    height: { max: 240 },
                                    frameRate: { max: 15 }
                                };
                            } else if (typeof constraints.video === 'object') {
                                constraints.video.width = { max: 320 };
                                constraints.video.height = { max: 240 };
                                constraints.video.frameRate = { max: 15 };
                            }
                        } else {
                            if (typeof constraints.video === 'boolean') {
                                constraints.video = {
                                    width: { ideal: 1280 },
                                    height: { ideal: 720 },
                                    frameRate: { ideal: 30 }
                                };
                            } else if (typeof constraints.video === 'object') {
                                if (constraints.video.width === undefined) {
                                    constraints.video.width = { ideal: 1280 };
                                }
                                if (constraints.video.height === undefined) {
                                    constraints.video.height = { ideal: 720 };
                                }
                                if (constraints.video.frameRate === undefined) {
                                    constraints.video.frameRate = { ideal: 30 };
                                }
                            }
                        }
                    }
                    return originalGetUserMedia(constraints);
                };
            }

            // Intercept RTCPeerConnection.prototype.addTrack to enforce low-bandwidth bitrates
            const originalAddTrack = RTCPeerConnection.prototype.addTrack;
            RTCPeerConnection.prototype.addTrack = function(track, ...streams) {
                const sender = originalAddTrack.apply(this, [track, ...streams]);
                
                // Find the peer user ID associated with this connection
                let targetUserId = null;
                for (const uId in peers) {
                    if (peers[uId] === this) {
                        targetUserId = uId;
                        break;
                    }
                }

                const isRemoteLBM = targetUserId && (peerLowBandwidthStatus[targetUserId] === true);
                if (isLowBandwidthMode || isRemoteLBM) {
                    setTimeout(() => {
                        try {
                            const params = sender.getParameters();
                            if (!params.encodings) params.encodings = [{}];
                            if (track.kind === 'video') {
                                const isScreen = screenStream && screenStream.getVideoTracks().includes(track);
                                params.encodings[0].maxBitrate = isScreen ? 150000 : 80000;
                                params.encodings[0].scaleResolutionDownBy = isScreen ? 1.5 : 2.0;
                            } else if (track.kind === 'audio') {
                                params.encodings[0].maxBitrate = 16000;
                            }
                            sender.setParameters(params).catch(e => console.warn("Failed to set low-bandwidth params on addTrack:", e));
                        } catch (e) {
                            console.warn("Failed to apply track parameters in wrapper:", e);
                        }
                    }, 100);
                }
                return sender;
            };
        })();

        function updateAllSenderBitrates() {
            for (const userId in peers) {
                const pc = peers[userId];
                if (!pc) continue;
                const isRemoteLBM = peerLowBandwidthStatus[userId] === true;
                const shouldLimit = isLowBandwidthMode || isRemoteLBM;
                pc.getSenders().forEach(sender => {
                    if (sender.track) {
                        try {
                            const params = sender.getParameters();
                            if (!params.encodings) params.encodings = [{}];
                            if (sender.track.kind === 'video') {
                                const isScreen = screenStream && screenStream.getVideoTracks().includes(sender.track);
                                if (shouldLimit) {
                                    params.encodings[0].maxBitrate = isScreen ? 150000 : 80000;
                                    params.encodings[0].scaleResolutionDownBy = isScreen ? 1.5 : 2.0;
                                } else {
                                    delete params.encodings[0].maxBitrate;
                                    delete params.encodings[0].scaleResolutionDownBy;
                                }
                            } else if (sender.track.kind === 'audio') {
                                if (shouldLimit) {
                                    params.encodings[0].maxBitrate = 16000;
                                } else {
                                    delete params.encodings[0].maxBitrate;
                                }
                            }
                            sender.setParameters(params).catch(e => console.warn("Failed to dynamically update sender params:", e));
                        } catch (e) {
                            console.warn("Failed to update sender params:", e);
                        }
                    }
                });
            }
        }

        async function updateLocalVideoConstraints() {
            if (localStream) {
                const videoTrack = localStream.getVideoTracks()[0];
                if (videoTrack) {
                    try {
                        if (isLowBandwidthMode) {
                            await videoTrack.applyConstraints({
                                width: { max: 320 },
                                height: { max: 240 },
                                frameRate: { max: 15 }
                            });
                        } else {
                            await videoTrack.applyConstraints({
                                width: { ideal: 1280 },
                                height: { ideal: 720 },
                                frameRate: { ideal: 30 }
                            });
                        }
                    } catch (e) {
                        console.warn("Failed to apply dynamic video constraints:", e);
                    }
                }
            }
        }

        function updateLowBandwidthBadgeVisibility() {
            const lightning = document.getElementById('lowBandwidthLightning');
            if (lightning) {
                if (isLowBandwidthMode) {
                    lightning.classList.remove('hidden');
                } else {
                    lightning.classList.add('hidden');
                }
            }
            const otgLightning = document.getElementById('onTheGoLowBandwidthLightning');
            if (otgLightning) {
                if (isLowBandwidthMode) {
                    otgLightning.classList.remove('hidden');
                } else {
                    otgLightning.classList.add('hidden');
                }
            }
            updateOnTheGoButtons();
        }

        async function handleLowBandwidthChange(checked) {
            isLowBandwidthMode = checked;
            const setupLBM = document.getElementById('setupLowBandwidth');
            const settingsLBM = document.getElementById('settingsLowBandwidth');
            if (setupLBM) setupLBM.checked = checked;
            if (settingsLBM) settingsLBM.checked = checked;
            savePreferences();
            updateLowBandwidthBadgeVisibility();
            updateAllSenderBitrates();
            await updateLocalVideoConstraints();
            updateLocalLabel();

            if (ws && ws.readyState === WebSocket.OPEN) {
                ws.send(JSON.stringify({
                    type: 'update-user',
                    data: { isLowBandwidthMode: checked }
                }));
            }
        }

        async function toggleOnTheGoLowBandwidth() {
            await handleLowBandwidthChange(!isLowBandwidthMode);
            if (isLowBandwidthMode) {
                playNotificationSound('bandwidth_on');
            } else {
                playNotificationSound('bandwidth_off');
            }
        }

        function handleOnTheGoChange(checked) {
            isOnTheGoMode = checked;
            const setupOtg = document.getElementById('setupOnTheGo');
            const settingsOtg = document.getElementById('settingsOnTheGo');
            if (setupOtg) setupOtg.checked = checked;
            if (settingsOtg) settingsOtg.checked = checked;
            savePreferences();
            toggleOnTheGoMode(checked);
        }

        function toggleOnTheGoMode(enable, forceShow) {
            isOnTheGoMode = enable;
            const setupOtg = document.getElementById('setupOnTheGo');
            const settingsOtg = document.getElementById('settingsOnTheGo');
            if (setupOtg) setupOtg.checked = enable;
            if (settingsOtg) settingsOtg.checked = enable;

            const otgOverlay = document.getElementById('onTheGoOverlay');
            if (otgOverlay) {
                if (enable) {
                    const configOverlay = document.getElementById('configOverlay');
                    const settingsOverlay = document.getElementById('settingsOverlay');
                    const configOpen = configOverlay && !configOverlay.classList.contains('hidden') && configOverlay.style.display !== 'none';
                    const settingsOpen = settingsOverlay && !settingsOverlay.classList.contains('hidden');

                    if (forceShow || (!configOpen && !settingsOpen)) {
                        otgOverlay.classList.remove('hidden');
                        
                        // Try locking screen orientation to portrait
                        if (screen.orientation && screen.orientation.lock) {
                            screen.orientation.lock('portrait').catch(err => {
                                console.log('Screen orientation lock failed or not supported:', err);
                            });
                        }
                        
                        // Auto-disable camera if active when enabling On-the-go mode
                        const videoTracks = localStream ? localStream.getVideoTracks() : [];
                        if (videoTracks.length > 0) {
                            const track = videoTracks[0];
                            track.stop();
                            localStream.removeTrack(track);

                            if (localStream._originalStream) {
                                localStream._originalStream.getVideoTracks().forEach(t => t.stop());
                            }

                            const btnPreviewCam = document.getElementById('btnPreviewCam');
                            if (btnPreviewCam) {
                                btnPreviewCam.classList.add('active-red');
                                btnPreviewCam.innerText = "Start Cam";
                                const placeholder = document.getElementById('previewPlaceholder');
                                if (placeholder) placeholder.style.display = 'flex';
                            }

                            const btnCam = document.getElementById('btnCam');
                            if (btnCam) {
                                btnCam.classList.add('active-red');
                                btnCam.innerHTML = `<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="1" y1="1" x2="23" y2="23"></line><path d="M21 21l-3.5-3.5m-2-2l-2-2m-2-2l-2-2m-2-2l-3.5-3.5"></path><path d="M15 7h5a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2h-5"></path><path d="M4 8v8a2 2 0 0 0 2 2h4.5"></path></svg>`;
                            }

                            if (ws && ws.readyState === WebSocket.OPEN) {
                                ws.send(JSON.stringify({
                                    type: 'cam-toggle',
                                    data: { enabled: false }
                                }));
                            }

                            for (const userId in peers) {
                                const pc = peers[userId];
                                const sender = pc.getSenders().find(s => s.track && s.track.kind === 'video');
                                if (sender) {
                                    pc.removeTrack(sender);
                                }
                            }

                            const previewVideo = document.getElementById('previewVideo');
                            if (previewVideo) previewVideo.srcObject = null;
                            const localVideo = document.getElementById('localVideo');
                            if (localVideo) localVideo.srcObject = null;

                            pendingCamToggle = true;
                            updateLocalAvatar();
                        }
                    }
                } else {
                    otgOverlay.classList.add('hidden');
                    // Unlock screen orientation
                    if (screen.orientation && screen.orientation.unlock) {
                        try {
                            screen.orientation.unlock();
                        } catch(e) {}
                    }
                }
            }
            updateOnTheGoButtons();
            savePreferences();
            updateLocalLabel();

            if (ws && ws.readyState === WebSocket.OPEN) {
                ws.send(JSON.stringify({
                    type: 'update-user',
                    data: { isOnTheGoMode: enable }
                }));
            }
        }

        function updateOnTheGoButtons() {
            const otgMicBtn = document.getElementById('btnOnTheGoMic');
            const otgDeafenBtn = document.getElementById('btnOnTheGoDeafen');
            const otgLbmBtn = document.getElementById('btnOnTheGoLowBandwidth');
            const otgMicWrapper = document.getElementById('onTheGoMicIconWrapper');
            const otgDeafenWrapper = document.getElementById('onTheGoDeafenIconWrapper');
            const otgLbmWrapper = document.getElementById('onTheGoLowBandwidthIconWrapper');
            const otgMicText = document.getElementById('onTheGoMicText');
            const otgDeafenText = document.getElementById('onTheGoDeafenText');
            const otgLbmText = document.getElementById('onTheGoLowBandwidthText');

            const isMicMuted = localStream && localStream.getAudioTracks().length > 0 ? !localStream.getAudioTracks()[0].enabled : true;

            if (otgMicBtn) {
                if (isDeafened) {
                    otgMicBtn.classList.add('bg-red-950', 'border-red-900', 'opacity-50', 'cursor-not-allowed');
                    otgMicBtn.classList.remove('bg-red-600', 'hover:bg-red-700', 'border-red-500', 'bg-zinc-800', 'hover:bg-zinc-700', 'border-zinc-700');
                    if (otgMicText) otgMicText.innerText = "Unmute";
                    if (otgMicWrapper) {
                        otgMicWrapper.innerHTML = `<svg class="w-7 h-7" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><line x1="1" y1="1" x2="23" y2="23"></line><path d="M9 9v3a3 3 0 0 0 5.12 2.12M15 9.34V4a3 3 0 0 0-5.94-.6"></path><path d="M17 16.95A7 7 0 0 1 5 12v-2m14 0v2a7 7 0 0 1-.11 1.23"></path><line x1="12" x2="12" y1="19" y2="22"></line></svg>`;
                    }
                } else if (isMicMuted) {
                    otgMicBtn.classList.add('bg-red-600', 'hover:bg-red-700', 'border-red-500');
                    otgMicBtn.classList.remove('bg-red-950', 'border-red-900', 'opacity-50', 'cursor-not-allowed', 'bg-zinc-800', 'hover:bg-zinc-700', 'border-zinc-700');
                    if (otgMicText) otgMicText.innerText = "Unmute";
                    if (otgMicWrapper) {
                        otgMicWrapper.innerHTML = `<svg class="w-7 h-7" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><line x1="1" y1="1" x2="23" y2="23"></line><path d="M9 9v3a3 3 0 0 0 5.12 2.12M15 9.34V4a3 3 0 0 0-5.94-.6"></path><path d="M17 16.95A7 7 0 0 1 5 12v-2m14 0v2a7 7 0 0 1-.11 1.23"></path><line x1="12" x2="12" y1="19" y2="22"></line></svg>`;
                    }
                } else {
                    otgMicBtn.classList.remove('bg-red-600', 'hover:bg-red-700', 'border-red-500', 'bg-red-950', 'border-red-900', 'opacity-50', 'cursor-not-allowed');
                    otgMicBtn.classList.add('bg-zinc-800', 'hover:bg-zinc-700', 'border-zinc-700');
                    if (otgMicText) otgMicText.innerText = "Mute";
                    if (otgMicWrapper) {
                        otgMicWrapper.innerHTML = `<svg class="w-7 h-7" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><path d="M12 2a3 3 0 0 0-3 3v7a3 3 0 0 0 6 0V5a3 3 0 0 0-3-3Z"/><path d="M19 10v2a7 7 0 0 1-14 0v-2"/><line x1="12" x2="12" y1="19" y2="22"/></svg>`;
                    }
                }
            }

            if (otgDeafenBtn) {
                if (isDeafened) {
                    otgDeafenBtn.classList.add('bg-red-600', 'hover:bg-red-700', 'border-red-500');
                    otgDeafenBtn.classList.remove('bg-zinc-800', 'hover:bg-zinc-700', 'border-zinc-700');
                    if (otgDeafenText) otgDeafenText.innerText = "Undeafen";
                    if (otgDeafenWrapper) {
                        otgDeafenWrapper.innerHTML = `<svg class="w-7 h-7" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><line x1="1" y1="1" x2="23" y2="23"></line><path d="M21 14a2 2 0 0 0-2-2h-3a2 2 0 0 0-2 2v3a2 2 0 0 0 2 2h1a2 2 0 0 0 2-2V14z"></path><path d="M3 14a2 2 0 0 1 2-2h3a2 2 0 0 1 2 2v3a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V14z"></path><path d="M20.4 10.4C20.2 6.5 17 3.5 13 3.1"></path><path d="M6.5 5.5A9 9 0 0 0 3 12"></path></svg>`;
                    }
                } else {
                    otgDeafenBtn.classList.remove('bg-red-600', 'hover:bg-red-700', 'border-red-500');
                    otgDeafenBtn.classList.add('bg-zinc-800', 'hover:bg-zinc-700', 'border-zinc-700');
                    if (otgDeafenText) otgDeafenText.innerText = "Deafen";
                    if (otgDeafenWrapper) {
                        otgDeafenWrapper.innerHTML = `<svg class="w-7 h-7" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><path d="M3 18v-6a9 9 0 0 1 18 0v6"></path><path d="M21 19a2 2 0 0 1-2 2h-1a2 2 0 0 1-2-2v-3a2 2 0 0 1 2-2h3zM3 19a2 2 0 0 0 2 2h1a2 2 0 0 0 2-2v-3a2 2 0 0 0-2-2H3z"></path></svg>`;
                    }
                }
            }

            if (otgLbmBtn) {
                if (isLowBandwidthMode) {
                    otgLbmBtn.classList.add('bg-amber-600', 'hover:bg-amber-700', 'border-amber-500');
                    otgLbmBtn.classList.remove('bg-zinc-800', 'hover:bg-zinc-700', 'border-zinc-700');
                    if (otgLbmText) otgLbmText.innerText = "Low Bandwidth Active";
                    if (otgLbmWrapper) {
                        otgLbmWrapper.innerHTML = `<svg class="w-7 h-7 text-white" fill="currentColor" viewBox="0 0 24 24"><path d="M13 10V3L4 14h7v7l9-11h-7z" /></svg>`;
                    }
                } else {
                    otgLbmBtn.classList.remove('bg-amber-600', 'hover:bg-amber-700', 'border-amber-500');
                    otgLbmBtn.classList.add('bg-zinc-800', 'hover:bg-zinc-700', 'border-zinc-700');
                    if (otgLbmText) otgLbmText.innerText = "Low Bandwidth Mode";
                    if (otgLbmWrapper) {
                        otgLbmWrapper.innerHTML = `<svg class="w-7 h-7" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><path d="M13 10V3L4 14h7v7l9-11h-7z" /></svg>`;
                    }
                }
            }
        }

        function updateOnTheGoSpeakingIndicator() {
            if (!isOnTheGoMode) return;
            
            let maxVol = 0;
            let speakerId = null;
            for (const id in activeSpeakers) {
                if (activeSpeakers[id] > maxVol) {
                    maxVol = activeSpeakers[id];
                    speakerId = id;
                }
            }

            const otgName = document.getElementById('onTheGoSpeakingName');
            const otgAvatar = document.getElementById('onTheGoAvatar');
            const otgPlaceholder = document.getElementById('onTheGoAvatarPlaceholder');
            const otgGlow = document.getElementById('onTheGoSpeakingGlow');
            const otgAvatarWrapper = document.getElementById('onTheGoAvatarWrapper');

            if (!speakerId) {
                if (otgName) otgName.innerText = "No one speaking";
                if (otgAvatar) otgAvatar.classList.add('hidden');
                if (otgPlaceholder) otgPlaceholder.classList.remove('hidden');
                if (otgGlow) {
                    otgGlow.classList.remove('scale-105', 'opacity-100');
                    otgGlow.classList.add('scale-100', 'opacity-0');
                }
                if (otgAvatarWrapper) otgAvatarWrapper.classList.remove('otg-speaking-pulse');
                return;
            }

            let name = "Guest";
            let avatarUrl = null;

            if (speakerId === 'local') {
                name = userNickname || "You";
                avatarUrl = userAvatar;
            } else {
                const rawUserId = speakerId.startsWith('wrapper-') ? speakerId.replace('wrapper-', '') : speakerId;
                const wrapper = document.getElementById(`wrapper-${rawUserId}`);
                if (wrapper) {
                    const labelEl = wrapper.querySelector('.name-tag');
                    if (labelEl) {
                        name = labelEl.textContent.trim();
                    }
                    const imgEl = wrapper.querySelector('.avatar-center img');
                    if (imgEl && !imgEl.classList.contains('hidden')) {
                        avatarUrl = imgEl.src;
                    }
                }
            }

            if (otgName) otgName.innerText = name;
            if (avatarUrl) {
                if (otgAvatar) {
                    otgAvatar.src = avatarUrl;
                    otgAvatar.classList.remove('hidden');
                }
                if (otgPlaceholder) otgPlaceholder.classList.add('hidden');
            } else {
                if (otgAvatar) otgAvatar.classList.add('hidden');
                if (otgPlaceholder) otgPlaceholder.classList.remove('hidden');
            }

            if (otgGlow) {
                otgGlow.classList.remove('scale-100', 'opacity-0');
                otgGlow.classList.add('scale-105', 'opacity-100');
            }
            if (otgAvatarWrapper) otgAvatarWrapper.classList.add('otg-speaking-pulse');
        }

        function setActiveTabSession() {
            try {
                if (!activeTabSessionKey) activeTabSessionKey = 'rustrooms_active_tab_' + currentPath;
                localStorage.setItem(activeTabSessionKey, JSON.stringify({ id: tabId, timestamp: Date.now() }));
            } catch(e) {}
        }

        function stopAllMedia(isActualUnload = false) {
            if (isUnloading) return; // Prevent multiple calls
            if (isActualUnload) {
                isUnloading = true;
            }

            // Stop local stream
            if (localStream) {
                localStream.getTracks().forEach(track => {
                    try { 
                        track.enabled = false;
                        track.stop(); 
                    } catch(e) {}
                });
                if (localStream._originalStream) {
                    localStream._originalStream.getTracks().forEach(track => {
                        try { 
                            track.enabled = false;
                            track.stop(); 
                        } catch(e) {}
                    });
                }
                localStream = null;
            }

            // Stop screen stream
            if (screenStream) {
                screenStream.getTracks().forEach(track => {
                    try { 
                        track.enabled = false;
                        track.stop(); 
                    } catch(e) {}
                });
                screenStream = null;
            }

            // Close all peer connections
            if (typeof peers !== 'undefined' && peers) {
                Object.keys(peers).forEach(userId => {
                    try {
                        if (peers[userId]) {
                            peers[userId].getSenders().forEach(sender => {
                                if (sender.track) {
                                    try { 
                                        sender.track.enabled = false;
                                        sender.track.stop(); 
                                    } catch(e) {}
                                }
                            });
                            peers[userId].close();
                        }
                    } catch(e) {}
                });
                peers = {};
            }

            // Close audio context
            if (audioContext) {
                try {
                    audioContext.close().catch(() => {});
                } catch(e) {}
                audioContext = null;
            }

            // Only perform DOM manipulation if we are NOT unloading, as doing so during page tear-down crashes iOS Safari
            if (!isActualUnload) {
                try {
                    const videos = document.querySelectorAll('video');
                    videos.forEach(v => {
                        try {
                            v.pause();
                            v.srcObject = null;
                            v.removeAttribute('src'); // Explicitly remove src
                            v.load();
                        } catch(e) {}
                    });
                } catch(e) {}
            }
        }

        function clearActiveTabSession(isActualUnload = false) {
            stopAllMedia(isActualUnload);
            try {
                if (activeTabSessionKey) {
                    const data = localStorage.getItem(activeTabSessionKey);
                    if (data) {
                        const parsed = JSON.parse(data);
                        if (parsed.id === tabId) {
                            localStorage.removeItem(activeTabSessionKey);
                        }
                    }
                }
            } catch(e) {}
            if (tabHeartbeatInterval) {
                clearInterval(tabHeartbeatInterval);
                tabHeartbeatInterval = null;
            }
        }

        function isAnotherTabActive() {
            try {
                const key = 'rustrooms_active_tab_' + currentPath;
                const data = localStorage.getItem(key);
                if (!data) return false;
                const parsed = JSON.parse(data);
                if (parsed.id === tabId) return false;
                return (Date.now() - parsed.timestamp) < 5000;
            } catch(e) { return false; }
        }

        window.addEventListener('beforeunload', () => clearActiveTabSession(true));
        window.addEventListener('pagehide', () => clearActiveTabSession(true));
        window.addEventListener('unload', () => clearActiveTabSession(true));
        document.addEventListener('visibilitychange', () => {
            if (document.visibilityState === 'hidden') {
                // If we are in the setup screen and not joined yet, 
                // we might want to stop media to be safe if the user switches away/closes
                if (!isConfigured) {
                    // But only if it's not a temporary switch. 
                    // For tab closing, pagehide is usually enough, but visibilitychange 'hidden' is a strong signal.
                }
            }
        });

        let reconnectStatusTimeout = null;
        let reconnectTimer = null;
        let iosSlowRetryTimer = null;
        let wsConnectionId = 0;
        const reconnectDelayMs = 5000;

        let heartbeatInterval = null;
        const heartbeatIntervalMs = isIOS ? 3000 : 2000;
        const heartbeatTimeoutMs = 8000;
        let lastPingSentTime = 0;
        let lastPongTime = Date.now();
        let heartbeatTimeout = null;
        let missedPongCount = 0;

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

            const otgPingContainer = document.getElementById('onTheGoPingContainer');
            const otgPingText = document.getElementById('onTheGoPingText');
            if (otgPingContainer && otgPingText) {
                otgPingContainer.classList.remove('hidden');
                otgPingText.innerText = `${pingMs}ms`;
                otgPingContainer.classList.remove('ping-good', 'ping-fair', 'ping-poor');
                if (pingMs < 100) {
                    otgPingContainer.classList.add('ping-good');
                } else if (pingMs < 250) {
                    otgPingContainer.classList.add('ping-fair');
                } else {
                    otgPingContainer.classList.add('ping-poor');
                }
            }
        }

        let statsWindowVisible = false;
        let statsUpdateInterval = null;
        let prevStatsData = {};
        let prevStatsTimestamp = 0;

        function toggleStatsWindow() {
            const statsWindow = document.getElementById('statsWindow');
            const statusPillWrapper = document.getElementById('statusPillWrapper');
            if (!statsWindow || !statusPillWrapper) return;

            statsWindowVisible = !statsWindowVisible;

            if (statsWindowVisible) {
                const rect = statusPillWrapper.getBoundingClientRect();
                const top = rect.bottom + window.scrollY + 8;
                const right = window.innerWidth - rect.right + window.scrollX;
                statsWindow.style.top = `${top}px`;
                statsWindow.style.right = `${right}px`;
                statsWindow.classList.add('visible');
                prevStatsData = {};
                prevStatsTimestamp = 0;
                startStatsUpdate();
            } else {
                statsWindow.classList.remove('visible');
                stopStatsUpdate();
            }
        }

        document.addEventListener('click', (event) => {
            if (statsWindowVisible) {
                const statsWindow = document.getElementById('statsWindow');
                const statusPillWrapper = document.getElementById('statusPillWrapper');
                if (statsWindow && statusPillWrapper &&
                    !statsWindow.contains(event.target) &&
                    !statusPillWrapper.contains(event.target)) {
                    toggleStatsWindow();
                }
            }
        });

        function startStatsUpdate() {
            if (statsUpdateInterval) return;
            updateWebRTCStats();
            statsUpdateInterval = setInterval(updateWebRTCStats, 2000);
        }

        function stopStatsUpdate() {
            if (statsUpdateInterval) {
                clearInterval(statsUpdateInterval);
                statsUpdateInterval = null;
            }
        }

        function calcBitrateKbps(reportId, currentBytes, nowMs) {
            const prev = prevStatsData[reportId];
            if (!prev || !prev.bytes || !prev.timestamp) {
                return 0;
            }
            const deltaBytes = currentBytes - prev.bytes;
            const deltaSec = (nowMs - prev.timestamp) / 1000;
            if (deltaSec <= 0 || deltaBytes <= 0) return 0;
            return Math.round((deltaBytes * 8) / (deltaSec * 1000));
        }

        async function updateWebRTCStats() {
            const statPing = document.getElementById('statPing');
            const statJitter = document.getElementById('statJitter');
            const statVideoRes = document.getElementById('statVideoRes');
            const statVideoBitrate = document.getElementById('statVideoBitrate');
            const statVideoCodec = document.getElementById('statVideoCodec');
            const statVideoFrames = document.getElementById('statVideoFrames');
            const statAudioBitrate = document.getElementById('statAudioBitrate');
            const statAudioCodec = document.getElementById('statAudioCodec');
            const statPacketsSent = document.getElementById('statPacketsSent');
            const statPacketsReceived = document.getElementById('statPacketsReceived');
            const statPacketsLost = document.getElementById('statPacketsLost');
            const statLowBandwidth = document.getElementById('statLowBandwidth');

            if (statLowBandwidth) {
                if (isLowBandwidthMode) {
                    statLowBandwidth.textContent = 'Enabled';
                    statLowBandwidth.className = 'stats-row-value text-amber-500 font-semibold';
                } else {
                    statLowBandwidth.textContent = 'Disabled';
                    statLowBandwidth.className = 'stats-row-value text-zinc-400 font-normal';
                }
            }

            const pingText = document.getElementById('pingText');
            if (pingText && statPing) {
                statPing.textContent = pingText.textContent;
                statPing.className = 'stat-value ' + (parseInt(pingText.textContent) < 100 ? 'good' : parseInt(pingText.textContent) < 250 ? 'fair' : 'poor');
            }

            let totalPacketsSent = 0;
            let totalPacketsReceived = 0;
            let totalPacketsLost = 0;
            let videoRes = '--';
            let videoBitrate = '--';
            let videoCodec = '--';
            let videoFrames = '--';
            let audioBitrate = '--';
            let audioCodec = '--';
            let jitter = '--';

            const nowMs = Date.now();
            const newStatsData = {};
            const peerValues = Object.values(peers);

            for (const pc of peerValues) {
                try {
                    const stats = await pc.getStats();
                    stats.forEach(report => {
                        if (report.type === 'inbound-rtp' && report.kind === 'video') {
                            const width = report.frameWidth || 0;
                            const height = report.frameHeight || 0;
                            if (width > 0 && height > 0) {
                                videoRes = `${width}x${height}`;
                            }
                            const fps = report.framesPerSecond || 0;
                            if (fps > 0) {
                                videoFrames = `${fps} fps`;
                            }
                            if (report.bytesReceived) {
                                const key = report.id + '_recv';
                                newStatsData[key] = { bytes: report.bytesReceived, timestamp: nowMs };
                                const bitrate = calcBitrateKbps(key, report.bytesReceived, nowMs);
                                if (bitrate > 0) {
                                    videoBitrate = `${bitrate} kbps`;
                                }
                            }
                            totalPacketsReceived += report.packetsReceived || 0;
                            totalPacketsLost += report.packetsLost || 0;
                        } else if (report.type === 'inbound-rtp' && report.kind === 'audio') {
                            if (report.bytesReceived) {
                                const key = report.id + '_recv';
                                newStatsData[key] = { bytes: report.bytesReceived, timestamp: nowMs };
                                const bitrate = calcBitrateKbps(key, report.bytesReceived, nowMs);
                                if (bitrate > 0) {
                                    audioBitrate = `${bitrate} kbps`;
                                }
                            }
                            if (report.jitter && !isNaN(parseFloat(report.jitter))) {
                                jitter = `${Math.round(parseFloat(report.jitter) * 1000)}ms`;
                            }
                            totalPacketsReceived += report.packetsReceived || 0;
                            totalPacketsLost += report.packetsLost || 0;
                        } else if (report.type === 'outbound-rtp' && report.kind === 'video') {
                            const width = report.frameWidth || 0;
                            const height = report.frameHeight || 0;
                            if (width > 0 && height > 0 && videoRes === '--') {
                                videoRes = `${width}x${height}`;
                            }
                            const fps = report.framesPerSecond || 0;
                            if (fps > 0 && videoFrames === '--') {
                                videoFrames = `${fps} fps`;
                            }
                            if (report.bytesSent) {
                                const key = report.id + '_sent';
                                newStatsData[key] = { bytes: report.bytesSent, timestamp: nowMs };
                                const bitrate = calcBitrateKbps(key, report.bytesSent, nowMs);
                                if (bitrate > 0 && videoBitrate === '--') {
                                    videoBitrate = `${bitrate} kbps`;
                                }
                            }
                            totalPacketsSent += report.packetsSent || 0;
                        } else if (report.type === 'outbound-rtp' && report.kind === 'audio') {
                            if (report.bytesSent) {
                                const key = report.id + '_sent';
                                newStatsData[key] = { bytes: report.bytesSent, timestamp: nowMs };
                                const bitrate = calcBitrateKbps(key, report.bytesSent, nowMs);
                                if (bitrate > 0 && audioBitrate === '--') {
                                    audioBitrate = `${bitrate} kbps`;
                                }
                            }
                            totalPacketsSent += report.packetsSent || 0;
                        } else if (report.type === 'codec') {
                            const codecName = report.mimeType || '';
                            if (codecName.includes('video') && videoCodec === '--') {
                                videoCodec = codecName.split('/')[1] || codecName;
                            } else if (codecName.includes('audio') && audioCodec === '--') {
                                audioCodec = codecName.split('/')[1] || codecName;
                            }
                        }
                    });
                } catch (e) {
                    console.warn('Error getting WebRTC stats:', e);
                }
            }

            prevStatsData = newStatsData;
            prevStatsTimestamp = nowMs;

            if (statJitter) statJitter.textContent = jitter;
            if (statVideoRes) statVideoRes.textContent = videoRes;
            if (statVideoBitrate) statVideoBitrate.textContent = videoBitrate;
            if (statVideoCodec) statVideoCodec.textContent = videoCodec;
            if (statVideoFrames) statVideoFrames.textContent = videoFrames;
            if (statAudioBitrate) statAudioBitrate.textContent = audioBitrate;
            if (statAudioCodec) statAudioCodec.textContent = audioCodec;
            if (statPacketsSent) statPacketsSent.textContent = totalPacketsSent.toLocaleString();
            if (statPacketsReceived) statPacketsReceived.textContent = totalPacketsReceived.toLocaleString();
            if (statPacketsLost) statPacketsLost.textContent = totalPacketsLost.toLocaleString();
        }

        let lastVisibilityHidden = 0;

        document.addEventListener('visibilitychange', () => {
            if (document.visibilityState === 'hidden') {
                lastVisibilityHidden = Date.now();
            } else if (document.visibilityState === 'visible') {
                const wasFrozenMs = Date.now() - lastVisibilityHidden;
                if (wasFrozenMs > heartbeatIntervalMs && ws && ws.readyState === WebSocket.OPEN) {
                    console.log(`Tab was hidden for ${Math.round(wasFrozenMs / 1000)}s, restarting heartbeat`);
                    startHeartbeat();
                }
            }
        });

        function escapeHtml(str) {
            if (!str) return '';
            return String(str).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;').replace(/'/g, '&#39;');
        }

        function sendPing() {
            if (ws && ws.readyState === WebSocket.OPEN) {
                lastPingSentTime = Date.now();
                ws.send(JSON.stringify({ type: 'ping' }));

                if (heartbeatTimeout) clearTimeout(heartbeatTimeout);
                heartbeatTimeout = setTimeout(() => {
                    if (document.visibilityState === 'hidden') {
                        return;
                    }
                    const now = Date.now();
                    const timeSincePong = now - lastPongTime;
                    const timeSinceHidden = now - lastVisibilityHidden;
                    if (timeSinceHidden < heartbeatTimeoutMs * 2) {
                        console.log('Heartbeat timeout skipped - tab was recently hidden, restarting heartbeat');
                        missedPongCount = 0;
                        startHeartbeat();
                        return;
                    }
                    if (timeSincePong > heartbeatIntervalMs + heartbeatTimeoutMs) {
                        missedPongCount++;
                        if (missedPongCount < 2) {
                            console.warn(`Heartbeat: missed pong #${missedPongCount}, sending emergency ping before disconnect`);
                            try { ws.send(JSON.stringify({ type: 'ping' })); } catch(e) {}
                            heartbeatTimeout = setTimeout(() => {
                                const recheckPong = Date.now() - lastPongTime;
                                if (recheckPong > heartbeatIntervalMs + heartbeatTimeoutMs) {
                                    console.warn('Heartbeat timeout - emergency ping also failed, closing connection');
                                    missedPongCount = 0;
                                    ws.close();
                                } else {
                                    console.log('Heartbeat recovered after emergency ping');
                                    missedPongCount = 0;
                                }
                            }, isIOS ? 8000 : 5000);
                        } else {
                            console.warn('Heartbeat timeout - no pong received after retries, closing connection');
                            missedPongCount = 0;
                            ws.close();
                        }
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
            missedPongCount = 0;
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

        if (nicknameInput) {
            nicknameInput.addEventListener('input', () => {
                savePreferences();
            });
        }

        async function initAudioWorklet() {
            if (isIOS) return false;
            if (workletLoadingPromise) return workletLoadingPromise;

            if (!audioContext) {
                audioContext = new (window.AudioContext || window.webkitAudioContext)();
            }

            workletLoadingPromise = (async () => {
                try {
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

        async function tryResumeAudioContext(timeoutMs = 300) {
            if (!audioContext) return false;
            if (audioContext.state === 'running') return true;
            if (audioContext.state === 'closed') return false;

            try {
                const resumed = await Promise.race([
                    audioContext.resume().then(() => true).catch(() => false),
                    new Promise(resolve => setTimeout(() => resolve(false), timeoutMs))
                ]);
                return resumed && audioContext.state === 'running';
            } catch (err) {
                console.warn("AudioContext resume failed", err);
                return false;
            }
        }

        let noSleepVideo = null;

        function startNoSleepVideo() {
            if (noSleepVideo) return;
            try {
                noSleepVideo = document.createElement('video');
                noSleepVideo.setAttribute('playsinline', '');
                noSleepVideo.setAttribute('muted', '');
                noSleepVideo.setAttribute('loop', '');
                noSleepVideo.muted = true;
                noSleepVideo.style.position = 'fixed';
                noSleepVideo.style.top = '-1px';
                noSleepVideo.style.left = '-1px';
                noSleepVideo.style.width = '1px';
                noSleepVideo.style.height = '1px';
                noSleepVideo.style.opacity = '0.01';
                noSleepVideo.style.pointerEvents = 'none';
                noSleepVideo.style.zIndex = '-1';
                // Tiny silent MP4 — keeps iOS Safari from throttling/suspending WebSockets
                noSleepVideo.src = 'data:video/mp4;base64,AAAAIGZ0eXBpc29tAAACAGlzb21pc28yYXZjMW1wNDEAAAAIZnJlZQAAA3BtZGF0AAACrwYF//+r3EXpvebZSLeWLNgg2SPu73gyNjQgLSBjb3JlIDE2NCByMzA5NSBiYWVlNDAwIC0gSC4yNjQvTVBFRy00IEFWQyBjb2RlYyAtIENvcHlsZWZ0IDIwMDMtMjAyMiAtIGh0dHA6Ly93d3cudmlkZW9sYW4ub3JnL3gyNjQuaHRtbCAtIG9wdGlvbnM6IGNhYmFjPTEgcmVmPTMgZGVibG9jaz0xOjA6MCBhbmFseXNlPTB4MzoweDExMyBtZT1oZXggc3VibWU9NyBwc3k9MSBwc3lfcmQ9MS4wMDowLjAwIG1peGVkX3JlZj0xIG1lX3JhbmdlPTE2IGNocm9tYV9tZT0xIHRyZWxsaXM9MSA4eDhkY3Q9MSBjcW09MCBkZWFkem9uZT0yMSwxMSBmYXN0X3Bza2lwPTEgY2hyb21hX3FwX29mZnNldD0tMiB0aHJlYWRzPTEgbG9va2FoZWFkX3RocmVhZHM9MSBzbGljZWRfdGhyZWFkcz0wIG5yPTAgZGVjaW1hdGU9MSBpbnRlcmxhY2VkPTAgYmx1cmF5X2NvbXBhdD0wIGNvbnN0cmFpbmVkX2ludHJhPTAgYmZyYW1lcz0zIGJfcHlyYW1pZD0yIGJfYWRhcHQ9MSBiX2JpYXM9MCBkaXJlY3Q9MSB3ZWlnaHRiPTEgb3Blbl9nb3A9MCB3ZWlnaHRwPTIga2V5aW50PTI1MCBrZXlpbnRfbWluPTI1IHNjZW5lY3V0PTQwIGludHJhX3JlZnJlc2g9MCByY19sb29rYWhlYWQ9NDAgcmM9Y3JmIG1idHJlZT0xIGNyZj0yMy4wIHFjb21wPTAuNjAgcXBtaW49MCBxcG1heD02OSBxcHN0ZXA9NCBpcF9yYXRpbz0xLjQwIGFxPTE6MS4wMACAAAAMZWliAAADrfBccwAAAAMAAAMAAAMAIBBgAJQAAAAwAAADAAADAAADAAADAAjUAAADAAADAAADAAADAAADAAADAAADAAADAAADAAADAAADAAAYxgAABwBAAAAGuUGaIAD//vbcvgSuBfAAAAMAAAMAUJgAoEqwEAAAAwAAAwAAAwAADQChIAAAAwAAADAAADAAADAAADAAADAi0AAAAwAAADAAADAAADAAADAAADAAEroAAAAwDMAAABakGaQgwhBAAAAwEC0AAAAwAAAwAA';
                document.body.appendChild(noSleepVideo);
                const playPromise = noSleepVideo.play();
                if (playPromise) playPromise.catch(() => {});
                console.log('NoSleep video started for iOS');
            } catch(e) {
                console.warn('NoSleep video failed:', e);
            }
        }

        function stopNoSleepVideo() {
            if (noSleepVideo) {
                try {
                    noSleepVideo.pause();
                    noSleepVideo.remove();
                } catch(e) {}
                noSleepVideo = null;
            }
        }

        async function requestWakeLock() {
            if (hasLeftRoom) return;
            try {
                if ('wakeLock' in navigator) {
                    if (wakeLock) {
                        try { await wakeLock.release(); } catch(e) {}
                        wakeLock = null;
                    }
                    wakeLock = await navigator.wakeLock.request('screen');
                    wakeLock.addEventListener('release', () => {
                        console.log('Wake Lock released');
                        wakeLock = null;
                    });
                    console.log('Wake Lock active');
                } else if (isIOS) {
                    startNoSleepVideo();
                }
            } catch (err) {
                console.error(`Wake Lock failed: ${err.name}, ${err.message}`);
                if (isIOS) startNoSleepVideo();
            }
        }

        document.addEventListener('visibilitychange', async () => {
            if (document.visibilityState === 'visible') {
                if (!isIOS) {
                    await checkAndRestartLocalStreamIfNeeded();
                }
                if (wakeLock !== null || !hasLeftRoom) {
                    await requestWakeLock();
                }
            }
        });

        ['click', 'touchstart'].forEach(evt => {
            document.addEventListener(evt, () => {
                if (!wakeLock && !hasLeftRoom) {
                    requestWakeLock();
                }
            }, { passive: true });
        });

        async function loadDevices() {
            const btnJoin = document.getElementById('btnJoin');
            const btnCam = document.getElementById('btnCam');

            isCameraReady = false;
            if (btnCam) btnCam.disabled = true;

            loadPreferences();
            try {
                try {
                    const constraints = { audio: true };
                    if (!pendingCamToggle) {
                        constraints.video = true;
                    }
                    const permStream = await navigator.mediaDevices.getUserMedia(constraints);
                    permStream.getTracks().forEach(t => t.stop());
                    if (isUnloading) return;
                } catch (e) {
                    console.warn("Permission request failed", e);
                }

                await populateDeviceList();
                await detectCameras();
                navigator.mediaDevices.ondevicechange = populateDeviceList;

                await startPreview();

            } catch (e) {
                console.warn("Device access initialization failed", e);
                updatePreviewButtons();
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

                detectCameras();

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
                            noiseSuppression: isIOS,
                            autoGainControl: true,                            sampleRate: 48000
                        }
                    };
                     let stream = await navigator.mediaDevices.getUserMedia(constraints);
                     if (isUnloading) {
                         stream.getTracks().forEach(t => t.stop());
                         return;
                     }

                     if (!audioContext) audioContext = new (window.AudioContext || window.webkitAudioContext)();
                     const workletLoaded = await initAudioWorklet();
                     if (audioContext.state === 'suspended') audioContext.resume().catch(e => {});

                     let newTrack;
                     if (workletLoaded) {
                         const source = audioContext.createMediaStreamSource(stream);
                         const worklet = new AudioWorkletNode(audioContext, 'rnnoise-processor');
                         const dest = audioContext.createMediaStreamDestination();
                         source.connect(worklet);
                         worklet.connect(dest);
                         newTrack = dest.stream.getAudioTracks()[0];
                     } else {
                         newTrack = stream.getAudioTracks()[0];
                     }

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
                     if (isUnloading) {
                         newVideoStream.getTracks().forEach(t => t.stop());
                         return;
                     }
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

                      currentVideoInputId = videoId;
                      const newFacingMode = newTrack.getSettings().facingMode;
                      if (newFacingMode) {
                          currentFacingMode = newFacingMode;
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

        let audioMonitorGeneration = {};
        let audioMonitorNodes = {};

        function cleanupAudioMonitor(targetId) {
            if (audioMonitorNodes[targetId]) {
                try { audioMonitorNodes[targetId].source.disconnect(); } catch(e) {}
                try { audioMonitorNodes[targetId].analyser.disconnect(); } catch(e) {}
                delete audioMonitorNodes[targetId];
            }
            if (audioMonitorGeneration[targetId]) {
                audioMonitorGeneration[targetId]++;
            }
        }

        async function setupAudioMonitor(stream, targetId) {
            if (isIOS) return;
            if (!audioContext) return;
            if (!stream.getAudioTracks().length) return;

            const audioReady = await tryResumeAudioContext();
            if (!audioReady) {
                return;
            }

            cleanupAudioMonitor(targetId);

            if (!audioMonitorGeneration[targetId]) audioMonitorGeneration[targetId] = 0;
            audioMonitorGeneration[targetId]++;
            const myGeneration = audioMonitorGeneration[targetId];

            const source = audioContext.createMediaStreamSource(stream);
            const analyser = audioContext.createAnalyser();
            analyser.fftSize = 256;
            source.connect(analyser);

            audioMonitorNodes[targetId] = { source, analyser };

            const bufferLength = analyser.frequencyBinCount;
            const dataArray = new Uint8Array(bufferLength);

            function checkAudio() {
                if (audioMonitorGeneration[targetId] !== myGeneration) {
                    try { source.disconnect(); } catch(e) {}
                    try { analyser.disconnect(); } catch(e) {}
                    return;
                }
                if (targetId !== 'local' && !document.getElementById(targetId)) {
                    cleanupAudioMonitor(targetId);
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
                        activeSpeakers[targetId] = average;

                        if (!gifSpeakingState[targetId]) {
                            gifSpeakingState[targetId] = true;
                            toggleGifAnimation(targetId, true);
                        }

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
                        delete activeSpeakers[targetId];

                        if (gifSpeakingState[targetId]) {
                            gifSpeakingState[targetId] = false;
                            toggleGifAnimation(targetId, false);
                        }

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
                    delete activeSpeakers[targetId];

                    if (targetId !== 'local') {
                        const rawUserId = targetId.startsWith('wrapper-') ? targetId.replace('wrapper-', '') : targetId;
                        const sidebarAvatar = document.querySelector(`.room-user-row[data-user-id="${rawUserId}"] .mini-avatar`);
                        if (sidebarAvatar) sidebarAvatar.classList.remove('speaking-glow');
                    }
                }

                updateOnTheGoSpeakingIndicator();
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
                        userAvatarIsGif = !!data.isGif;
                        userAvatarStaticFrame = null;
                        const displaySrc = userAvatar;
                        if (avatarPreview) {
                            avatarPreview.src = displaySrc;
                            avatarPreview.classList.remove('hidden');
                            avatarPlaceholder.classList.add('hidden');
                            const removeBtn = document.getElementById('btnRemoveSetupAvatar');
                            if (removeBtn) removeBtn.classList.remove('hidden');
                        }
                        if (document.getElementById('settingsAvatarPreview')) {
                            const sap = document.getElementById('settingsAvatarPreview');
                            sap.src = displaySrc;
                            sap.classList.remove('hidden');
                            document.getElementById('settingsAvatarPlaceholder').classList.add('hidden');
                        }
                        if (userAvatarIsGif) {
                            extractGifFirstFrame(userAvatar).then(sf => {
                                userAvatarStaticFrame = sf;
                                if (avatarPreview) avatarPreview.src = sf;
                                if (document.getElementById('settingsAvatarPreview')) {
                                    document.getElementById('settingsAvatarPreview').src = sf;
                                }
                            });
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
                    if (data.facingMode) {
                        currentFacingMode = data.facingMode;
                    }
                    if (data.isLowBandwidthMode !== undefined) {
                        isLowBandwidthMode = data.isLowBandwidthMode;
                        const setupLBM = document.getElementById('setupLowBandwidth');
                        const settingsLBM = document.getElementById('settingsLowBandwidth');
                        if (setupLBM) setupLBM.checked = isLowBandwidthMode;
                        if (settingsLBM) settingsLBM.checked = isLowBandwidthMode;
                        updateLowBandwidthBadgeVisibility();
                    }
                    if (data.isOnTheGoMode !== undefined) {
                        isOnTheGoMode = data.isOnTheGoMode;
                        const setupOtg = document.getElementById('setupOnTheGo');
                        const settingsOtg = document.getElementById('settingsOnTheGo');
                        if (setupOtg) setupOtg.checked = isOnTheGoMode;
                        if (settingsOtg) settingsOtg.checked = isOnTheGoMode;
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
                const sNickname = document.getElementById('settingsNicknameInput');
                if (sAudio && sAudio.value !== undefined) audioInputId = sAudio.value;
                if (sVideo && sVideo.value !== undefined) videoInputId = sVideo.value;
                if (sAudioOut && sAudioOut.value !== undefined) audioOutputId = sAudioOut.value;
                if (sNickname) userNickname = sNickname.value.trim() || "Guest";
            } else if (isConfigOpen) {
                if (audioSelect) audioInputId = audioSelect.value;
                if (videoSelect) videoInputId = videoSelect.value;
                if (audioOutputSelect) audioOutputId = audioOutputSelect.value;
                const cNickname = document.getElementById('nicknameInput');
                if (cNickname) userNickname = cNickname.value.trim() || "Guest";
            }

            let isMuted = pendingMicToggle;
            let isCamOff = pendingCamToggle;

            if (localStream) {
                const audioTrack = localStream.getAudioTracks()[0];
                const videoTrack = localStream.getVideoTracks()[0];
                if (audioTrack) {
                    isMuted = !audioTrack.enabled;
                } else {
                    isMuted = true;
                }
                if (videoTrack) {
                    isCamOff = !videoTrack.enabled;
                } else {
                    isCamOff = true;
                }
            }

            try {
                localStorage.setItem('rustrooms_profile', JSON.stringify({
                    nickname: userNickname,
                    avatar: userAvatar,
                    isGif: userAvatarIsGif,
                    audioOutputId: audioOutputId,
                    audioInputId: audioInputId,
                    videoInputId: videoInputId,
                    isMuted: isMuted,
                    isCamOff: isCamOff,
                    isDeafened: isDeafened,
                    facingMode: currentFacingMode,
                    isLowBandwidthMode: isLowBandwidthMode,
                    isOnTheGoMode: isOnTheGoMode
                }));
            } catch(e) {
                console.warn('Could not save preferences to localStorage:', e.message);
            }

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
            const audioReady = await tryResumeAudioContext();
            if (!audioReady) {
                bar.style.width = '0%';
                return;
            }

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

        function resizeImageForAvatar(file) {
            return new Promise((resolve) => {
                const reader = new FileReader();
                reader.onload = function(e) {
                    const img = new Image();
                    img.onload = function() {
                        const MAX_DIM = 1200;
                        let w = img.naturalWidth;
                        let h = img.naturalHeight;
                        if (w > MAX_DIM || h > MAX_DIM) {
                            if (w > h) { h = Math.round(h * MAX_DIM / w); w = MAX_DIM; }
                            else { w = Math.round(w * MAX_DIM / h); h = MAX_DIM; }
                        }
                        const canvas = document.createElement('canvas');
                        canvas.width = w;
                        canvas.height = h;
                        const ctx = canvas.getContext('2d');
                        ctx.drawImage(img, 0, 0, w, h);
                        resolve(canvas.toDataURL('image/jpeg', 0.8));
                    };
                    img.onerror = function() { resolve(e.target.result); };
                    img.src = e.target.result;
                };
                reader.readAsDataURL(file);
            });
        }

        function handleAvatarUpload(input) {
            const file = input.files[0];
            if (!file) return;

            if (file.type === 'image/gif') {
                const reader = new FileReader();
                reader.onload = function(e) {
                    const gifDataUrl = e.target.result;
                    userAvatar = gifDataUrl;
                    userAvatarIsGif = true;
                    extractGifFirstFrame(gifDataUrl).then(staticFrame => {
                        userAvatarStaticFrame = staticFrame;
                        avatarPreview.src = staticFrame;
                        avatarPreview.classList.remove('hidden');
                        avatarPlaceholder.classList.add('hidden');
                        const removeBtn = document.getElementById('btnRemoveSetupAvatar');
                        if (removeBtn) removeBtn.classList.remove('hidden');
                        savePreferences();
                    });
                };
                reader.readAsDataURL(file);
            } else {
                resizeImageForAvatar(file).then(dataUrl => {
                    openCropModal(dataUrl, 'setup');
                });
            }
            input.value = '';
        }

        function removeSetupAvatar() {
            userAvatar = null;
            userAvatarIsGif = false;
            userAvatarStaticFrame = null;
            avatarPreview.src = '';
            avatarPreview.classList.add('hidden');
            avatarPlaceholder.classList.remove('hidden');
            const removeBtn = document.getElementById('btnRemoveSetupAvatar');
            if (removeBtn) removeBtn.classList.add('hidden');
            savePreferences();
        }

        function removeSettingsAvatar() {
            newAvatarCandidate = null;
            newAvatarCandidateIsGif = false;
            newAvatarCandidateStaticFrame = null;
            settingsAvatarPreview.src = '';
            settingsAvatarPreview.classList.add('hidden');
            settingsAvatarPlaceholder.classList.remove('hidden');
            const removeBtn = document.getElementById('btnRemoveSettingsAvatar');
            if (removeBtn) removeBtn.classList.add('hidden');
            saveSettings();
        }

        function extractGifFirstFrame(gifDataUrl) {
            return new Promise((resolve) => {
                const img = new Image();
                img.onload = function() {
                    const canvas = document.createElement('canvas');
                    canvas.width = img.naturalWidth;
                    canvas.height = img.naturalHeight;
                    const ctx = canvas.getContext('2d');
                    ctx.drawImage(img, 0, 0);
                    resolve(canvas.toDataURL('image/png'));
                };
                img.onerror = function() {
                    resolve(gifDataUrl);
                };
                img.src = gifDataUrl;
            });
        }

        function restartGif(url) {
            if (isIOS) return url;
            return url.split('#')[0] + '#' + Date.now();
        }

        let gifSpeakingState = {};

        function toggleGifAnimation(targetId, isSpeaking) {
            if (targetId === 'local') {
                if (!userAvatarIsGif || !userAvatar) return;
                const centerImg = document.getElementById('localAvatarCenterImg');
                const bgImg = document.getElementById('localAvatarImg');
                const sidebarImg = document.querySelector(`.room-user-row[data-user-id="${persistentUserId}"] .mini-avatar img`);
                const staticSrc = userAvatarStaticFrame || userAvatar;
                if (isSpeaking) {
                    const animSrc = restartGif(userAvatar);
                    if (centerImg) centerImg.src = animSrc;
                    if (bgImg) bgImg.src = animSrc;
                    if (sidebarImg) sidebarImg.src = animSrc;
                } else {
                    if (centerImg) centerImg.src = staticSrc;
                    if (bgImg) bgImg.src = staticSrc;
                    if (sidebarImg) sidebarImg.src = staticSrc;
                }
            } else {
                const rawUserId = targetId.startsWith('wrapper-') ? targetId.replace('wrapper-', '') : targetId;
                const wrapper = document.getElementById(targetId);
                if (!wrapper) return;
                const avatarCenter = wrapper.querySelector('.avatar-center');
                if (!avatarCenter) return;
                const imgs = avatarCenter.querySelectorAll('img');
                imgs.forEach(img => {
                    const gifSrc = img.dataset.gifSrc;
                    const staticSrc = img.dataset.staticSrc;
                    if (gifSrc && staticSrc) {
                        img.src = isSpeaking ? restartGif(gifSrc) : staticSrc;
                    }
                });
                const bgImg = wrapper.querySelector('.avatar-img');
                if (bgImg && bgImg.dataset.gifSrc && bgImg.dataset.staticSrc) {
                    bgImg.src = isSpeaking ? restartGif(bgImg.dataset.gifSrc) : bgImg.dataset.staticSrc;
                }
                const sidebarImg = document.querySelector(`.room-user-row[data-user-id="${rawUserId}"] .mini-avatar img`);
                if (sidebarImg && sidebarImg.dataset.gifSrc && sidebarImg.dataset.staticSrc) {
                    sidebarImg.src = isSpeaking ? restartGif(sidebarImg.dataset.gifSrc) : sidebarImg.dataset.staticSrc;
                }
            }
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

            savePreferences();

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

                const shouldGetVideo = !pendingCamToggle;

                let videoConstraints = false;
                if (shouldGetVideo) {
                    if (videoSource) {
                        videoConstraints = { deviceId: { exact: videoSource } };
                    } else {
                        videoConstraints = { facingMode: currentFacingMode };
                    }
                }

                const constraints = {
                    audio: {
                        deviceId: audioSource ? { exact: audioSource } : undefined,
                        echoCancellation: true,
                        noiseSuppression: isIOS,
                        autoGainControl: true,                        sampleRate: 48000
                    },
                    video: videoConstraints
                };

                let rawStream = await navigator.mediaDevices.getUserMedia(constraints);
                if (isUnloading) {
                    rawStream.getTracks().forEach(t => t.stop());
                    return;
                }

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

                 if (rawStream.getAudioTracks().length > 0) {
                     if (!audioContext) audioContext = new (window.AudioContext || window.webkitAudioContext)();
                     const audioReady = await tryResumeAudioContext();
                     const workletLoaded = audioReady ? await initAudioWorklet() : false;

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

                await setupVolumeMeter(localStream, 'setupMicBar');

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
                    let rawStream = await navigator.mediaDevices.getUserMedia({ 
                        audio: {
                            echoCancellation: true,
                            noiseSuppression: isIOS,
                            autoGainControl: true,                        }, 
                        video: false
                    });
                    if (isUnloading) {
                        rawStream.getTracks().forEach(t => t.stop());
                        return;
                    }

                    const newA = rawStream.getAudioTracks()[0];
                    if (newA) newA.enabled = previousAudioEnabled;

                    if (rawStream.getAudioTracks().length > 0) {
                         if (!audioContext) audioContext = new (window.AudioContext || window.webkitAudioContext)();
                         const audioReady = await tryResumeAudioContext();
                         const workletLoaded = audioReady ? await initAudioWorklet() : false;

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
                        pendingCamToggle = false;
                    }
                    if (pendingMicToggle) {
                        const audioTrack = localStream.getAudioTracks()[0];
                        if (audioTrack && audioTrack.enabled) {
                            audioTrack.enabled = false;
                            needsUpdate = true;
                        }
                        pendingMicToggle = false;
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
                     btnMic.classList.add('active-red');
                     btnMic.innerText = "Unmute";
                 } else {
                     btnMic.classList.remove('active-red');
                     btnMic.innerText = "Mute";
                 }
             }

             if (!videoTrack) {

                 if (!isPreviewStarting) {
                     btnCam.disabled = false;
                     btnCam.classList.remove('opacity-50', 'cursor-not-allowed');
                 }
                 btnCam.classList.add('active-red');
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
                     btnCam.classList.add('active-red');
                     btnCam.innerText = "Start Cam";
                     document.getElementById('previewPlaceholder').style.display = 'flex';
                 } else {
                     btnCam.classList.remove('active-red');
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
                        btnMic.classList.add('active-red');
                        btnMic.innerText = "Unmute";
                    } else {
                        btnMic.classList.remove('active-red');
                        btnMic.innerText = "Mute";
                    }
                    btnMic.blur();
                 }
                 savePreferences();
                 return;
             }
             if (!localStream) return;
            const track = localStream.getAudioTracks()[0];
            if (track) {
                track.enabled = !track.enabled;
                if (track.enabled && isDeafened) {
                    isDeafened = false;
                }
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
                        btnCam.classList.add('active-red');
                        btnCam.innerText = "Start Cam";
                        document.getElementById('previewPlaceholder').style.display = 'flex';
                    } else {
                        btnCam.classList.remove('active-red');
                        btnCam.innerText = "Stop Cam";
                        document.getElementById('previewPlaceholder').style.display = 'none';
                    }
                    btnCam.blur();
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
                         if (isUnloading) {
                             newStream.getTracks().forEach(t => t.stop());
                             return;
                         }
                         const newTrack = newStream.getVideoTracks()[0];

                         if (!newTrack || newTrack.readyState !== 'live') {
                             console.warn("Camera track not properly initialized, retrying...");
                             newTrack?.stop();
                             if (newTrack && localStream.getVideoTracks().includes(newTrack)) {
                                 localStream.removeTrack(newTrack);
                             }
                             await new Promise(r => setTimeout(r, 100));
                             const retryStream = await navigator.mediaDevices.getUserMedia(constraints);
                             if (isUnloading) {
                                 retryStream.getTracks().forEach(t => t.stop());
                                 return;
                             }
                             const retryTrack = retryStream.getVideoTracks()[0];                             if (retryTrack) {
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

        async function checkAndRestartLocalStreamIfNeeded() {
            if (hasLeftRoom) return;
            const needsRestart = !localStream || localStream.getTracks().some(track => track.readyState === 'ended');
            if (needsRestart) {
                console.log("Local stream tracks are ended/missing. Re-acquiring media...");
                try {
                    await startPreview();
                } catch(e) {
                    console.error("Failed to restart local stream on wakeup:", e);
                }
            }
        }

        async function joinRoom() {

            hasLeftRoom = false;

            if (isAnotherTabActive()) {
                document.getElementById('alertTitle').innerText = 'Already In Call';
                document.getElementById('alertMessage').innerText = 'You already have an active call open in another tab for this room. Please close it first.';

                const alertBtn = document.querySelector('#alertModal button');
                const oldOnClick = alertBtn.onclick;

                alertBtn.onclick = function() {
                    closeCustomAlert();
                    alertBtn.onclick = oldOnClick;
                    sessionStorage.setItem('rustrooms_welcomed', 'false');
                    sessionStorage.setItem('rustrooms_setup_done', 'false');
                    stopAllMedia(false);
                    roomId = '';
                    channelId = '';
                    history.replaceState(null, '', '/');
                    document.getElementById('welcomeOverlay').style.display = 'flex';
                    document.querySelector('main').style.display = 'none';
                    document.querySelector('.taskbar').style.display = 'none';
                    const configOverlay = document.getElementById('configOverlay');
                    if (configOverlay) {
                        configOverlay.classList.add('hidden', 'opacity-0');
                    }
                };

                document.getElementById('alertModal').classList.add('open');
                return;
            }

            proceedJoinRoom();
        }

        async function proceedJoinRoom() {
            userNickname = nicknameInput.value.trim() || "Guest";
            const setupDone = sessionStorage.getItem('rustrooms_setup_done') === 'true';
            if (!setupDone) {
                isDeafened = false;
            }
            savePreferences();

            setActiveTabSession();
            tabHeartbeatInterval = setInterval(setActiveTabSession, 2000);

            if (!audioContext) {
                audioContext = new (window.AudioContext || window.webkitAudioContext)();
            }
            await tryResumeAudioContext(2000);
            await initAudioWorklet();

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

                if (btnMic) {
                    btnMic.disabled = true;
                }

                document.querySelectorAll('video, audio').forEach(el => {
                    if (el.id !== 'localVideo' && el.id !== 'previewVideo') {
                        el.muted = true;
                    }
                });
            }

            connectWs();

            sessionStorage.setItem('rustrooms_setup_done', 'true');

            if (isOnTheGoMode) {
                toggleOnTheGoMode(true, true);
            }

            await requestWakeLock();
        }

        // Global event listeners to handle network and lifecycle events
        window.addEventListener('offline', () => {
            if (hasLeftRoom) return;
            console.warn('Network connection lost (offline)');
            updateStatus('disconnected', 'Network Offline');
            updateConnectionStatus();
        });

        window.addEventListener('online', () => {
            if (hasLeftRoom) return;

            if (isReconnecting) {
                console.log('Already reconnecting, skipping network restore trigger');
                return;
            }

            console.log('Network connection restored (online)');
            updateStatus('connecting', 'Reconnecting...');

            reconnectionAttempts = 0;
            connectWs();
        });

        if (isIOS) {
            document.addEventListener('visibilitychange', async () => {
                if (document.visibilityState === 'visible' && !hasLeftRoom) {
                    stopHeartbeat();
                    
                    // Restart media tracks if they were ended by iOS during lock/background
                    await checkAndRestartLocalStreamIfNeeded();

                    if (!ws || ws.readyState === WebSocket.CLOSED || ws.readyState === WebSocket.CLOSING) {
                        console.log('iOS returned from background, WebSocket dead, reconnecting...');
                        isReconnecting = false;
                        reconnectionAttempts = 0;
                        connectWs();
                    } else if (ws.readyState === WebSocket.OPEN) {
                        startHeartbeat();

                        let hasDeadPeer = false;
                        for (const uid in peers) {
                            const peerState = peers[uid].connectionState || peers[uid].iceConnectionState;
                            if (peerState === 'disconnected' || peerState === 'failed' || peerState === 'closed') {
                                hasDeadPeer = true;
                                break;
                            }
                        }
                        if (hasDeadPeer) {
                            console.log('iOS returned from background, dead peers detected, re-establishing...');
                            for (const uid in peers) {
                                removePeer(uid);
                            }
                            peerCamStatus = {};
                            peerScreenStatus = {};
                            peerScreenHasAudio = {};
                            isReconnecting = false;
                            reconnectionAttempts = 0;
                            connectWs();
                        }
                    }
                    if (audioContext && audioContext.state === 'suspended') {
                        audioContext.resume().catch(e => {});
                    }
                }
            });

            // iOS WebSocket watchdog — catches silent WS deaths that don't trigger onclose
            setInterval(() => {
                if (hasLeftRoom) return;
                const now = Date.now();
                const pongAge = now - lastPongTime;
                // If we haven't received a pong in 3x the heartbeat interval, WS is probably dead
                const watchdogThreshold = heartbeatIntervalMs * 3 + heartbeatTimeoutMs;
                if (ws && ws.readyState === WebSocket.OPEN && pongAge > watchdogThreshold) {
                    console.warn(`iOS watchdog: no pong in ${Math.round(pongAge/1000)}s, force-reconnecting`);
                    missedPongCount = 0;
                    ws.close();
                } else if (!ws || ws.readyState === WebSocket.CLOSED || ws.readyState === WebSocket.CLOSING) {
                    if (!isReconnecting && !hasLeftRoom) {
                        console.warn('iOS watchdog: WebSocket is dead and no reconnection in progress, reconnecting...');
                        reconnectionAttempts = 0;
                        isReconnecting = false;
                        connectWs();
                    }
                }
            }, 30000);
        }

        // Handle iOS BFCache restoration (back-forward cache)
        window.addEventListener('pageshow', (event) => {
            if (event.persisted && !hasLeftRoom) {
                console.log('Page restored from BFCache, checking WebSocket...');
                if (!ws || ws.readyState !== WebSocket.OPEN) {
                    isReconnecting = false;
                    reconnectionAttempts = 0;
                    connectWs();
                } else {
                    startHeartbeat();
                }
            }
        });

        const welcomeOverlay = document.getElementById('welcomeOverlay');

        function playNotificationSound(type) {
            if (!audioContext || audioContext.state === 'closed') {
                try {
                    audioContext = new (window.AudioContext || window.webkitAudioContext)();
                } catch (e) {
                    console.warn("Failed to create AudioContext:", e);
                    return;
                }
            }
            if (audioContext.state === 'suspended') {
                audioContext.resume().catch(e => console.warn("Failed to resume AudioContext:", e));
            }

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
            } else if (type === 'bandwidth_on') {
                 osc.type = 'sine';
                 osc.frequency.setValueAtTime(400, now);
                 osc.frequency.exponentialRampToValueAtTime(600, now + 0.08);

                 gain.gain.setValueAtTime(0.08, now);
                 gain.gain.exponentialRampToValueAtTime(0.001, now + 0.08);

                 const osc2 = audioContext.createOscillator();
                 const gain2 = audioContext.createGain();
                 osc2.connect(gain2);
                 gain2.connect(audioContext.destination);
                 osc2.type = 'sine';
                 osc2.frequency.setValueAtTime(600, now + 0.1);
                 osc2.frequency.exponentialRampToValueAtTime(800, now + 0.18);

                 gain2.gain.setValueAtTime(0.08, now + 0.1);
                 gain2.gain.exponentialRampToValueAtTime(0.001, now + 0.18);

                 osc.start(now);
                 osc.stop(now + 0.08);
                 osc2.start(now + 0.1);
                 osc2.stop(now + 0.18);
            } else if (type === 'bandwidth_off') {
                 osc.type = 'sine';
                 osc.frequency.setValueAtTime(800, now);
                 osc.frequency.exponentialRampToValueAtTime(600, now + 0.08);

                 gain.gain.setValueAtTime(0.08, now);
                 gain.gain.exponentialRampToValueAtTime(0.001, now + 0.08);

                 const osc2 = audioContext.createOscillator();
                 const gain2 = audioContext.createGain();
                 osc2.connect(gain2);
                 gain2.connect(audioContext.destination);
                 osc2.type = 'sine';
                 osc2.frequency.setValueAtTime(600, now + 0.1);
                 osc2.frequency.exponentialRampToValueAtTime(400, now + 0.18);

                 gain2.gain.setValueAtTime(0.08, now + 0.1);
                 gain2.gain.exponentialRampToValueAtTime(0.001, now + 0.18);

                 osc.start(now);
                 osc.stop(now + 0.08);
                 osc2.start(now + 0.1);
                 osc2.stop(now + 0.18);
            }
        }

        function updateStatus(state, message) {
            statusText.innerText = message;
            connectionDot.className = 'connection-dot ' + state;

            const otgDot = document.getElementById('onTheGoConnectionDot');
            const otgText = document.getElementById('onTheGoStatusText');
            if (otgText) otgText.innerText = message;
            if (otgDot) {
                otgDot.className = 'connection-dot ' + state;
            }
        }

        function showReconnectButtons() {
            ['btnReconnect', 'onTheGoBtnReconnect'].forEach(id => {
                const btn = document.getElementById(id);
                if (btn) btn.classList.remove('hidden');
            });
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

            const isOpen = !wasOpen;

            if (isOpen) {
                sidebar.classList.add('open');
                overlay.classList.add('open');
                body.classList.add('sidebar-open');
                sidebarToggle.classList.add('hidden');

                if (isDesktop) {
                    const pip = document.getElementById('localPipWrapper');
                    if (pip) {
                        const pipRect = pip.getBoundingClientRect();
                        const sidebarWidth = 340;
                        const margin = 24;

                        if (pipRect.left < sidebarWidth + margin) {
                            const newLeft = sidebarWidth + margin;
                            pip.style.left = newLeft + 'px';
                            pip.style.bottom = '';
                            pip.style.right = '';
                        }
                    }
                }
            } else {
                sidebar.classList.remove('open');
                overlay.classList.remove('open');
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
                            const sidebarWidth = 340;
                            const margin = 24;

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
                } else {
                    overlay.classList.remove('open');
                    body.classList.remove('sidebar-open');
                    sidebarToggle.classList.remove('hidden');
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
                        const sidebarWidth = 340;
                        const margin = 24;

                        if (pipRect.left < sidebarWidth + margin) {
                            const newLeft = sidebarWidth + margin;
                            pip.style.left = newLeft + 'px';
                            pip.style.bottom = '';
                            pip.style.right = '';
                        }
                    }
                } else {
                    overlay.classList.add('open');
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

        let _uvmLongPressTimer = null;
        let _uvmTouchMoved = false;

        function handleUserContextMenu(e, el) {
            e.preventDefault();
            e.stopPropagation();
            const userId = el.dataset.userId;
            const nickname = el.dataset.userNickname;
            if (!userId || userId === persistentUserId) return;
            showUserVolumeMenu(userId, nickname, e.clientX, e.clientY);
        }

        function handleUserTouchStart(e, el) {
            _uvmTouchMoved = false;
            const touch = e.touches[0];
            const tx = touch.clientX;
            const ty = touch.clientY;
            _uvmLongPressTimer = setTimeout(() => {
                if (_uvmTouchMoved) return;
                const userId = el.dataset.userId;
                const nickname = el.dataset.userNickname;
                if (!userId || userId === persistentUserId) return;
                e.preventDefault();
                showUserVolumeMenu(userId, nickname, tx, ty);
            }, 500);
        }

        function handleUserTouchEnd(e) {
            if (_uvmLongPressTimer) {
                clearTimeout(_uvmLongPressTimer);
                _uvmLongPressTimer = null;
            }
        }

        function handleUserTouchCancel() {
            _uvmTouchMoved = true;
            if (_uvmLongPressTimer) {
                clearTimeout(_uvmLongPressTimer);
                _uvmLongPressTimer = null;
            }
        }

        function showUserVolumeMenu(userId, nickname, x, y) {
            const menu = document.getElementById('userVolumeMenu');
            if (!menu) return;

            const mainVol = getVolumeSettings(userId, 'main');
            const hasScreen = !!peerScreenHasAudio[userId];
            const screenVol = hasScreen ? getVolumeSettings(userId, 'screen') : 1.0;

            const vidEl = document.getElementById(`vid-${userId}`);
            const mainMuted = vidEl ? vidEl.muted : false;
            let screenMuted = false;
            if (hasScreen) {
                const screenAud = document.getElementById(`aud-screen-${userId}`);
                screenMuted = screenAud ? screenAud.muted : false;
            }

            const volSvgOn = `<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5"></polygon><path d="M19.07 4.93a10 10 0 0 1 0 14.14M15.54 8.46a5 5 0 0 1 0 7.07"></path></svg>`;
            const volSvgOff = `<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5"></polygon><line x1="23" y1="9" x2="17" y2="15"></line><line x1="17" y1="9" x2="23" y2="15"></line></svg>`;
            const screenSvgOn = `<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="4" y="2" width="16" height="14" rx="2" ry="2"></rect><line x1="12" y1="22" x2="12" y2="16"></line><path d="M5 12h14"></path><path d="M12 12v4"></path></svg>`;
            const screenSvgOff = `<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="4" y="2" width="16" height="14" rx="2" ry="2"></rect><line x1="23" y1="9" x2="17" y2="15"></line><line x1="17" y1="9" x2="23" y2="15"></line></svg>`;

            let html = `
                <div class="uvm-header">
                    <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="var(--text-muted)" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2"></path><circle cx="12" cy="7" r="4"></circle></svg>
                    <span class="uvm-name">${escapeHtml(nickname)}</span>
                    <button class="uvm-close" onclick="closeUserVolumeMenu()">
                        <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="18" y1="6" x2="6" y2="18"></line><line x1="6" y1="6" x2="18" y2="18"></line></svg>
                    </button>
                </div>
                <div class="uvm-section">
                    <div class="uvm-label">Mic Volume</div>
                    <div class="uvm-slider-row">
                        <button id="uvm-mute-main" class="${mainMuted ? 'muted' : ''}" onclick="uvmToggleMute('${userId}', 'main')">
                            ${mainMuted ? volSvgOff : volSvgOn}
                        </button>
                        <input type="range" min="0" max="1" step="0.05" value="${mainVol}" id="uvm-slider-main" oninput="uvmSetVolume('${userId}', 'main', this.value)">
                        <span class="uvm-vol-pct" id="uvm-pct-main">${Math.round(mainVol * 100)}%</span>
                    </div>
                </div>
            `;

            if (hasScreen) {
                html += `
                <div class="uvm-section">
                    <div class="uvm-label">Screen Volume</div>
                    <div class="uvm-slider-row">
                        <button id="uvm-mute-screen" class="${screenMuted ? 'muted' : ''}" onclick="uvmToggleMute('${userId}', 'screen')">
                            ${screenMuted ? screenSvgOff : screenSvgOn}
                        </button>
                        <input type="range" min="0" max="1" step="0.05" value="${screenVol}" id="uvm-slider-screen" oninput="uvmSetVolume('${userId}', 'screen', this.value)">
                        <span class="uvm-vol-pct" id="uvm-pct-screen">${Math.round(screenVol * 100)}%</span>
                    </div>
                </div>
                `;
            }

            menu.innerHTML = html;
            menu.dataset.userId = userId;

            menu.style.left = '0px';
            menu.style.top = '0px';
            menu.classList.add('open');

            requestAnimationFrame(() => {
                const mw = menu.offsetWidth;
                const mh = menu.offsetHeight;
                const vw = window.innerWidth;
                const vh = window.innerHeight;
                let left = x;
                let top = y;
                if (left + mw > vw - 8) left = vw - mw - 8;
                if (left < 8) left = 8;
                if (top + mh > vh - 8) top = vh - mh - 8;
                if (top < 8) top = 8;
                menu.style.left = left + 'px';
                menu.style.top = top + 'px';
            });
        }

        function closeUserVolumeMenu() {
            const menu = document.getElementById('userVolumeMenu');
            if (menu) {
                menu.classList.remove('open');
                menu.dataset.userId = '';
            }
        }

        window.uvmToggleMute = function(userId, type) {
            toggleMute(userId, type);

            let el;
            if (type === 'screen') {
                el = document.getElementById(`aud-screen-${userId}`);
                if (!el) el = document.getElementById(`vid-${userId}`);
            } else {
                el = document.getElementById(`vid-${userId}`);
            }
            const isMuted = el ? el.muted : false;

            const btn = document.getElementById(`uvm-mute-${type}`);
            if (btn) {
                if (type === 'screen') {
                    const screenSvgOn = `<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="4" y="2" width="16" height="14" rx="2" ry="2"></rect><line x1="12" y1="22" x2="12" y2="16"></line><path d="M5 12h14"></path><path d="M12 12v4"></path></svg>`;
                    const screenSvgOff = `<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="4" y="2" width="16" height="14" rx="2" ry="2"></rect><line x1="23" y1="9" x2="17" y2="15"></line><line x1="17" y1="9" x2="23" y2="15"></line></svg>`;
                    btn.innerHTML = isMuted ? screenSvgOff : screenSvgOn;
                } else {
                    const volSvgOn = `<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5"></polygon><path d="M19.07 4.93a10 10 0 0 1 0 14.14M15.54 8.46a5 5 0 0 1 0 7.07"></path></svg>`;
                    const volSvgOff = `<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5"></polygon><line x1="23" y1="9" x2="17" y2="15"></line><line x1="17" y1="9" x2="23" y2="15"></line></svg>`;
                    btn.innerHTML = isMuted ? volSvgOff : volSvgOn;
                }
                btn.classList.toggle('muted', isMuted);
            }
        };

        window.uvmSetVolume = function(userId, type, val) {
            setVolume(userId, type, val);

            const pct = document.getElementById(`uvm-pct-${type}`);
            if (pct) pct.textContent = Math.round(val * 100) + '%';

            const overlaySlider = document.querySelector(`#vol-row-${type}-${userId} input[type=range]`);
            if (overlaySlider) overlaySlider.value = val;
        };

        document.addEventListener('mousedown', function(e) {
            const menu = document.getElementById('userVolumeMenu');
            if (menu && menu.classList.contains('open') && !menu.contains(e.target)) {
                closeUserVolumeMenu();
            }
        });

        document.addEventListener('keydown', function(e) {
            if (e.key === 'Escape') {
                closeUserVolumeMenu();
            }
        });

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
                window.location.href = `/${name ? encodeURIComponent(name) : crypto.randomUUID()}`;
            });
        }

        async function createNewChannel() {
            showNameModal("Create New Channel", "Enter channel name", (name) => {
                if (!name) return;
                performChannelSwitch(roomId, name);
            });
        }

        async function performChannelSwitch(newRoomId, newChannelId) {
            if (newChannelId && newChannelId.toLowerCase() === 'general') {
                newChannelId = 'General';
            }
            if (newChannelId && newChannelId.length > 32) newChannelId = newChannelId.substring(0, 32);

            if (ws) {
                ws.onclose = null;
                ws.onerror = null;
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

            const newUrl = `/${roomId}${channelId && channelId.toLowerCase() !== 'general' ? '/' + encodeURIComponent(channelId) : ''}`;
            if (window.location.pathname !== newUrl) {
                history.pushState({ roomId, channelId }, "", newUrl);
            }

            wsUrl = `${wsProtocol}//${window.location.host}/ws/${roomId}/${encodeURIComponent(channelId)}`;
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
            const newChannelId = decodeURIComponent(parts[1] || '') || (newRoomId ? 'General' : '');

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
                const normalizedNewName = newName.toLowerCase() === 'general' ? 'General' : newName;
                if (globalRoomList[normalizedNewName]) {
                    showCustomAlert("Channel Exists", `A channel named "${normalizedNewName}" already exists.`);
                    return;
                }
                if (ws && ws.readyState === WebSocket.OPEN) {
                    ws.send(JSON.stringify({
                        type: 'rename-channel',
                        data: { channelId: targetRoomId, newName: normalizedNewName }
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
                        <div class="room-user-row pointer-events-auto cursor-pointer" data-user-id="${uid}" data-user-nickname="${escapeHtml(u.nickname)}" onclick="handleUserClick(this)" oncontextmenu="handleUserContextMenu(event, this)" ontouchstart="handleUserTouchStart(event, this)" ontouchend="handleUserTouchEnd(event)" ontouchmove="handleUserTouchCancel()">
                            <div class="mini-avatar">
                                ${u.avatar ? (u.isGif && u.staticFrame ? `<img src="${escapeHtml(u.staticFrame)}" data-gif-src="${escapeHtml(u.avatar)}" data-static-src="${escapeHtml(u.staticFrame)}">` : `<img src="${escapeHtml(u.avatar)}">`) : `<div class="mini-avatar-placeholder">${escapeHtml(u.nickname.charAt(0).toUpperCase())}</div>`}
                            </div>
                            <span class="room-user-name">${escapeHtml(u.nickname)}</span>
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
                                ${u.isLowBandwidthMode ? `
                                    <div class="status-icon active animate-pulse" style="color: #f59e0b;" title="Low Bandwidth Mode Active">
                                        <svg class="w-3.5 h-3.5" fill="currentColor" viewBox="0 0 24 24"><path d="M13 10V3L4 14h7v7l9-11h-7z" /></svg>
                                    </div>
                                ` : ''}
                                ${u.isOnTheGoMode ? `
                                    <div class="status-icon active" style="color: #60a5fa;" title="On-the-go Mode Active">
                                        <svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><rect x="5" y="2" width="14" height="20" rx="2" ry="2"></rect><line x1="12" x2="12.01" y1="18" y2="18"></line></svg>
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
                             <span class="channel-timer text-[10px] text-zinc-500 font-medium" data-created-at="${roomInfo.created_at || 0}">
                                ${formatDuration(roomInfo.created_at)}
                             </span>
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
                    sessionStorage.setItem('rustrooms_welcomed', 'true');
                    window.location.href = `/${crypto.randomUUID()}`;
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
                     sessionStorage.setItem('rustrooms_welcomed', 'true');
                     window.location.href = `/${crypto.randomUUID()}`;
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

        function proceedToSetup() {
            sessionStorage.setItem('rustrooms_welcomed', 'true');
            const inviteOverlay = document.getElementById('inviteWelcomeOverlay');
            inviteOverlay.classList.add('opacity-0');
            setTimeout(() => {
                inviteOverlay.classList.add('hidden');
                configOverlay.classList.remove('hidden');
                configOverlay.classList.remove('opacity-0');
                initSetupButtonTouchHandlers();
                loadDevices();
            }, 300);
        }

        async function updateInviteOverlay() {
            if (!roomId || !channelId) return;
            if (sessionStorage.getItem('rustrooms_welcomed') === 'true') return;
            
            try {
                const res = await fetch(`/${roomId}/${encodeURIComponent(channelId)}/status`);
                if (!res.ok) return;
                
                const data = await res.json();
                
                document.getElementById('inviteChannelName').innerText = `# ${data.name}`;
                
                const userList = document.getElementById('inviteUserList');
                userList.innerHTML = '';
                
                const uids = Object.keys(data.users);
                if (uids.length === 0) {
                    userList.innerHTML = '<p class="text-zinc-500 italic text-sm">No one is here yet. Be the first!</p>';
                } else {
                    uids.forEach(uid => {
                        const u = data.users[uid];
                        const userDiv = document.createElement('div');
                        userDiv.className = 'flex flex-col items-center gap-2 p-3 rounded-2xl bg-zinc-900/50 border border-zinc-800 min-w-[100px]';
                        userDiv.innerHTML = `
                            <div class="w-12 h-12 rounded-xl overflow-hidden bg-zinc-800 border border-zinc-700">
                                ${u.avatar ? `<img src="${escapeHtml(u.staticFrame || u.avatar)}" class="w-full h-full object-cover">` : `<div class="w-full h-full flex items-center justify-center text-xl">👤</div>`}
                            </div>
                            <span class="text-xs font-semibold text-zinc-300 truncate max-w-[80px]">${escapeHtml(u.nickname)}</span>
                        `;
                        userList.appendChild(userDiv);
                    });
                }
                
                if (data.created_at > 0) {
                    const updateDuration = () => {
                        const el = document.getElementById('inviteCallDuration');
                        if (el) el.innerText = `Running for ${formatDuration(data.created_at)}`;
                    };
                    updateDuration();
                    setInterval(updateDuration, 1000);
                }
                
                const inviteOverlay = document.getElementById('inviteWelcomeOverlay');
                inviteOverlay.classList.remove('hidden');
                setTimeout(() => inviteOverlay.classList.remove('opacity-0'), 10);
                
            } catch (e) {
                console.error("Error fetching status", e);
                // Fallback to setup screen
                configOverlay.classList.remove('hidden');
                configOverlay.classList.remove('opacity-0');
            }
        }

        function formatDuration(createdAt) {
            if (!createdAt) return "0:00";
            const now = Math.floor(Date.now() / 1000);
            const diff = Math.max(0, now - createdAt);
            const h = Math.floor(diff / 3600);
            const m = Math.floor((diff % 3600) / 60);
            const s = diff % 60;
            
            if (h > 0) {
                return `${h}:${m.toString().padStart(2, '0')}:${s.toString().padStart(2, '0')}`;
            }
            return `${m}:${s.toString().padStart(2, '0')}`;
        }

        setInterval(() => {
            document.querySelectorAll('.channel-timer').forEach(el => {
                const createdAt = parseInt(el.dataset.createdAt);
                if (createdAt) {
                    el.innerText = formatDuration(createdAt);
                }
            });
        }, 1000);

        if (roomId) {
            loadPreferences();

            // Enable On the Go mode options only for mobile and tablet devices
            if (isMobileDevice()) {
                const btnOnTheGo = document.getElementById('btnOnTheGo');
                if (btnOnTheGo) btnOnTheGo.classList.remove('hidden');

                const setupOtgRow = document.getElementById('setupOnTheGoRow');
                if (setupOtgRow) setupOtgRow.classList.remove('hidden');

                const settingsOtgRow = document.getElementById('settingsOnTheGoRow');
                if (settingsOtgRow) settingsOtgRow.classList.remove('hidden');
            } else {
                // Ensure On-the-go mode setting is inactive on desktop
                isOnTheGoMode = false;
            }

            const setupDone = sessionStorage.getItem('rustrooms_setup_done') === 'true';
            const welcomed = sessionStorage.getItem('rustrooms_welcomed') === 'true';

            if (setupDone && roomId) {
                loadDevices().then(() => joinRoom());
            } else if (welcomed) {
                configOverlay.classList.remove('hidden');
                configOverlay.classList.remove('opacity-0');
                initSetupButtonTouchHandlers();
                loadDevices();
            } else {
                updateInviteOverlay();
            }
        } else {
            welcomeOverlay.style.display = 'flex';
        }

        function initSetupButtonTouchHandlers() {
            const btnPreviewMic = document.getElementById('btnPreviewMic');
            const btnPreviewCam = document.getElementById('btnPreviewCam');
            const speakerTestButtons = document.querySelectorAll('.btn-icon-test');

            [btnPreviewMic, btnPreviewCam].forEach(btn => {
                if (btn) {
                    btn.addEventListener('touchstart', function() {
                        this.classList.add('is-pressed');
                    }, { passive: true });
                    btn.addEventListener('touchend', function() {
                        this.classList.remove('is-pressed');
                    }, { passive: true });
                    btn.addEventListener('touchcancel', function() {
                        this.classList.remove('is-pressed');
                    }, { passive: true });
                }
            });

            speakerTestButtons.forEach(btn => {
                btn.addEventListener('touchstart', function() {
                    this.classList.add('is-pressed');
                }, { passive: true });
                btn.addEventListener('touchend', function() {
                    this.classList.remove('is-pressed');
                }, { passive: true });
                btn.addEventListener('touchcancel', function() {
                    this.classList.remove('is-pressed');
                }, { passive: true });
            });
        }

        function connectWs() {
            // Close any existing WebSocket to prevent ghost connections
            wsConnectionId++;
            const thisConnectionId = wsConnectionId;

            if (ws) {
                const oldWs = ws;
                oldWs.onclose = null;
                oldWs.onerror = null;
                if (oldWs.readyState === WebSocket.OPEN || oldWs.readyState === WebSocket.CONNECTING) {
                    oldWs.close();
                }
            }

            // Cancel any pending timers from previous connection attempts
            if (reconnectTimer) {
                clearTimeout(reconnectTimer);
                reconnectTimer = null;
            }
            if (iosSlowRetryTimer) {
                clearTimeout(iosSlowRetryTimer);
                iosSlowRetryTimer = null;
            }
            if (desktopSlowRetryTimer) {
                clearTimeout(desktopSlowRetryTimer);
                desktopSlowRetryTimer = null;
            }
            if (reconnectStatusTimeout) {
                clearTimeout(reconnectStatusTimeout);
                reconnectStatusTimeout = null;
            }

            stopHeartbeat();
            isReconnecting = false;
            updateStatus('connecting', 'Connecting...');

            Object.keys(peers).forEach(uid => {
                removePeer(uid);
            });
            peerCamStatus = {};
            peerScreenStatus = {};
            peerScreenHasAudio = {};
            pendingCandidates = {};

            ws = new WebSocket(wsUrl);

                        ws.onopen = () => {
                            if (wsConnectionId !== thisConnectionId) return; // stale connection

                            if (reconnectStatusTimeout) {
                                clearTimeout(reconnectStatusTimeout);
                                reconnectStatusTimeout = null;
                            }
                            if (iosSlowRetryTimer) {
                                clearTimeout(iosSlowRetryTimer);
                                iosSlowRetryTimer = null;
                            }
                            if (desktopSlowRetryTimer) {
                                clearTimeout(desktopSlowRetryTimer);
                                desktopSlowRetryTimer = null;
                            }
                            if (reconnectTimer) {
                                clearTimeout(reconnectTimer);
                                reconnectTimer = null;
                            }

                            playNotificationSound('join');
                            reconnectionAttempts = 0;
                            desktopSlowRetryCount = 0;
                            isReconnecting = false;
                            updateStatus('connected', 'Connected');
                            startHeartbeat();
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
                                    isGif: userAvatarIsGif,
                                    staticFrame: userAvatarStaticFrame,
                                    camEnabled: camEnabled,
                                    screenEnabled: screenEnabled,
                                    screenAudio: screenHasAudio,
                                    micTrackId: audioTrack ? audioTrack.id : null,
                                    screenAudioTrackId: screenStream ? (screenStream.getAudioTracks()[0]?.id || null) : null,
                                    isMuted: isMuted,
                                    isDeafened: isDeafened,
                                    isLowBandwidthMode: isLowBandwidthMode,
                                    isOnTheGoMode: isOnTheGoMode,
                                    password: roomCreationPassword
                                }
                            }));
                            checkEmpty();
                        };

                        ws.onmessage = async (event) => {
                            if (wsConnectionId !== thisConnectionId) return; // stale connection
                            const msg = JSON.parse(event.data);

                            switch (msg.type) {
                                case 'joined':
                                    if (msg.userId) {
                                        persistentUserId = msg.userId;
                                        localStorage.setItem('rustrooms_user_id', persistentUserId);
                                    }
                                    break;
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
                                    try {
                                        globalRoomList = msg.data;
                                        if (typeof updateRoomListUI === 'function') updateRoomListUI();
                                    } catch (e) { console.error("Error updating room-list UI:", e); }
                                    break;
                                case 'room-deleted':
                                    alert("The room has been deleted.");
                                    window.location.href = "/";
                                    break;
                                case 'existing-users':
                                    try {
                                        if (msg.data && Array.isArray(msg.data.users)) {
                                            msg.data.users.forEach(user => {
                                                if (user.status.isScreenSharing !== undefined) {
                                                    peerScreenStatus[user.id] = user.status.isScreenSharing;
                                                }
                                                if (user.status.isLowBandwidthMode !== undefined) {
                                                    peerLowBandwidthStatus[user.id] = user.status.isLowBandwidthMode;
                                                }
                                                if (user.status.isOnTheGoMode !== undefined) {
                                                    peerOnTheGoStatus[user.id] = user.status.isOnTheGoMode;
                                                }
                                                if (peers[user.id]) {
                                                    updatePeerInfo(user.id, user.status.nickname, user.status.avatar, user.status.isMuted, user.status.isDeafened, user.status.isGif, user.status.staticFrame);
                                                } else {
                                                    initPeer(user.id, false, user.status.nickname, user.status.avatar, user.status.isMuted, user.status.isDeafened, user.status.isGif, user.status.staticFrame);
                                                }
                                            });
                                            updateAllSenderBitrates();
                                        }
                                    } catch (e) { console.error("Error processing existing-users:", e); }
                                    break;
                                case 'user-joined':
                                    try {
                                        playNotificationSound('join');
                                        const joinedScreenAudio = getScreenAudioFlag(msg.data);
                                        updatePeerTrackHints(msg.userId, msg.data);

                                        if (msg.data.camEnabled !== undefined) {
                                            peerCamStatus[msg.userId] = msg.data.camEnabled;
                                        }
                                        if (msg.data.screenEnabled !== undefined) {
                                            peerScreenStatus[msg.userId] = msg.data.screenEnabled;
                                        }
                                        if (msg.data.isLowBandwidthMode !== undefined) {
                                            peerLowBandwidthStatus[msg.userId] = msg.data.isLowBandwidthMode;
                                            updateAllSenderBitrates();
                                        }
                                        if (msg.data.isOnTheGoMode !== undefined) {
                                            peerOnTheGoStatus[msg.userId] = msg.data.isOnTheGoMode;
                                        }

                                        if (peers[msg.userId]) {
                                            updatePeerInfo(msg.userId, msg.data?.nickname, msg.data?.avatar, msg.data?.isMuted, msg.data?.isDeafened, msg.data?.isGif, msg.data?.staticFrame);
                                            if (joinedScreenAudio !== undefined) {
                                                peerScreenHasAudio[msg.userId] = joinedScreenAudio;
                                            }
                                        } else {
                                            if (joinedScreenAudio !== undefined) {
                                                peerScreenHasAudio[msg.userId] = joinedScreenAudio;
                                            }
                                            initPeer(msg.userId, true, msg.data?.nickname, msg.data?.avatar, msg.data?.isMuted, msg.data?.isDeafened, msg.data?.isGif, msg.data?.staticFrame);
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
                                                isGif: userAvatarIsGif,
                                                staticFrame: userAvatarStaticFrame,
                                                camEnabled: myCamEnabled,
                                                screenEnabled: myScreenEnabled,
                                                screenAudio: myScreenHasAudio,
                                                micTrackId: myAudioTrack ? myAudioTrack.id : null,
                                                screenAudioTrackId: screenStream ? (screenStream.getAudioTracks()[0]?.id || null) : null,
                                                isMuted: myMuted,
                                                isDeafened: isDeafened,
                                                isLowBandwidthMode: isLowBandwidthMode,
                                                isOnTheGoMode: isOnTheGoMode
                                            }
                                        }));
                                    } catch (e) { console.error("Error processing user-joined:", e); }
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
                                        delete peerLowBandwidthStatus[msg.userId];
                                        delete peerOnTheGoStatus[msg.userId];
                                    }
                                    break;
                                case 'user-kicked':
                                    if (msg.userId === persistentUserId) {
                                        hasLeftRoom = true;
                                        alert("You have been kicked from the room.");
                                        sessionStorage.removeItem('rustrooms_setup_done');
                                        window.location.href = "/";
                                    } else {
                                        playNotificationSound('leave');
                                        removePeer(msg.userId);
                                        delete peerCamStatus[msg.userId];
                                        delete peerScreenStatus[msg.userId];
                                        delete peerScreenHasAudio[msg.userId];
                                        delete peerMicTrackId[msg.userId];
                                        delete peerScreenAudioTrackId[msg.userId];
                                        delete peerLowBandwidthStatus[msg.userId];
                                        delete peerOnTheGoStatus[msg.userId];
                                        updateRoomListUI();
                                    }
                                    break;
                                case 'user-update':
                                     updatePeerTrackHints(msg.userId, msg.data);
                                     if (msg.data.isLowBandwidthMode !== undefined) {
                                         peerLowBandwidthStatus[msg.userId] = msg.data.isLowBandwidthMode;
                                         updateAllSenderBitrates();
                                     }
                                     if (msg.data.isOnTheGoMode !== undefined) {
                                         peerOnTheGoStatus[msg.userId] = msg.data.isOnTheGoMode;
                                     }
                                     updatePeerInfo(msg.userId, msg.data.nickname, msg.data.avatar, msg.data.isMuted, msg.data.isDeafened, msg.data.isGif, msg.data.staticFrame);
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

                                        const wrapper = document.getElementById(`wrapper-${msg.userId}`);
                                        if (wrapper) {
                                            const vid = document.getElementById(`vid-${msg.userId}`);
                                            if (vid && vid.srcObject && vid.srcObject.getAudioTracks().length > 0) {
                                                (async () => { await setupAudioMonitor(vid.srcObject, `wrapper-${msg.userId}`); })();
                                            }
                                        }
                                    }
                                    break;
                                case 'identify':
                                    try {
                                        const identifiedScreenAudio = getScreenAudioFlag(msg.data);
                                        updatePeerTrackHints(msg.userId, msg.data);
                                        if (msg.data.camEnabled !== undefined) {
                                            peerCamStatus[msg.userId] = msg.data.camEnabled;
                                        }
                                        if (msg.data.screenEnabled !== undefined) {
                                            peerScreenStatus[msg.userId] = msg.data.screenEnabled;
                                        }
                                        if (msg.data.isLowBandwidthMode !== undefined) {
                                            peerLowBandwidthStatus[msg.userId] = msg.data.isLowBandwidthMode;
                                            updateAllSenderBitrates();
                                        }
                                        if (msg.data.isOnTheGoMode !== undefined) {
                                            peerOnTheGoStatus[msg.userId] = msg.data.isOnTheGoMode;
                                        }
                                        if (identifiedScreenAudio !== undefined) {
                                            peerScreenHasAudio[msg.userId] = identifiedScreenAudio;
                                        }
                                        if (peers[msg.userId]) {
                                            updatePeerInfo(msg.userId, msg.data.nickname, msg.data.avatar, msg.data.isMuted, msg.data.isDeafened, msg.data.isGif, msg.data.staticFrame);
                                        } else {
                                            initPeer(msg.userId, false, msg.data.nickname, msg.data.avatar, msg.data.isMuted, msg.data.isDeafened, msg.data.isGif, msg.data.staticFrame);
                                        }
                                        if (peerScreenStatus[msg.userId] === true && identifiedScreenAudio === true) {
                                            ensureScreenAudioUI(msg.userId);
                                        }
                                    } catch (e) { console.error("Error processing identify:", e); }
                                    break;
                                case 'rename-channel':
                                    if (roomId === msg.data.roomId && channelId === msg.data.oldName) {
                                        performChannelSwitch(roomId, msg.data.newName);
                                    }
                                    break;
                                case 'signal':
                                    handleSignal(msg.userId, msg.data);
                                    break;
                                case 'keepalive':
                                    // Server keepalive — ignore (not a pong response to our ping)
                                    break;
                                case 'pong':
                                    handlePong();
                                    break;
                            }
                        };

                        ws.onclose = (event) => {
                            if (wsConnectionId !== thisConnectionId) return; // stale connection

                            // Code 4001 = server inactivity timeout — skip reconnect, show disconnected
                            if (event.code === 4001) {
                                stopHeartbeat();
                                updateStatus('disconnected', 'Disconnected (inactive)');
                                showReconnectButtons();
                                isReconnecting = false;
                                return;
                            }

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
                            if (reconnectionAttempts >= maxReconnectionAttempts) {
                                if (isIOS) {
                                    // On iOS, never fully give up — fall back to slow periodic retries
                                    console.warn(`iOS: exhausted ${maxReconnectionAttempts} fast retries, switching to slow retry every 30s`);
                                    updateStatus('connecting', 'Connection lost — retrying...');
                                    showReconnectButtons();
                                    isReconnecting = false;
                                    iosSlowRetryTimer = setTimeout(() => {
                                        iosSlowRetryTimer = null;
                                        if (!hasLeftRoom && (!ws || ws.readyState !== WebSocket.OPEN)) {
                                            reconnectionAttempts = Math.floor(maxReconnectionAttempts * 0.75);
                                            isReconnecting = false;
                                            connectWs();
                                        }
                                    }, 30000);
                                } else {
                                    // Desktop: fall back to slow periodic retries (every 60s, up to 5 times)
                                    // before giving up entirely, so transient outages don't require manual action
                                    console.warn(`Desktop: exhausted ${maxReconnectionAttempts} fast retries, switching to slow retry every 60s`);
                                    updateStatus('connecting', 'Connection lost — retrying...');
                                    showReconnectButtons();
                                    isReconnecting = false;
                                    desktopSlowRetryCount = (desktopSlowRetryCount || 0) + 1;
                                    if (desktopSlowRetryCount <= 5) {
                                        desktopSlowRetryTimer = setTimeout(() => {
                                            desktopSlowRetryTimer = null;
                                            if (!hasLeftRoom && (!ws || ws.readyState !== WebSocket.OPEN)) {
                                                reconnectionAttempts = Math.floor(maxReconnectionAttempts * 0.75);
                                                isReconnecting = false;
                                                connectWs();
                                            }
                                        }, 60000);
                                    } else {
                                        updateStatus('disconnected', 'Disconnected');
                                        console.error('WebSocket disconnected after multiple retries. No further attempts will be made.');
                                        stopHeartbeat();
                                    }
                                }
                            } else {
                                const delay = getReconnectDelay(reconnectionAttempts);

                                reconnectStatusTimeout = setTimeout(() => {

                                    if (isReconnecting && (!ws || ws.readyState !== WebSocket.OPEN)) {
                                        updateStatus('connecting', `Reconnecting... (Attempt ${reconnectionAttempts}/${maxReconnectionAttempts})`);
                                    }
                                }, reconnectDelayMs);

                                console.log(`Reconnecting in ${Math.round(delay)}ms...`);
                                reconnectTimer = setTimeout(() => {
                                    reconnectTimer = null;
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
                            // onerror is usually followed by onclose which handles reconnection,
                            // but on some mobile browsers onclose may not fire after onerror.
                            // Kick off a fallback reconnect after a short delay if onclose doesn't fire.
                            const errConnectionId = thisConnectionId;
                            setTimeout(() => {
                                if (wsConnectionId !== errConnectionId) return; // stale
                                if (hasLeftRoom) return;
                                if (ws && ws.readyState === WebSocket.CONNECTING) return; // still connecting
                                if (isReconnecting) return;
                                // If the socket is closed/closing and onclose never fired, trigger reconnect
                                if (!ws || ws.readyState === WebSocket.CLOSED || ws.readyState === WebSocket.CLOSING) {
                                    console.warn('onerror fallback: socket appears dead without onclose, triggering reconnect');
                                    reconnectionAttempts++;
                                    isReconnecting = true;
                                    const delay = getReconnectDelay(reconnectionAttempts);
                                    reconnectTimer = setTimeout(() => {
                                        reconnectTimer = null;
                                        isReconnecting = false;
                                        connectWs();
                                    }, delay);
                                }
                            }, 2000);
                        };
                    }

        function retryConnection() {
            // Cancel any pending timers from previous connection attempts
            if (reconnectTimer) {
                clearTimeout(reconnectTimer);
                reconnectTimer = null;
            }
            if (iosSlowRetryTimer) {
                clearTimeout(iosSlowRetryTimer);
                iosSlowRetryTimer = null;
            }
            if (desktopSlowRetryTimer) {
                clearTimeout(desktopSlowRetryTimer);
                desktopSlowRetryTimer = null;
            }
            if (reconnectStatusTimeout) {
                clearTimeout(reconnectStatusTimeout);
                reconnectStatusTimeout = null;
            }

            const btns = [
                document.getElementById('btnReconnect'),
                document.getElementById('onTheGoBtnReconnect')
            ];
            btns.forEach(btn => {
                if (btn) {
                    btn.classList.add('text-green-500', 'bg-green-500/10');
                    btn.classList.remove('text-slate-400', 'hover:text-white', 'hover:bg-slate-700');
                }
            });

            setTimeout(() => {
                btns.forEach(btn => {
                    if (btn) {
                        btn.classList.add('hidden');
                        btn.classList.remove('text-green-500', 'bg-green-500/10');
                        btn.classList.add('text-slate-400', 'hover:text-white', 'hover:bg-slate-700');
                    }
                });

                hasLeftRoom = false;
                isReconnecting = false;
                reconnectionAttempts = 0;
                desktopSlowRetryCount = 0;
                connectWs();
            }, 300);
        }

        function setAvatar(layer, avatar, isGif, staticFrame) {
            layer.innerHTML = '';
            if (avatar) {
               const displaySrc = isGif && staticFrame ? staticFrame : avatar;
               const bgImg = document.createElement('img');
               bgImg.src = displaySrc;
               bgImg.className = 'avatar-img';
               bgImg.draggable = false;
               if (isGif && staticFrame) {
                   bgImg.dataset.gifSrc = avatar;
                   bgImg.dataset.staticSrc = staticFrame;
               }

               const centerDiv = document.createElement('div');
               centerDiv.className = 'avatar-center';

               const centerImg = document.createElement('img');
               centerImg.src = displaySrc;
               centerImg.draggable = false;
               if (isGif && staticFrame) {
                   centerImg.dataset.gifSrc = avatar;
                   centerImg.dataset.staticSrc = staticFrame;
               }

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

        function updatePeerInfo(userId, nickname, avatar, isMuted, isDeafened, isGif, staticFrame) {
            const wrapper = document.getElementById(`wrapper-${userId}`);
            if (wrapper) {
                const nameSpan = wrapper.querySelector('.peer-name');
                if (nameSpan && nickname) nameSpan.innerText = nickname;

                const statusContainer = wrapper.querySelector('.peer-status-icons');
                if (statusContainer) {
                    statusContainer.innerHTML = '';
                    let hasIcons = false;
                    let iconsHTML = '';

                    if (isDeafened) {
                        hasIcons = true;
                        iconsHTML += `<span class="text-red-500" title="Deafened"><svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><line x1="1" y1="1" x2="23" y2="23"></line><path d="M21 14a2 2 0 0 0-2-2h-3a2 2 0 0 0-2 2v3a2 2 0 0 0 2 2h1a2 2 0 0 0 2-2V14z"></path><path d="M3 14a2 2 0 0 1 2-2h3a2 2 0 0 1 2 2v3a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V14z"></path><path d="M20.4 10.4C20.2 6.5 17 3.5 13 3.1"></path><path d="M6.5 5.5A9 9 0 0 0 3 12"></path></svg></span>`;
                    } else if (isMuted) {
                        hasIcons = true;
                        iconsHTML += `<span class="text-red-500" title="Muted"><svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><line x1="1" y1="1" x2="23" y2="23"></line><path d="M9 9v3a3 3 0 0 0 5.12 2.12M15 9.34V4a3 3 0 0 0-5.94-.6"></path><path d="M17 16.95A7 7 0 0 1 5 12v-2m14 0v2a7 7 0 0 1-.11 1.23"></path></svg></span>`;
                    }

                    const isPeerLBM = (userId === 'local') ? isLowBandwidthMode : (peerLowBandwidthStatus[userId] === true);
                    if (isPeerLBM) {
                        hasIcons = true;
                        iconsHTML += `
                            <span class="text-amber-500 animate-pulse" title="Low Bandwidth Mode Active">
                                <svg class="w-3.5 h-3.5" fill="currentColor" viewBox="0 0 24 24"><path d="M13 10V3L4 14h7v7l9-11h-7z" /></svg>
                            </span>
                        `;
                    }

                    const isPeerOTG = (userId === 'local') ? isOnTheGoMode : (peerOnTheGoStatus[userId] === true);
                    if (isPeerOTG) {
                        hasIcons = true;
                        iconsHTML += `
                            <span class="text-blue-400" title="On-the-go Mode Active">
                                <svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><rect x="5" y="2" width="14" height="20" rx="2" ry="2"></rect><line x1="12" x2="12.01" y1="18" y2="18"></line></svg>
                            </span>
                        `;
                    }

                    if (hasIcons) {
                        statusContainer.classList.remove('hidden');
                        statusContainer.classList.add('flex', 'items-center', 'gap-1.5');
                        statusContainer.innerHTML = iconsHTML;
                    } else {
                        statusContainer.classList.add('hidden');
                        statusContainer.classList.remove('flex', 'items-center', 'gap-1.5');
                    }
                }

                const avatarLayer = wrapper.querySelector('.avatar-layer');
                if (avatarLayer) {
                     setAvatar(avatarLayer, avatar, isGif, staticFrame);
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
            tilePositions: null,
            // Autoscroll fields
            scrollSpeed: 0,
            scrollInterval: null,
            lastClientX: 0,
            lastClientY: 0
        };

        function startAutoScroll(speed) {
            dragState.scrollSpeed = speed;
            if (!dragState.scrollInterval) {
                const scrollContainer = remoteGrid.parentElement;
                const scrollLoop = () => {
                    if (!dragState.isDragging || !dragState.scrollSpeed) {
                        dragState.scrollInterval = null;
                        return;
                    }
                    
                    scrollContainer.scrollTop += dragState.scrollSpeed;
                    
                    // Update positions of remaining tiles relative to the scrolled viewport
                    updateTilePositions();
                    
                    // Trigger intersection & reordering checks at the updated positions
                    checkIntersectionAndReorder(dragState.lastClientX, dragState.lastClientY);
                    
                    dragState.scrollInterval = requestAnimationFrame(scrollLoop);
                };
                dragState.scrollInterval = requestAnimationFrame(scrollLoop);
            }
        }

        function stopAutoScroll() {
            dragState.scrollSpeed = 0;
            if (dragState.scrollInterval) {
                cancelAnimationFrame(dragState.scrollInterval);
                dragState.scrollInterval = null;
            }
        }

        function updateTilePositions() {
            if (!dragState.isDragging) return;
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
        }

        function checkIntersectionAndReorder(clientX, clientY) {
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
        }

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

            dragState.lastClientX = clientX;
            dragState.lastClientY = clientY;

            if (!dragState.isDragging) {
                const deltaX = Math.abs(clientX - dragState.startX);
                const deltaY = Math.abs(clientY - dragState.startY);
                if (deltaX < 5 && deltaY < 5) return;

                dragState.isDragging = true;
                dragState.allTiles = [...remoteGrid.querySelectorAll('.video-container')];
                dragState.currentIndex = dragState.allTiles.indexOf(dragState.draggedEl);

                updateTilePositions();

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

            checkIntersectionAndReorder(clientX, clientY);

            // AUTO-SCROLL LOGIC
            const scrollContainer = remoteGrid.parentElement;
            const containerRect = scrollContainer.getBoundingClientRect();
            
            const threshold = 60;
            const distFromTop = clientY - containerRect.top;
            const distFromBottom = containerRect.bottom - clientY;

            if (distFromTop < threshold) {
                const speed = -Math.max(2, (1 - distFromTop / threshold) * 15);
                startAutoScroll(speed);
            } else if (distFromBottom < threshold) {
                const speed = Math.max(2, (1 - distFromBottom / threshold) * 15);
                startAutoScroll(speed);
            } else {
                stopAutoScroll();
            }

            if (isTouch) {
                e.preventDefault();
            }
        }

        function handleDragEnd(e) {
            stopAutoScroll();

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
            remoteGrid.className = 'grid gap-2 md:gap-4 w-full h-full max-w-[1600px] transition-all duration-500 grid-expand my-auto';

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

        function negotiate(userId, pc, isIceRestart = false) {
            if (!peers[userId] || peers[userId] !== pc) return; // peer was removed
            if (pc.connectionState === 'closed' || pc.connectionState === 'failed') return; // peer is dead
            const options = isIceRestart ? { iceRestart: true } : {};
            pc.createOffer(options)
                .then(offer => {
                    if (!peers[userId] || peers[userId] !== pc) return; // peer was removed during async op
                    offer.sdp = forceStereoAudio(offer.sdp);
                    return pc.setLocalDescription(offer);
                })
                .then(() => {
                    if (!peers[userId] || peers[userId] !== pc) return; // peer was removed during async op
                    sendSignal(userId, { type: 'offer', sdp: pc.localDescription });
                })
                .catch(e => console.error("Negotiation error", e));
        }

        function createPeerUI(userId, displayName, avatarUrl, remoteIsDeafened, remoteIsMuted, isGif, staticFrame) {

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

            setAvatar(avatarLayer, avatarUrl, isGif, staticFrame);

            const label = document.createElement('div');
            label.className = 'name-tag absolute bottom-3 left-3 bg-black/45 backdrop-blur-xl px-3 py-1.5 rounded-lg text-sm text-white z-30 flex items-center gap-1.5';

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
            fsBtn.className = 'absolute top-3 right-3 p-2 rounded-lg bg-black/40 hover:bg-black/60 text-white backdrop-blur-md transition-all opacity-0 group-hover:opacity-100 scale-95 hover:scale-100 z-30';
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

        function initPeer(userId, initiator, nickname, avatarUrl, isMuted, remoteIsDeafened, isGif, staticFrame) {
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

            createPeerUI(userId, displayName, avatarUrl, remoteIsDeafened, isMuted, isGif, staticFrame);
            updatePeerInfo(userId, displayName, avatarUrl, isMuted, remoteIsDeafened, isGif, staticFrame);

            pc.ontrack = (event) => {

                if (peers[userId] !== pc) {
                    return;
                }

                let container = document.getElementById(`wrapper-${userId}`);
                let vid = document.getElementById(`vid-${userId}`);

                if (!container || !vid) {
                    createPeerUI(userId, displayName, avatarUrl, remoteIsDeafened, isMuted, isGif, staticFrame);
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
                     vid.play().then(() => {
                         const sv = getVolumeSettings(userId, 'main');
                         if (vid.volume !== sv) vid.volume = sv;
                     }).catch(e => console.error("Remote play err", e));

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
                        const sv = getVolumeSettings(userId, 'main');
                        if (vid.volume !== sv) vid.volume = sv;
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

                    if (initiator) {
                        setTimeout(() => {
                            if (peers[userId] === pc && pc.connectionState === 'disconnected') {
                                console.log(`Triggering ICE restart for ${userId.substr(0,4)}`);
                                negotiate(userId, pc, true);
                            }
                        }, 2000);
                    }

                    if (!pc._disconnectTimeout) {
                        pc._disconnectTimeout = setTimeout(() => {
                            if (peers[userId] === pc && pc.connectionState === 'disconnected') {
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
                    const _vid = document.getElementById(`vid-${userId}`);
                    if (_vid) {
                        const sv = getVolumeSettings(userId, 'main');
                        if (_vid.volume !== sv) _vid.volume = sv;
                    }
                    const _screenAud = document.getElementById(`aud-screen-${userId}`);
                    if (_screenAud) {
                        const ssv = getVolumeSettings(userId, 'screen');
                        if (_screenAud.volume !== ssv) _screenAud.volume = ssv;
                    }
                    updateConnectionStatus();
                }
            };

            if (initiator) {
                negotiate(userId, pc);
            }
        }

        async function flushPendingCandidates(userId, pc) {
            if (!pendingCandidates[userId] || pendingCandidates[userId].length === 0) return;
            const candidates = pendingCandidates[userId].splice(0);
            for (const candidate of candidates) {
                try {
                    await pc.addIceCandidate(new RTCIceCandidate(candidate));
                } catch (e) {
                    console.warn("Failed to flush buffered ICE candidate for", userId.substr(0,4), e);
                }
            }
        }

        async function handleSignal(userId, data) {
            if (!peers[userId]) initPeer(userId, false, undefined, null);
            const pc = peers[userId];
            if (!pc || pc.connectionState === 'closed' || pc.connectionState === 'failed') return; // peer is dead

            try {
                if (data.type === 'offer') {
                    if (pc.signalingState !== 'stable' && pc.signalingState !== 'have-local-offer') {
                        console.warn(`Ignoring offer from ${userId.substr(0,4)} in state ${pc.signalingState}`);
                        return;
                    }
                    pendingCandidates[userId] = [];
                    await pc.setRemoteDescription(new RTCSessionDescription(data.sdp));
                    if (!peers[userId] || peers[userId] !== pc) return; // peer was removed during async op
                    await flushPendingCandidates(userId, pc);
                    if (!peers[userId] || peers[userId] !== pc) return;
                    const answer = await pc.createAnswer();
                    answer.sdp = forceStereoAudio(answer.sdp);
                    await pc.setLocalDescription(answer);
                    sendSignal(userId, { type: 'answer', sdp: answer });
                } else if (data.type === 'answer') {
                    if (pc.signalingState !== 'have-local-offer') {
                        console.warn(`Ignoring answer from ${userId.substr(0,4)} in state ${pc.signalingState}`);
                        return;
                    }
                    await pc.setRemoteDescription(new RTCSessionDescription(data.sdp));
                    if (!peers[userId] || peers[userId] !== pc) return; // peer was removed during async op
                    await flushPendingCandidates(userId, pc);
                } else if (data.type === 'candidate') {
                    if (!pc.remoteDescription || !pc.remoteDescription.type) {
                        if (!pendingCandidates[userId]) pendingCandidates[userId] = [];
                        pendingCandidates[userId].push(data.candidate);
                        return;
                    }
                    await pc.addIceCandidate(new RTCIceCandidate(data.candidate));
                }
            } catch (e) {
                console.error("Signaling error", e);
            }
        }

        function removePeer(userId) {
            cleanupAudioMonitor(`wrapper-${userId}`);

            if (peers[userId]) {
                try {
                    peers[userId].getReceivers().forEach(receiver => {
                        if (receiver.track) {
                            receiver.track.onmute = null;
                            receiver.track.onunmute = null;
                            receiver.track.onended = null;
                        }
                    });
                } catch(e) {}

                if (peers[userId]._disconnectTimeout) {
                    clearTimeout(peers[userId]._disconnectTimeout);
                    peers[userId]._disconnectTimeout = null;
                }
                peers[userId].close();
                delete peers[userId];
            }

            const vid = document.getElementById(`vid-${userId}`);
            if (vid) {
                vid.pause();
                if (vid.srcObject) {
                    try {
                        vid.srcObject.getTracks().forEach(track => track.stop());
                    } catch(e) {}
                    vid.srcObject = null;
                }
            }

            const el = document.getElementById(`wrapper-${userId}`);
            if (el) el.remove();

            const screenAud = document.getElementById(`aud-screen-${userId}`);
            if (screenAud) {
                screenAud.pause();
                screenAud.srcObject = null;
                screenAud.remove();
            }
            const volRow = document.getElementById(`vol-row-screen-${userId}`);
            if (volRow) volRow.remove();

            const sidebarAvatar = document.querySelector(`.room-user-row[data-user-id="${userId}"] .mini-avatar`);
            if (sidebarAvatar) sidebarAvatar.classList.remove('speaking-glow');

            delete peerMicTrackId[userId];
            delete peerScreenAudioTrackId[userId];
            delete pendingCandidates[userId];
            checkEmpty();
        }

        function sendSignal(toId, data) {
            ws.send(JSON.stringify({ type: 'signal', target: toId, data: data }));
        }

        window.toggleFullscreen = function(userId) {
            const el = document.getElementById(`wrapper-${userId}`);
            if (!el) return;

            const isFullscreen = document.fullscreenElement || document.webkitFullscreenElement || document.mozFullScreenElement || document.msFullscreenElement;

            if (!isFullscreen) {
                const vid = document.getElementById(`vid-${userId}`);

                if (el.requestFullscreen) {
                    el.requestFullscreen().catch(err => {
                        console.error(`Error attempting to enable fullscreen: ${err.message}`);
                    });
                } else if (el.webkitRequestFullscreen) {
                    el.webkitRequestFullscreen();
                } else if (el.mozRequestFullScreen) {
                    el.mozRequestFullScreen();
                } else if (el.msRequestFullscreen) {
                    el.msRequestFullscreen();
                } else if (vid && vid.webkitEnterFullscreen) {
                    vid.webkitEnterFullscreen();
                }
            } else {
                if (document.exitFullscreen) {
                    document.exitFullscreen();
                } else if (document.webkitExitFullscreen) {
                    document.webkitExitFullscreen();
                } else if (document.mozCancelFullScreen) {
                    document.mozCancelFullScreen();
                } else if (document.msExitFullscreen) {
                    document.msExitFullscreen();
                }
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

            // Release wake lock and stop no sleep video
            if (wakeLock) {
                try {
                    wakeLock.release();
                } catch(e) {}
                wakeLock = null;
            }
            stopNoSleepVideo();

            clearActiveTabSession(false);

            if (statsWindowVisible) {
                toggleStatsWindow();
            }

            playNotificationSound('leave');

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
                ws.onclose = null;
                ws.onerror = null;
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

            sessionStorage.setItem('rustrooms_welcomed', 'false');
            sessionStorage.setItem('rustrooms_setup_done', 'false');

            roomId = '';
            channelId = '';
            if (window.location.pathname !== '/') {
                history.pushState(null, "", "/");
            }

            const inviteOverlay = document.getElementById('inviteWelcomeOverlay');

            if (roomId) {
                updateInviteOverlay();
                if (welcomeOverlay) welcomeOverlay.style.display = 'none';
            } else {
                if (welcomeOverlay) welcomeOverlay.style.display = 'flex';
                if (inviteOverlay) {
                    inviteOverlay.classList.add('hidden', 'opacity-0');
                }
            }
            if (mainApp) mainApp.style.display = 'none';
            if (taskbar) taskbar.style.display = 'none';

            // Hide the On-the-go overlay visually, but preserve the setting
            const otgOverlay = document.getElementById('onTheGoOverlay');
            if (otgOverlay) {
                otgOverlay.classList.add('hidden');
                // Unlock screen orientation
                if (screen.orientation && screen.orientation.unlock) {
                    try {
                        screen.orientation.unlock();
                    } catch(e) {}
                }
            }

            // Reset AudioWorklet cached promise to support new AudioContext
            workletLoadingPromise = null;

            // Reset reconnection counters
            reconnectionAttempts = 0;
            desktopSlowRetryCount = 0;

            // Reset speaker tracking
            activeSpeakers = {};
            peerLowBandwidthStatus = {};
            peerOnTheGoStatus = {};

            // Hide/reset status pill elements to pristine states
            const reconnectBtn = document.getElementById('btnReconnect');
            if (reconnectBtn) reconnectBtn.classList.add('hidden');
            const otgReconnectBtn = document.getElementById('onTheGoBtnReconnect');
            if (otgReconnectBtn) otgReconnectBtn.classList.add('hidden');

            const pingContainer = document.getElementById('pingContainer');
            if (pingContainer) pingContainer.classList.add('hidden');
            const otgPingContainer = document.getElementById('onTheGoPingContainer');
            if (otgPingContainer) otgPingContainer.classList.add('hidden');

            const lightning = document.getElementById('lowBandwidthLightning');
            if (lightning) lightning.classList.add('hidden');
            const otgLightning = document.getElementById('onTheGoLowBandwidthLightning');
            if (otgLightning) otgLightning.classList.add('hidden');

            statusText.innerText = 'Waiting...';
            connectionDot.className = 'connection-dot';
            const otgStatusText = document.getElementById('onTheGoStatusText');
            if (otgStatusText) otgStatusText.innerText = 'Connected';
            const otgDot = document.getElementById('onTheGoConnectionDot');
            if (otgDot) otgDot.className = 'connection-dot';

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
                updateOnTheGoButtons();

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
            updateOnTheGoButtons();
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
                    if (track.readyState === 'ended' || track.muted) {
                        trackIsBroken = true;
                        console.warn("Camera track is broken, cleaning up");
                        track.stop();
                        localStream.removeTrack(track);
                        tracks = [];
                    }
                }

                if (tracks.length === 0 || trackIsBroken) {

                    btn.innerHTML = `<svg class="spinner" xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M21 12a9 9 0 1 1-6.219-8.56"/></svg>`;

                    try {
                        const camVideoConstraints = currentVideoInputId ? { deviceId: { exact: currentVideoInputId } } : { facingMode: currentFacingMode };
                        const newStream = await navigator.mediaDevices.getUserMedia({ video: camVideoConstraints });
                        if (isUnloading) {
                            newStream.getTracks().forEach(t => t.stop());
                            return;
                        }
                        const newTrack = newStream.getVideoTracks()[0];

                        if (!newTrack || newTrack.readyState !== 'live') {
                            console.warn("Camera track not properly initialized, retrying...");
                            newTrack?.stop();
                            await new Promise(r => setTimeout(r, 100));
                            const retryStream = await navigator.mediaDevices.getUserMedia({ video: camVideoConstraints });
                            if (isUnloading) {
                                retryStream.getTracks().forEach(t => t.stop());
                                return;
                            }
                            const retryTrack = retryStream.getVideoTracks()[0];
                            if (retryTrack) {                                retryTrack.enabled = true;
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

                        const camVideoConstraints = currentVideoInputId ? { deviceId: { exact: currentVideoInputId } } : { facingMode: currentFacingMode };
                        const newStream = await navigator.mediaDevices.getUserMedia({ video: camVideoConstraints });
                        if (isUnloading) {
                            newStream.getTracks().forEach(t => t.stop());
                            return;
                        }
                        const newTrack = newStream.getVideoTracks()[0];

                        if (!newTrack || newTrack.readyState !== 'live') {
                            console.warn("Camera track not properly initialized, retrying...");
                            newTrack?.stop();
                            await new Promise(r => setTimeout(r, 100));
                            const retryStream = await navigator.mediaDevices.getUserMedia({ video: camVideoConstraints });
                            if (isUnloading) {
                                retryStream.getTracks().forEach(t => t.stop());
                                return;
                            }
                            const retryTrack = retryStream.getVideoTracks()[0];
                            if (retryTrack) {                                retryTrack.enabled = true;
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

        let switchCamInProgress = false;

        async function switchCamera() {
            if (switchCamInProgress || !isCameraReady) return;
            if (!localStream) return;

            const videoTrack = localStream.getVideoTracks()[0];
            if (!videoTrack) return;

            switchCamInProgress = true;
            const btn = document.getElementById('btnSwitchCam');
            if (btn) btn.disabled = true;

            try {
                const trackSettings = videoTrack.getSettings();
                const actualFacing = trackSettings.facingMode || currentFacingMode;
                const newFacingMode = actualFacing === 'user' ? 'environment' : 'user';

                const newStream = await navigator.mediaDevices.getUserMedia({
                    video: { facingMode: { exact: newFacingMode } }
                });

                if (isUnloading) {
                    newStream.getTracks().forEach(t => t.stop());
                    return;
                }

                const newTrack = newStream.getVideoTracks()[0];
                if (!newTrack || newTrack.readyState !== 'live') {
                    newTrack?.stop();
                    throw new Error('Could not get camera track');
                }

                localStream.removeTrack(videoTrack);
                videoTrack.stop();
                localStream.addTrack(newTrack);

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

                const localVideoEl = document.getElementById('localVideo');
                if (localVideoEl) {
                    localVideoEl.srcObject = null;
                    localVideoEl.srcObject = localStream;
                }

                currentFacingMode = newTrack.getSettings().facingMode || newFacingMode;
                currentVideoInputId = newTrack.getSettings().deviceId || null;

                const settingsVideo = document.getElementById('settingsVideoSource');
                if (settingsVideo && currentVideoInputId) {
                    if ([...settingsVideo.options].some(o => o.value === currentVideoInputId)) {
                        settingsVideo.value = currentVideoInputId;
                    }
                }

                const setupVideo = document.getElementById('videoSource');
                if (setupVideo && currentVideoInputId) {
                    if ([...setupVideo.options].some(o => o.value === currentVideoInputId)) {
                        setupVideo.value = currentVideoInputId;
                    }
                }

                savePreferences();
            } catch (e) {
                console.error("Camera switch failed", e);
            } finally {
                switchCamInProgress = false;
                if (btn) btn.disabled = false;
            }
        }

        async function detectCameras() {
            try {
                const devices = await navigator.mediaDevices.enumerateDevices();
                const videoDevices = devices.filter(d => d.kind === 'videoinput');
                const btnSwitchCam = document.getElementById('btnSwitchCam');
                if (btnSwitchCam && videoDevices.length > 1) {
                    btnSwitchCam.classList.remove('hidden');
                }
            } catch (e) {
                console.warn('Could not enumerate devices for camera detection:', e);
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

                if (localStream && localStream.getAudioTracks().length > 0) {
                    await setupAudioMonitor(localStream, 'local');
                }

            } else {
                try {
                    screenStream = await navigator.mediaDevices.getDisplayMedia({
                        video: { cursor: true },
                        systemAudio: "include",
                        audio: {
                            echoCancellation: false,
                            noiseSuppression: false,
                            autoGainControl: false,
                            restrictOwnAudio: true,
                            channelCount: 2,
                            sampleRate: 48000,
                            sampleSize: 16
                        }
                    });
                    if (isUnloading) {
                        screenStream.getTracks().forEach(t => t.stop());
                        return;
                    }
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

                    if (localStream && localStream.getAudioTracks().length > 0) {
                        await setupAudioMonitor(localStream, 'local');
                    }
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
                statusIcons = `<span class="ml-1.5 inline-flex items-center text-red-500" title="Deafened"><svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><line x1="1" y1="1" x2="23" y2="23"></line><path d="M21 14a2 2 0 0 0-2-2h-3a2 2 0 0 0-2 2v3a2 2 0 0 0 2 2h1a2 2 0 0 0 2-2V14z"></path><path d="M3 14a2 2 0 0 1 2-2h3a2 2 0 0 1 2 2v3a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V14z"></path><path d="M20.4 10.4C20.2 6.5 17 3.5 13 3.1"></path><path d="M6.5 5.5A9 9 0 0 0 3 12"></path></svg></span>`;
            } else {
                const audioTrack = localStream ? localStream.getAudioTracks()[0] : null;
                if (!audioTrack || !audioTrack.enabled) {
                    statusIcons = `<span class="ml-1.5 inline-flex items-center text-red-500" title="Muted"><svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><line x1="1" y1="1" x2="23" y2="23"></line><path d="M9 9v3a3 3 0 0 0 5.12 2.12M15 9.34V4a3 3 0 0 0-5.94-.6"></path><path d="M17 16.95A7 7 0 0 1 5 12v-2m14 0v2a7 7 0 0 1-.11 1.23"></path></svg></span>`;
                }
            }

            if (isLowBandwidthMode) {
                statusIcons += `
                    <span class="ml-1.5 inline-flex items-center text-amber-500 animate-pulse" title="Low Bandwidth Mode Active">
                        <svg class="w-3.5 h-3.5" fill="currentColor" viewBox="0 0 24 24"><path d="M13 10V3L4 14h7v7l9-11h-7z" /></svg>
                    </span>
                `;
            }

            if (isOnTheGoMode) {
                statusIcons += `
                    <span class="ml-1.5 inline-flex items-center text-blue-400" title="On-the-go Mode Active">
                        <svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><rect x="5" y="2" width="14" height="20" rx="2" ry="2"></rect><line x1="12" x2="12.01" y1="18" y2="18"></line></svg>
                    </span>
                `;
            }

            label.innerHTML = `<span class="flex items-center">${escapeHtml(userNickname)} (You)${statusIcons}</span>`;
        }

        function copyLink() {
            navigator.clipboard.writeText(window.location.href);

            const btn = document.getElementById('btnCopy');
            const otgBtn = document.getElementById('btnOnTheGoCopy');

            if (btn && !btn.classList.contains('bg-green-600')) {
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

            if (otgBtn && !otgBtn.classList.contains('bg-emerald-600')) {
                const originalHTML = otgBtn.innerHTML;
                const originalClass = otgBtn.className;

                otgBtn.innerHTML = `
                    <div id="onTheGoCopyIconWrapper">
                        <svg class="w-7 h-7" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><polyline points="20 6 9 17 4 12"/></svg>
                    </div>
                    <span id="onTheGoCopyText">Copied!</span>
                `;
                otgBtn.classList.add('bg-emerald-600', 'border-emerald-600');
                otgBtn.classList.remove('bg-zinc-800', 'hover:bg-zinc-700', 'border-zinc-700');

                setTimeout(() => {
                    otgBtn.innerHTML = originalHTML;
                    otgBtn.className = originalClass;
                }, 2000);
            }
        }

        const settingsOverlay = document.getElementById('settingsOverlay');
        const settingsNicknameInput = document.getElementById('settingsNicknameInput');
        const settingsAvatarInput = document.getElementById('settingsAvatarInput');
        const settingsAvatarPreview = document.getElementById('settingsAvatarPreview');
        const settingsAvatarPlaceholder = document.getElementById('settingsAvatarPlaceholder');
        let newAvatarCandidate = null;
        let newAvatarCandidateIsGif = false;
        let newAvatarCandidateStaticFrame = null;
        let settingsInitialAudioId = '';
        let settingsInitialVideoId = '';
        let settingsInitialAudioOutputId = '';
        let settingsNicknameDebounce = null;

        function handleSettingsNicknameInput() {
            userNickname = settingsNicknameInput.value.trim() || "Guest";
            savePreferences();
            updateLocalLabel();
            if (settingsNicknameDebounce) clearTimeout(settingsNicknameDebounce);
            settingsNicknameDebounce = setTimeout(() => {
                if (ws && ws.readyState === WebSocket.OPEN) {
                    ws.send(JSON.stringify({
                        type: "update-user",
                        data: { nickname: userNickname }
                    }));
                }
            }, 500);
        }

        async function handleSettingsMicChange(value) {
            currentAudioInputId = value;
            const currentVideoTrack = localStream ? localStream.getVideoTracks()[0] : null;
            const currentVideoId = currentVideoTrack ? currentVideoTrack.getSettings().deviceId : null;
            await switchMediaStream(value, currentVideoId);
            savePreferences();
        }

        async function handleSettingsCamChange(value) {
            currentVideoInputId = value;
            const currentAudioTrack = localStream ? localStream.getAudioTracks()[0] : null;
            const currentAudioId = currentAudioTrack ? currentAudioTrack.getSettings().deviceId : null;
            await switchMediaStream(currentAudioId, value);
            savePreferences();
        }

        async function openSettings() {
            settingsNicknameInput.value = userNickname;
            newAvatarCandidate = userAvatar;
            newAvatarCandidateIsGif = userAvatarIsGif;
            newAvatarCandidateStaticFrame = userAvatarStaticFrame;

            const settingsLBM = document.getElementById('settingsLowBandwidth');
            if (settingsLBM) settingsLBM.checked = isLowBandwidthMode;
            const settingsOtg = document.getElementById('settingsOnTheGo');
            if (settingsOtg) settingsOtg.checked = isOnTheGoMode;

            const removeBtn = document.getElementById('btnRemoveSettingsAvatar');
            if (userAvatar) {
                const displaySrc = userAvatarIsGif && userAvatarStaticFrame ? userAvatarStaticFrame : userAvatar;
                settingsAvatarPreview.src = displaySrc;
                settingsAvatarPreview.classList.remove('hidden');
                settingsAvatarPlaceholder.classList.add('hidden');
                if (removeBtn) removeBtn.classList.remove('hidden');
            } else {
                settingsAvatarPreview.classList.add('hidden');
                settingsAvatarPlaceholder.classList.remove('hidden');
                if (removeBtn) removeBtn.classList.add('hidden');
            }

            await populateSettingsDeviceList();
            const settingsAudio = document.getElementById('settingsAudioSource');
            const settingsVideo = document.getElementById('settingsVideoSource');
            const settingsAudioOutput = document.getElementById('settingsAudioOutputSource');
            settingsInitialAudioId = settingsAudio ? settingsAudio.value : '';
            settingsInitialVideoId = settingsVideo ? settingsVideo.value : '';
            settingsInitialAudioOutputId = settingsAudioOutput ? settingsAudioOutput.value : '';
            settingsOverlay.classList.remove('hidden');
            initSetupButtonTouchHandlers();
            if (localStream) {
                await setupVolumeMeter(localStream, 'settingsMicBar');
            }
        }

        function closeSettings() {
            settingsOverlay.classList.add('hidden');
            if (settingsMeterFrameId) cancelAnimationFrame(settingsMeterFrameId);
            if (isOnTheGoMode) {
                toggleOnTheGoMode(true, true);
            }
        }

        function handleSettingsAvatarUpload(input) {
            const file = input.files[0];
            if (!file) return;

            if (file.type === 'image/gif') {
                const reader = new FileReader();
                reader.onload = function(e) {
                    const gifDataUrl = e.target.result;
                    newAvatarCandidate = gifDataUrl;
                    newAvatarCandidateIsGif = true;
                    extractGifFirstFrame(gifDataUrl).then(staticFrame => {
                        newAvatarCandidateStaticFrame = staticFrame;
                        settingsAvatarPreview.src = staticFrame;
                        settingsAvatarPreview.classList.remove('hidden');
                        settingsAvatarPlaceholder.classList.add('hidden');
                        const removeBtn = document.getElementById('btnRemoveSettingsAvatar');
                        if (removeBtn) removeBtn.classList.remove('hidden');
                        saveSettings();
                    });
                };
                reader.readAsDataURL(file);
            } else {
                resizeImageForAvatar(file).then(dataUrl => {
                    openCropModal(dataUrl, 'settings');
                });
            }
            input.value = '';
        }

        async function saveSettings() {
            userAvatar = newAvatarCandidate;
            userAvatarIsGif = newAvatarCandidateIsGif;
            userAvatarStaticFrame = newAvatarCandidateStaticFrame;
            savePreferences();

            updateLocalAvatar();

            if (ws && ws.readyState === WebSocket.OPEN) {
                 ws.send(JSON.stringify({
                    type: "update-user",
                    data: {
                        nickname: userNickname,
                        avatar: userAvatar,
                        isGif: userAvatarIsGif,
                        staticFrame: userAvatarStaticFrame
                    }
                }));
            }

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
                     const displaySrc = userAvatarIsGif && userAvatarStaticFrame ? userAvatarStaticFrame : userAvatar;
                     img.src = displaySrc;
                     img.classList.remove('hidden');

                     centerImg.src = displaySrc;
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
                    userAvatarIsGif = false;
                    userAvatarStaticFrame = null;
                    avatarPreview.src = userAvatar;
                    avatarPreview.classList.remove('hidden');
                    avatarPlaceholder.classList.add('hidden');
                    const removeBtn = document.getElementById('btnRemoveSetupAvatar');
                    if (removeBtn) removeBtn.classList.remove('hidden');
                    savePreferences();
                } else if (currentCropTarget === 'settings') {
                    newAvatarCandidate = base64;
                    newAvatarCandidateIsGif = false;
                    newAvatarCandidateStaticFrame = null;
                    settingsAvatarPreview.src = newAvatarCandidate;
                    settingsAvatarPreview.classList.remove('hidden');
                    settingsAvatarPlaceholder.classList.add('hidden');
                    const removeBtn = document.getElementById('btnRemoveSettingsAvatar');
                    if (removeBtn) removeBtn.classList.remove('hidden');
                    closeCropModal();
                    saveSettings();
                    return;
                }
                closeCropModal();
            });
        }
    </script>

    <div id="cropModal" class="fixed inset-0 z-[250] flex items-center justify-center p-4 hidden" style="background: var(--bg-primary);">
        <div class="glass-panel p-6 md:p-8 rounded-2xl w-full max-w-md max-h-[95vh] flex flex-col items-center relative z-10">
            <h3 class="text-xl font-bold tracking-tight mb-4" style="color: var(--text-primary);">Crop Your Avatar</h3>
            <div id="cropWrapper" class="w-full relative"></div>
            <div class="flex gap-4 w-full mt-2">
                <button onclick="closeCropModal()" class="btn-secondary flex-1 py-3 text-white rounded-xl font-medium transition-all">Cancel</button>
                <button onclick="applyCrop()" class="btn-primary flex-1 py-3 text-white rounded-xl font-medium transition-all">Save Avatar</button>
            </div>
        </div>
    </div>

    <div id="statsWindow" class="stats-window" onclick="event.stopPropagation()">
        <div class="stats-header">
            <div class="stats-title">
                <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M22 12h-4l-3 9L9 3l-3 9H2"/></svg>
                Connection Stats
            </div>
            <div class="stats-close" onclick="toggleStatsWindow()">
                <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg>
            </div>
        </div>
        <div class="stats-content">
            <div class="stats-section">
                <div class="stats-section-title">
                    <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"/><path d="M12 2a14.5 14.5 0 0 0 0 20"/><path d="M2 12h20"/></svg>
                    Network
                </div>
                <div class="stats-grid">
                    <div class="stat-item">
                        <span class="stat-label">Ping</span>
                        <span id="statPing" class="stat-value">--</span>
                    </div>
                    <div class="stat-item">
                        <span class="stat-label">Jitter</span>
                        <span id="statJitter" class="stat-value">--</span>
                    </div>
                </div>
                <div class="stats-row mt-3">
                    <span class="stats-row-label">Low Bandwidth Mode</span>
                    <span id="statLowBandwidth" class="stats-row-value text-zinc-400 font-normal">Disabled</span>
                </div>
            </div>

            <div class="stats-section">
                <div class="stats-section-title">
                    <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect width="18" height="18" x="3" y="3" rx="2"/><path d="M8.5 8.5v.01"/><path d="M15.5 8.5v.01"/><path d="M8.5 15.5v.01"/><path d="M15.5 15.5v.01"/><path d="M3 12h18"/><path d="M12 3v18"/></svg>
                    Video
                </div>
                <div class="stats-row">
                    <span class="stats-row-label">Resolution</span>
                    <span id="statVideoRes" class="stats-row-value">--</span>
                </div>
                <div class="stats-row">
                    <span class="stats-row-label">Bitrate</span>
                    <span id="statVideoBitrate" class="stats-row-value">--</span>
                </div>
                <div class="stats-row">
                    <span class="stats-row-label">Codec</span>
                    <span id="statVideoCodec" class="stats-row-value">--</span>
                </div>
                <div class="stats-row">
                    <span class="stats-row-label">Frames</span>
                    <span id="statVideoFrames" class="stats-row-value">--</span>
                </div>
            </div>

            <div class="stats-section">
                <div class="stats-section-title">
                    <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M2 12h20"/><path d="M2 12v6a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2v-6"/><path d="M12 2v10"/><path d="m8 8 4-4 4 4"/></svg>
                    Audio
                </div>
                <div class="stats-row">
                    <span class="stats-row-label">Bitrate</span>
                    <span id="statAudioBitrate" class="stats-row-value">--</span>
                </div>
                <div class="stats-row">
                    <span class="stats-row-label">Codec</span>
                    <span id="statAudioCodec" class="stats-row-value">--</span>
                </div>
            </div>

            <div class="stats-section">
                <div class="stats-section-title">
                    <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z"/></svg>
                    Packets
                </div>
                <div class="stats-row">
                    <span class="stats-row-label">Sent</span>
                    <span id="statPacketsSent" class="stats-row-value">--</span>
                </div>
                <div class="stats-row">
                    <span class="stats-row-label">Received</span>
                    <span id="statPacketsReceived" class="stats-row-value">--</span>
                </div>
                <div class="stats-row">
                    <span class="stats-row-label">Lost</span>
                    <span id="statPacketsLost" class="stats-row-value">--</span>
                </div>
            </div>
        </div>
        <div class="stats-refresh">Updates every 2 seconds</div>
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
    pub is_gif: bool,
    pub static_frame: Option<String>,
    pub is_muted: bool,
    pub is_deafened: bool,
    pub is_screen_sharing: bool,
    #[serde(default)]
    pub is_low_bandwidth_mode: bool,
    #[serde(default)]
    pub is_on_the_go_mode: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RoomStatus {
    name: String,
    users: HashMap<String, UserStatus>,
    #[serde(default)]
    pub created_at: u64,
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
type ChannelCreationTimesMap = Arc<Mutex<HashMap<String, HashMap<String, u64>>>>;
const ROOM_EMPTY_GRACE_SECS: u64 = 120;
const MAX_ROOM_ID_LEN: usize = 64;
const MAX_CHANNEL_ID_LEN: usize = 32;
const MAX_NICKNAME_LEN: usize = 32;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ClusterMessage {
    #[serde(rename = "type")]
    msg_type: String,
    room_id: String,
    channel_id: String,
    user_id: String,
    msg_id: String,
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
    channel_creation_times: ChannelCreationTimesMap,
    cluster_key: Option<String>,
    cluster_scheme: String,
    allowed_url: Option<String>,
    connected_peers: Arc<Mutex<HashSet<String>>>,
    pub recent_cluster_msg_ids: Arc<Mutex<HashSet<String>>>,
    pub cluster_msg_history: Arc<Mutex<VecDeque<String>>>,
}

#[tokio::main]
async fn main() {
    let rooms: RoomMap = Arc::new(Mutex::new(HashMap::new()));
    let room_cleanup_generations: RoomCleanupMap = Arc::new(Mutex::new(HashMap::new()));
    let channel_creation_times: ChannelCreationTimesMap = Arc::new(Mutex::new(HashMap::new()));

    let room_creation_password = std::env::var("ROOM_CREATION_PASSWORD").ok().map(|p| p.trim().to_string()).filter(|s| !s.is_empty());
    let cluster_key = std::env::var("KEY").ok().map(|k| k.trim().to_string()).filter(|s| !s.is_empty());
    let cluster_scheme = std::env::var("CLUSTER_SCHEME").ok()
        .map(|s| s.trim().to_lowercase())
        .filter(|s| s == "wss")
        .unwrap_or_else(|| "ws".to_string());
    let allowed_url = std::env::var("URL").ok().map(|u| {
        let u = u.trim();
        let without_scheme = u.strip_prefix("https://")
            .or_else(|| u.strip_prefix("http://"))
            .unwrap_or(u);
        let host = without_scheme.trim_end_matches('/');
        let host = host.split('/').next().unwrap_or(host);
        let host = host.split(':').next().unwrap_or(host);
        host.to_string()
    }).filter(|s| !s.is_empty());
    let (cluster_tx, _) = tokio::sync::broadcast::channel::<String>(10000);
    let remote_users: RemoteUsersMap = Arc::new(Mutex::new(HashMap::new()));

    if cluster_key.is_some() {
        println!("CLUSTER: Enabled via KEY env var (DHT discovery, scheme: {})", cluster_scheme);
        if cluster_scheme == "ws" {
            eprintln!("WARNING: CLUSTER: Using unencrypted ws:// for inter-instance traffic.");
            eprintln!("WARNING: Set CLUSTER_SCHEME=wss and put a TLS-terminating proxy in front of cluster-ws if exposing over untrusted networks.");
        }
    }

    if let Some(ref url) = allowed_url {
        println!("URL RESTRICTION: Enabled - only allowing access from {}", url);
    }

    let state = AppState {
        rooms,
        room_cleanup_generations,
        room_creation_password,
        cluster_tx,
        remote_users,
        channel_creation_times,
        cluster_key,
        cluster_scheme,
        allowed_url,
        connected_peers: Arc::new(Mutex::new(HashSet::new())),
        recent_cluster_msg_ids: Arc::new(Mutex::new(HashSet::new())),
        cluster_msg_history: Arc::new(Mutex::new(VecDeque::new())),
    };

    let app = Router::new()
        .route("/", get(index))
        .route("/new", get(new_room))
        .route("/new/", get(redirect_new_trailing_slash))
        .route("/{room_id}", get(index))
        .route("/{room_id}/", get(redirect_room_trailing_slash))
        .route("/{room_id}/{channel_id}", get(index))
        .route("/{room_id}/{channel_id}/", get(redirect_channel_trailing_slash))
        .route("/{room_id}/{channel_id}/status", get(channel_status))
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
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Redirect, (axum::http::StatusCode, &'static str)> {
    if let Some(ref allowed_url) = state.allowed_url {
        let host = headers.get("host")
            .and_then(|v| v.to_str().ok())
            .map(|h| h.split(':').next().unwrap_or(h));
        match host {
            Some(h) if h == allowed_url => {},
            _ => return Err((axum::http::StatusCode::FORBIDDEN, "Forbidden")),
        }
    }
    if let Some(ref required_pass) = state.room_creation_password {
        match params.get("password") {
            Some(p) if p == required_pass => {},
            _ => return Err((axum::http::StatusCode::UNAUTHORIZED, "Unauthorized")),
        }
    }

    let room_id = if let Some(custom_name) = params.get("name") {
        if custom_name.is_empty() {
            Uuid::new_v4().to_string()
        } else {
            // Validate custom room name: alphanumeric, hyphens, underscores only, max length
            let trimmed = custom_name.trim();
            if trimmed.len() > MAX_ROOM_ID_LEN || trimmed.is_empty() || !trimmed.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
                return Err((axum::http::StatusCode::BAD_REQUEST, "Invalid room name: use only letters, numbers, hyphens, and underscores (max 64 characters)"));
            }
            trimmed.to_string()
        }
    } else {
        Uuid::new_v4().to_string()
    };

    Ok(Redirect::to(&format!("/{}", room_id)))
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

async fn index(State(state): State<AppState>, headers: axum::http::HeaderMap) -> axum::response::Response {
    if let Some(ref allowed_url) = state.allowed_url {
        let host = headers.get("host")
            .and_then(|v| v.to_str().ok())
            .map(|h| h.split(':').next().unwrap_or(h));
        match host {
            Some(h) if h == allowed_url => {},
            _ => return (axum::http::StatusCode::FORBIDDEN, "Forbidden").into_response(),
        }
    }

    let turn_url = std::env::var("TURN_URL").unwrap_or_default();
    let turn_username = std::env::var("TURN_USERNAME").unwrap_or_default();
    let turn_credential = std::env::var("TURN_CREDENTIAL").unwrap_or_default();

    let html = get_html_page(&turn_url, &turn_username, &turn_credential);
    (
        [(
            header::CONTENT_SECURITY_POLICY,
            "default-src 'self'; script-src 'self' 'unsafe-inline' 'wasm-unsafe-eval'; script-src-elem 'self' 'unsafe-inline'; worker-src 'self' blob:; style-src 'self' 'unsafe-inline'; font-src 'self'; img-src 'self' data: https: blob:; connect-src 'self' wss: ws:; media-src 'self' blob:; object-src 'none'; frame-ancestors 'none';"
        )],
        Html(html)
    ).into_response()
}

async fn ws_handler(
    Path((room_id, channel_id)): Path<(String, String)>,
    Query(_params): Query<HashMap<String, String>>,
    ws: WebSocketUpgrade,
    headers: axum::http::HeaderMap,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let mut channel_id = channel_id.chars().take(MAX_CHANNEL_ID_LEN).collect::<String>();
    if channel_id.eq_ignore_ascii_case("general") {
        channel_id = "General".to_string();
    }
    if room_id.len() > MAX_ROOM_ID_LEN {
        return (axum::http::StatusCode::BAD_REQUEST, "Room ID too long").into_response();
    }
    if let Some(ref allowed_url) = state.allowed_url {
        let host = headers.get("host")
            .and_then(|v| v.to_str().ok())
            .map(|h| h.split(':').next().unwrap_or(h));
        match host {
            Some(h) if h == allowed_url => {},
            _ => return (axum::http::StatusCode::FORBIDDEN, "Forbidden").into_response(),
        }
    }
    if let (Some(origin), Some(host)) = (headers.get("origin"), headers.get("host")) {
        if let (Ok(origin_str), Ok(host_str)) = (origin.to_str(), host.to_str()) {
             // Prevent bypass: "evil-example.com" must not match "example.com"
             let origin_host = origin_str.strip_prefix("https://")
                 .or_else(|| origin_str.strip_prefix("http://"))
                 .unwrap_or(origin_str)
                 .split('/').next().unwrap_or(origin_str)
                 .split(':').next().unwrap_or(origin_str);
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
        client_ip = forwarded_for.to_str().unwrap_or("").split(',').next().unwrap_or("").trim().to_string();
    }

    ws.max_message_size(8 * 1024 * 1024)
        .on_upgrade(move |socket| handle_socket(socket, room_id, channel_id, state, client_ip))
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
    ws.max_message_size(8 * 1024 * 1024)
        .on_upgrade(move |socket| handle_inbound_cluster(socket, state))
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
                        msg_id: Uuid::new_v4().to_string(),
                        status: Some(status.clone()),
                        data: Some(serde_json::json!({
                            "nickname": status.nickname,
                            "avatar": status.avatar,
                            "isMuted": status.is_muted,
                            "isDeafened": status.is_deafened,
                            "screenEnabled": status.is_screen_sharing,
                            "isLowBandwidthMode": status.is_low_bandwidth_mode,
                            "isOnTheGoMode": status.is_on_the_go_mode
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
                handle_cluster_message(&cm, &rooms, &remote_users, &state).await;
            }
        }
    }

    forwarder.abort();
    writer.abort();
    let dead = peer_users_cleanup.lock().await.clone();
    cleanup_dead_remote_users(&dead, &rooms, &remote_users, &state.channel_creation_times, &state.cluster_tx).await;
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
                    let addr_str_clean = addr_str.trim().to_string();
                    println!("CLUSTER: Discovered new peer: {}", addr_str_clean);
                    let state_clone = state.clone();
                    let scheme = state.cluster_scheme.clone();
                    tokio::spawn(async move {
                        let mut target_addr = addr_str_clean.clone();
                        let mut failures = 0u32;
                        loop {
                            let url = format!("{}://{}/cluster-ws", scheme, target_addr);
                            match connect_to_peer(&url, &state_clone).await {
                                Ok(_) => {
                                    println!("CLUSTER: Connection to {} closed", target_addr);
                                    failures = 0;
                                }
                                Err(e) => {
                                    failures += 1;
                                    println!("CLUSTER: Connection to {} failed ({}/3): {}", target_addr, failures, e);
                                    
                                    // NAT Loopback Fallback: If not already 127.0.0.1, try localhost
                                    if !target_addr.starts_with("127.0.0.1") {
                                        if let Some(port_idx) = addr_str_clean.rfind(':') {
                                            let port = &addr_str_clean[port_idx..];
                                            target_addr = format!("127.0.0.1{}", port);
                                            println!("CLUSTER: NAT Loopback? Retrying with local fallback: {}", target_addr);
                                            continue;
                                        }
                                    }

                                    if failures >= 3 {
                                        println!("CLUSTER: Giving up on {} (will retry if re-discovered)", addr_str_clean);
                                        break;
                                    }
                                }
                            }
                            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                        }

                        state_clone.connected_peers.lock().await.remove(&addr_str_clean);
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
                        msg_id: Uuid::new_v4().to_string(),
                        status: Some(status.clone()),
                        data: Some(serde_json::json!({
                            "nickname": status.nickname,
                            "avatar": status.avatar,
                            "isMuted": status.is_muted,
                            "isDeafened": status.is_deafened,
                            "screenEnabled": status.is_screen_sharing,
                            "isLowBandwidthMode": status.is_low_bandwidth_mode,
                            "isOnTheGoMode": status.is_on_the_go_mode
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
                handle_cluster_message(&cm, &rooms, &remote_users, state).await;
            }
        }
    }

    forwarder.abort();
    writer.abort();
    let dead = peer_users_cleanup.lock().await.clone();
    cleanup_dead_remote_users(&dead, &rooms, &remote_users, &state.channel_creation_times, &state.cluster_tx).await;
    Ok(())
}

async fn cleanup_dead_remote_users(
    dead: &HashSet<(String, String, String)>,
    rooms: &RoomMap,
    remote_users: &RemoteUsersMap,
    times: &ChannelCreationTimesMap,
    _cluster_tx: &tokio::sync::broadcast::Sender<String>,
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
        broadcast_channel_list(rooms, remote_users, times, room_id).await;
    }
}

async fn handle_cluster_message(msg: &ClusterMessage, rooms: &RoomMap, remote_users: &RemoteUsersMap, state: &AppState) {
    {
        let mut ids = state.recent_cluster_msg_ids.lock().await;
        if ids.contains(&msg.msg_id) {
            return;
        }
        ids.insert(msg.msg_id.clone());
        
        let mut history = state.cluster_msg_history.lock().await;
        history.push_back(msg.msg_id.clone());
        if history.len() > 1000 {
            if let Some(oldest) = history.pop_front() {
                ids.remove(&oldest);
            }
        }
    }

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
                broadcast_channel_list(rooms, remote_users, &state.channel_creation_times, &msg.room_id).await;
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
            broadcast_channel_list(rooms, remote_users, &state.channel_creation_times, &msg.room_id).await;
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
                broadcast_channel_list(rooms, remote_users, &state.channel_creation_times, &msg.room_id).await;
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
                broadcast_channel_list(rooms, remote_users, &state.channel_creation_times, &msg.room_id).await;
            }
        }
        "rename-channel" => {
            if let Some(ref data) = msg.data {
                let new_name = data.get("newName").and_then(|v| v.as_str()).unwrap_or("").to_string();
                if !new_name.is_empty() {
                    let old_name = msg.channel_id.clone();
                    let rename_notify = serde_json::to_string(&SignalMessage {
                        msg_type: "rename-channel".into(),
                        user_id: Some(msg.user_id.clone()),
                        target: None,
                        data: Some(serde_json::json!({
                            "roomId": msg.room_id,
                            "oldName": old_name,
                            "newName": new_name,
                        })),
                    }).unwrap();

                    let mut rl = remote_users.lock().await;
                    if let Some(room) = rl.get_mut(&msg.room_id) {
                        if let Some(channel_data) = room.remove(&msg.channel_id) {
                            room.insert(new_name.clone(), channel_data);
                        }
                    }
                    drop(rl);

                    // Forward rename-channel to local WebSocket clients in this room
                    let rooms_lock = rooms.lock().await;
                    if let Some(room) = rooms_lock.get(&msg.room_id) {
                        for (_ch_name, channel) in room.iter() {
                            for (_uid, (tx, _)) in channel.iter() {
                                let _ = tx.try_send(Ok(Message::Text(rename_notify.clone().into())));
                            }
                        }
                    }
                    drop(rooms_lock);

                    broadcast_channel_list(rooms, remote_users, &state.channel_creation_times, &msg.room_id).await;
                }
            }
        }
        "delete-channel" => {
            let mut rl = remote_users.lock().await;
            if let Some(room) = rl.get_mut(&msg.room_id) {
                room.remove(&msg.channel_id);
            }
            drop(rl);
            broadcast_channel_list(rooms, remote_users, &state.channel_creation_times, &msg.room_id).await;
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
    let mut msg_with_id = msg.clone();
    if msg_with_id.msg_id.is_empty() {
        msg_with_id.msg_id = Uuid::new_v4().to_string();
    }
    if let Ok(json) = serde_json::to_string(&msg_with_id) {
        let _ = cluster_tx.send(json);
    }
}

async fn broadcast_channel_list(rooms: &RoomMap, remote_users: &RemoteUsersMap, times: &ChannelCreationTimesMap, room_id: &str) {
    let rooms_lock = rooms.lock().await;
    let remote_lock = remote_users.lock().await;
    let times_lock = times.lock().await;

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
            let created_at = times_lock.get(room_id)
                .and_then(|t| t.get(cid))
                .copied()
                .unwrap_or(0);
            channel_list.insert(cid.clone(), RoomStatus {
                name: cid.clone(),
                users: user_map,
                created_at,
            });
        }
    }

    if let Some(remote_room) = remote_room {
        for (cid, users) in remote_room.iter() {
            let created_at = times_lock.get(room_id)
                .and_then(|t| t.get(cid))
                .copied()
                .unwrap_or(0);
            let entry = channel_list.entry(cid.clone()).or_insert_with(|| RoomStatus {
                name: cid.clone(),
                users: HashMap::new(),
                created_at,
            });
            for (user_id, status) in users.iter() {
                entry.users.insert(user_id.clone(), status.clone());
            }
        }
    }

    let msg = serde_json::to_string(&SignalMessage {
        msg_type: "room-list".into(),
        target: None,
        user_id: None,
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

async fn handle_socket(socket: WebSocket, room_id: String, channel_id: String, state: AppState, _client_ip: String) {
    let rooms = state.rooms.clone();
    let remote_users = state.remote_users.clone(); // Added remote_users clone
    let cluster_tx = state.cluster_tx.clone(); // Added cluster_tx clone
    let room_cleanup_generations = state.room_cleanup_generations.clone();
    let (mut user_ws_tx, mut user_ws_rx) = socket.split();
    let (tx, mut rx) = tokio::sync::mpsc::channel(5000);

    let mut user_id = String::new();
    let mut is_joined = false;

    // Server-side ping to detect dead iOS Safari connections
    let tx_ping = tx.clone();
    let (ping_shutdown_tx, mut ping_shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let last_activity = Arc::new(tokio::sync::Mutex::new(std::time::Instant::now()));
    let last_activity_writer = last_activity.clone();

    tokio::spawn(async move {
        while let Some(result) = rx.recv().await {
            if let Ok(msg) = result {
                if user_ws_tx.send(msg).await.is_err() {
                    break;
                }
            }
        }
    });

    // Server-side ping task: sends a ping every 5s, closes connection after 10s of silence
    let tx_for_ping = tx_ping.clone();
    let last_activity_for_ping = last_activity.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));
        interval.tick().await; // skip first immediate tick
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    let elapsed = last_activity_for_ping.lock().await.elapsed();
                    if elapsed > std::time::Duration::from_secs(10) {
                        // No activity for 10s, client is likely dead (iOS Safari silent drop)
                        let _ = tx_for_ping.try_send(Ok(Message::Close(Some(CloseFrame {
                            code: 4001,
                            reason: "Inactivity timeout".into(),
                        }))));
                        break;
                    }
                    // Send server-side keepalive
                    let ping_msg = serde_json::to_string(&SignalMessage {
                        msg_type: "keepalive".into(),
                        user_id: None,
                        target: None,
                        data: None,
                    }).unwrap();
                    if tx_for_ping.try_send(Ok(Message::Text(ping_msg.into()))).is_err() {
                        break;
                    }
                }
                _ = &mut ping_shutdown_rx => {
                    break;
                }
            }
        }
    });

    while let Some(result) = user_ws_rx.next().await {
        // Update last activity timestamp on any received message
        *last_activity_writer.lock().await = std::time::Instant::now();
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
                                .chars()
                                .take(MAX_NICKNAME_LEN)
                                .collect::<String>();

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

                            let is_low_bandwidth_mode = parsed.data.as_ref()
                                .and_then(|d| d.get("isLowBandwidthMode"))
                                .and_then(|v| v.as_bool())
                                .unwrap_or(false);

                            let is_on_the_go_mode = parsed.data.as_ref()
                                .and_then(|d| d.get("isOnTheGoMode"))
                                .and_then(|v| v.as_bool())
                                .unwrap_or(false);

                            if let Some(ref a) = avatar {
                                if a.len() > 7_000_000 {
                                    avatar = None;
                                }
                            }

                            let is_gif = parsed.data.as_ref()
                                .and_then(|d| d.get("isGif"))
                                .and_then(|v| v.as_bool())
                                .unwrap_or(false);

                            let static_frame = parsed.data.as_ref()
                                .and_then(|d| d.get("staticFrame"))
                                .and_then(|v| v.as_str())
                                .filter(|s| s.len() <= 7_000_000)
                                .map(|s| s.to_string());

                            {
                                let room_needs_password = if let Some(ref required_pass) = state.room_creation_password {
                                    let exists_locally = rooms.lock().await.contains_key(&room_id);
                                    if exists_locally {
                                        false
                                    } else {
                                        let exists_remotely = remote_users.lock().await.contains_key(&room_id);
                                        if exists_remotely {
                                            false
                                        } else {
                                            let pass_match = if let Some(ref data) = parsed.data {
                                                data.get("password")
                                                    .and_then(|v| v.as_str())
                                                    .map(|p| p == required_pass)
                                                    .unwrap_or(false)
                                            } else {
                                                false
                                            };
                                            !pass_match
                                        }
                                    }
                                } else {
                                    false
                                };

                                if room_needs_password {
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

                                let mut rooms_lock = rooms.lock().await;

                                let room = rooms_lock.entry(room_id.clone()).or_insert_with(HashMap::new);
                                room.entry("General".to_string()).or_insert_with(HashMap::new);
                                let channel = room.entry(channel_id.clone()).or_insert_with(HashMap::new);

                                {
                                    let mut times = state.channel_creation_times.lock().await;
                                    let room_times = times.entry(room_id.clone()).or_insert_with(HashMap::new);
                                    room_times.entry("General".to_string()).or_insert_with(|| {
                                        std::time::SystemTime::now()
                                            .duration_since(std::time::UNIX_EPOCH)
                                            .unwrap()
                                            .as_secs()
                                    });
                                    room_times.entry(channel_id.clone()).or_insert_with(|| {
                                        std::time::SystemTime::now()
                                            .duration_since(std::time::UNIX_EPOCH)
                                            .unwrap()
                                            .as_secs()
                                    });
                                }

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
                                    is_gif,
                                    static_frame: static_frame.clone(),
                                    is_muted,
                                    is_deafened,
                                    is_screen_sharing,
                                    is_low_bandwidth_mode,
                                    is_on_the_go_mode,
                                }));
                             }

                            if room_cleanup_generations.lock().await.remove(&room_id).is_some() {
                                println!("CLEANUP: Canceled pending deletion for room '{}'", room_id);
                            }
                            is_joined = true;

                            // Send the server-assigned userId back to the client
                            let joined_msg = serde_json::to_string(&SignalMessage {
                                msg_type: "joined".into(),
                                user_id: Some(user_id.clone()),
                                target: None,
                                data: None,
                            }).unwrap();
                            let _ = tx.try_send(Ok(Message::Text(joined_msg.into())));

                            {
                                let mut existing_users: Vec<serde_json::Value> = Vec::new();
                                let mut seen_ids = HashSet::new();
                                seen_ids.insert(user_id.clone());
                                {
                                    let rooms_lock = rooms.lock().await;
                                    if let Some(room) = rooms_lock.get(&room_id) {
                                        if let Some(channel) = room.get(&channel_id) {
                                            for (uid, (_, status)) in channel.iter() {
                                                if seen_ids.insert(uid.clone()) {
                                                    existing_users.push(serde_json::json!({
                                                        "id": uid,
                                                        "status": {
                                                            "nickname": status.nickname,
                                                            "avatar": status.avatar,
                                                            "isGif": status.is_gif,
                                                            "staticFrame": status.static_frame,
                                                            "isMuted": status.is_muted,
                                                            "isDeafened": status.is_deafened,
                                                            "isScreenSharing": status.is_screen_sharing,
                                                            "isLowBandwidthMode": status.is_low_bandwidth_mode,
                                                            "isOnTheGoMode": status.is_on_the_go_mode
                                                        }
                                                    }));
                                                }
                                            }
                                        }
                                    }
                                }
                                {
                                    let remote_lock = remote_users.lock().await;
                                    if let Some(remote_room) = remote_lock.get(&room_id) {
                                        if let Some(remote_channel) = remote_room.get(&channel_id) {
                                            for (uid, status) in remote_channel.iter() {
                                                if seen_ids.insert(uid.clone()) {
                                                    existing_users.push(serde_json::json!({
                                                        "id": uid,
                                                        "status": {
                                                            "nickname": status.nickname,
                                                            "avatar": status.avatar,
                                                            "isGif": status.is_gif,
                                                            "staticFrame": status.static_frame,
                                                            "isMuted": status.is_muted,
                                                            "isDeafened": status.is_deafened,
                                                            "isScreenSharing": status.is_screen_sharing,
                                                            "isLowBandwidthMode": status.is_low_bandwidth_mode,
                                                            "isOnTheGoMode": status.is_on_the_go_mode
                                                        }
                                                    }));
                                                }
                                            }
                                        }
                                    }
                                }
                                let existing_users_msg = serde_json::to_string(&SignalMessage {
                                    msg_type: "existing-users".into(),
                                    user_id: None,
                                    target: None,
                                    data: Some(serde_json::json!({ "users": existing_users })),
                                }).unwrap();
                                let _ = tx.try_send(Ok(Message::Text(existing_users_msg.into())));
                            }

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
                            cluster_broadcast(&cluster_tx, &ClusterMessage {
                                msg_type: "user-joined".into(),
                                room_id: room_id.clone(),
                                channel_id: channel_id.clone(),
                                user_id: user_id.clone(),
                                msg_id: Uuid::new_v4().to_string(),
                                status: Some(UserStatus {
                                    nickname: nickname.clone(),
                                    avatar: avatar.clone(),
                                    is_muted,
                                    is_deafened,
                                    is_screen_sharing,
                                    is_gif,
                                    static_frame: static_frame.clone(),
                                    is_low_bandwidth_mode,
                                    is_on_the_go_mode,
                                }),
                                data: notify_data.clone(),
                                signal_msg: None,
                            });
                            broadcast_channel_list(&rooms, &remote_users, &state.channel_creation_times, &room_id).await;
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
                                                    status.nickname = n.chars().take(MAX_NICKNAME_LEN).collect();
                                                }
                                                if let Some(a) = d.get("avatar") {
                                                    if a.is_null() {
                                                        status.avatar = None;
                                                        status.is_gif = false;
                                                        status.static_frame = None;
                                                    } else if let Some(a_str) = a.as_str() {
                                                        if a_str.len() <= 7_000_000 {
                                                            status.avatar = Some(a_str.to_string());
                                                        }
                                                    }
                                                }
                                                if let Some(g) = d.get("isGif").and_then(|v| v.as_bool()) {
                                                    status.is_gif = g;
                                                }
                                                if d.contains_key("staticFrame") {
                                                    let sf = d.get("staticFrame").and_then(|v| v.as_str())
                                                        .filter(|s| s.len() <= 7_000_000)
                                                        .map(|s| s.to_string());
                                                    if sf.is_some() {
                                                        status.static_frame = sf;
                                                    } else if d.get("staticFrame").map_or(false, |v| v.is_null()) {
                                                        status.static_frame = None;
                                                    }
                                                }
                                                if let Some(m) = d.get("isMuted").and_then(|v| v.as_bool()) {
                                                    status.is_muted = m;
                                                }
                                                if let Some(d) = d.get("isDeafened").and_then(|v| v.as_bool()) {
                                                    status.is_deafened = d;
                                                }
                                                if let Some(lbm) = d.get("isLowBandwidthMode").and_then(|v| v.as_bool()) {
                                                    status.is_low_bandwidth_mode = lbm;
                                                }
                                                if let Some(otg) = d.get("isOnTheGoMode").and_then(|v| v.as_bool()) {
                                                    status.is_on_the_go_mode = otg;
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

                                        if let Some(ref status) = full_status {
                                            cluster_broadcast(&cluster_tx, &ClusterMessage {
                                                msg_type: "user-update".into(),
                                                room_id: room_id.clone(),
                                                channel_id: channel_id.clone(),
                                                user_id: user_id.clone(),
                                                msg_id: Uuid::new_v4().to_string(),
                                                status: Some(status.clone()),
                                                data: None,
                                                signal_msg: None,
                                            });
                                        }
                                    }
                                }
                            }
                            broadcast_channel_list(&rooms, &remote_users, &state.channel_creation_times, &room_id).await;
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
                            cluster_broadcast(&cluster_tx, &ClusterMessage {
                                msg_type: "cam-toggle".into(),
                                room_id: room_id.clone(),
                                channel_id: channel_id.clone(),
                                user_id: user_id.clone(),
                                msg_id: Uuid::new_v4().to_string(),
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

                            cluster_broadcast(&cluster_tx, &ClusterMessage {
                                msg_type: "screen-toggle".into(),
                                room_id: room_id.clone(),
                                channel_id: channel_id.clone(),
                                user_id: user_id.clone(),
                                msg_id: Uuid::new_v4().to_string(),
                                status: None,
                                data: parsed.data.clone(),
                                signal_msg: None,
                            });
                            broadcast_channel_list(&rooms, &remote_users, &state.channel_creation_times, &room_id).await;
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

                                    cluster_broadcast(&cluster_tx, &ClusterMessage {
                                        msg_type: "user-kicked".into(),
                                        room_id: room_id.clone(),
                                        channel_id: channel_id.clone(),
                                        user_id: kick_uid.clone(),
                                        msg_id: Uuid::new_v4().to_string(),
                                        status: None,
                                        data: None,
                                        signal_msg: None,
                                    });
                                    broadcast_channel_list(&rooms, &remote_users, &state.channel_creation_times, &room_id).await;
                                }
                            }
                        } else if parsed.msg_type == "rename-channel" {
                            let mut target_channel_id = parsed.data.as_ref()
                                .and_then(|d| d.get("channelId"))
                                .and_then(|v| v.as_str())
                                .unwrap_or(&channel_id)
                                .to_string();

                            if target_channel_id.eq_ignore_ascii_case("general") {
                                target_channel_id = "General".to_string();
                            }

                            if target_channel_id != "General" {
                                let new_name = parsed.data.as_ref()
                                    .and_then(|d| d.get("newName"))
                                    .and_then(|v| v.as_str())
                                    .map(|s| s.to_string());

                                if let Some(mut new_name_str) = new_name {
                                    if new_name_str.eq_ignore_ascii_case("general") {
                                        new_name_str = "General".to_string();
                                    }

                                    let mut rooms_lock = rooms.lock().await;

                                    let can_rename = if let Some(room) = rooms_lock.get(&room_id) {
                                        if let Some(target_channel) = room.get(&target_channel_id) {
                                            target_channel.is_empty() && !room.contains_key(&new_name_str)
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

                                         // Broadcast rename-channel to local users in this room
                                         let rename_msg = serde_json::to_string(&SignalMessage {
                                             msg_type: "rename-channel".into(),
                                             user_id: Some(user_id.clone()),
                                             target: None,
                                             data: Some(serde_json::json!({
                                                 "roomId": room_id,
                                                 "oldName": target_channel_id,
                                                 "newName": new_name_str,
                                             })),
                                         }).unwrap();

                                         if let Some(room) = rooms_lock.get(&room_id) {
                                             for (_ch_name, channel) in room.iter() {
                                                 for (_uid, (tx, _)) in channel.iter() {
                                                     let _ = tx.try_send(Ok(Message::Text(rename_msg.clone().into())));
                                                 }
                                             }
                                         }

                                         drop(rooms_lock);

                                         // Also rename in remote_users so signal routing stays consistent
                                         {
                                             let mut rl = remote_users.lock().await;
                                             if let Some(room) = rl.get_mut(&room_id) {
                                                 if let Some(channel_data) = room.remove(&target_channel_id) {
                                                     room.insert(new_name_str.clone(), channel_data);
                                                 }
                                             }
                                         }

                                         cluster_broadcast(&cluster_tx, & ClusterMessage {
                                             msg_type: "rename-channel".into(),
                                             room_id: room_id.clone(),
                                             channel_id: target_channel_id.clone(),
                                             user_id: user_id.clone(),
                                             msg_id: Uuid::new_v4().to_string(),
                                             status: None,
                                             data: Some(serde_json::json!({ "roomId": room_id, "oldName": target_channel_id, "newName": new_name_str })),
                                             signal_msg: None,
                                         });
                                         broadcast_channel_list(&rooms, &remote_users, &state.channel_creation_times, &room_id).await;
                                    }
                                }
                            }
                        } else if parsed.msg_type == "delete-channel" {
                            let mut target_channel_id = parsed.data.as_ref()
                                .and_then(|d| d.get("channelId"))
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string();

                            if target_channel_id.eq_ignore_ascii_case("general") {
                                target_channel_id = "General".to_string();
                            }

                            if !target_channel_id.is_empty() && target_channel_id != "General" {
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

                                      cluster_broadcast(&cluster_tx, &ClusterMessage {
                                          msg_type: "delete-channel".into(),
                                          room_id: room_id.clone(),
                                          channel_id: target_channel_id.clone(),
                                          user_id: user_id.clone(),
                                          msg_id: Uuid::new_v4().to_string(),
                                          status: None,
                                          data: None,
                                          signal_msg: None,
                                      });
                                      broadcast_channel_list(&rooms, &remote_users, &state.channel_creation_times, &room_id).await;
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
                                    let rl = remote_users.lock().await;
                                    rl.get(&room_id)
                                        .and_then(|r| r.get(&channel_id))
                                        .map(|c| c.contains_key(target_id))
                                        .unwrap_or(false)
                                };
                                if is_remote {
                                    let mut forwarded_msg = parsed.clone();
                                    forwarded_msg.user_id = Some(user_id.clone());
                                    let forwarded_text = serde_json::to_string(&forwarded_msg).unwrap();
                                    cluster_broadcast(&cluster_tx, &ClusterMessage {
                                        msg_type: "signal".into(),
                                        room_id: room_id.clone(),
                                        channel_id: channel_id.clone(),
                                        user_id: user_id.clone(),
                                        msg_id: Uuid::new_v4().to_string(),
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

    // Stop the server-side ping task
    let _ = ping_shutdown_tx.send(());

    let mut actually_removed = false;
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

                if removed {
                    actually_removed = true;
                    schedule_room_cleanup = room.values().all(|c| c.is_empty());
                }
            }
        }
    }

    if schedule_room_cleanup {
        let has_remote = remote_users.lock().await.get(&room_id)
            .map(|r| r.values().any(|c| !c.is_empty()))
            .unwrap_or(false);
        if has_remote {
            schedule_room_cleanup = false;
        }
    }

    if is_joined && actually_removed {
        cluster_broadcast(&state.cluster_tx, &ClusterMessage {
            msg_type: "user-left".into(),
            room_id: room_id.clone(),
            channel_id: channel_id.clone(),
            user_id: user_id.clone(),
            msg_id: Uuid::new_v4().to_string(),
            status: None,
            data: None,
            signal_msg: None,
        });
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
        let remote_users_clone = remote_users.clone();
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
                let has_remote = remote_users_clone.lock().await.get(&room_id_clone)
                    .map(|r| r.values().any(|c| !c.is_empty()))
                    .unwrap_or(false);
                if has_remote {
                    false
                } else {
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
                }
            };

            if removed_room {
                let mut cleanup_lock = cleanup_clone.lock().await;
                if cleanup_lock.get(&room_id_clone).copied() == Some(next_generation) {
                    cleanup_lock.remove(&room_id_clone);
                }
                println!("CLEANUP: Removed empty room '{}' after {}s empty", room_id_clone, ROOM_EMPTY_GRACE_SECS);
            } else {
                // Room still has remote users or became non-empty; reschedule cleanup.
                let mut cleanup_lock = cleanup_clone.lock().await;
                if cleanup_lock.get(&room_id_clone).copied() == Some(next_generation) {
                    let next_gen = next_generation + 1;
                    cleanup_lock.insert(room_id_clone.clone(), next_gen);
                    let rooms_retry = rooms_clone.clone();
                    let cleanup_retry = cleanup_clone.clone();
                    let remote_retry = remote_users_clone.clone();
                    let rid_retry = room_id_clone.clone();
                    tokio::spawn(async move {
                        tokio::time::sleep(std::time::Duration::from_secs(ROOM_EMPTY_GRACE_SECS)).await;
                        let gen_current = cleanup_retry.lock().await.get(&rid_retry).copied() == Some(next_gen);
                        if !gen_current { return; }
                        let has_remote = remote_retry.lock().await.get(&rid_retry)
                            .map(|r| r.values().any(|c| !c.is_empty()))
                            .unwrap_or(false);
                        if has_remote {
                            // Still has remote users, clear generation so future activity can re-trigger.
                            let mut cl = cleanup_retry.lock().await;
                            if cl.get(&rid_retry).copied() == Some(next_gen) { cl.remove(&rid_retry); }
                            return;
                        }
                        let removed = {
                            let mut rl = rooms_retry.lock().await;
                            let should = rl.get(&rid_retry).map(|rm| rm.values().all(|c| c.is_empty())).unwrap_or(false);
                            if should { rl.remove(&rid_retry); true } else { false }
                        };
                        if removed {
                            let mut cl = cleanup_retry.lock().await;
                            if cl.get(&rid_retry).copied() == Some(next_gen) { cl.remove(&rid_retry); }
                            println!("CLEANUP: Removed empty room '{}' after rescheduled check", rid_retry);
                        }
                    });
                }
            }
        });
    }
    broadcast_channel_list(&rooms, &remote_users, &state.channel_creation_times, &room_id).await;
}

async fn channel_status(
    Path((room_id, channel_id)): Path<(String, String)>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let mut channel_id = channel_id;
    if channel_id.eq_ignore_ascii_case("general") {
        channel_id = "General".to_string();
    }
    let rooms_lock = state.rooms.lock().await;
    let remote_lock = state.remote_users.lock().await;
    let times_lock = state.channel_creation_times.lock().await;

    let mut users_map = HashMap::new();

    if let Some(room) = rooms_lock.get(&room_id) {
        if let Some(channel) = room.get(&channel_id) {
            for (uid, (_, status)) in channel.iter() {
                users_map.insert(uid.clone(), status.clone());
            }
        }
    }

    if let Some(remote_room) = remote_lock.get(&room_id) {
        if let Some(remote_channel) = remote_room.get(&channel_id) {
            for (uid, status) in remote_channel.iter() {
                users_map.insert(uid.clone(), status.clone());
            }
        }
    }

    let created_at = times_lock.get(&room_id)
        .and_then(|t| t.get(&channel_id))
        .copied()
        .unwrap_or(0);

    axum::Json(RoomStatus {
        name: channel_id,
        users: users_map,
        created_at,
    })
}

