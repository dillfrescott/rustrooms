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
    collections::HashMap,
    sync::Arc,
};
use tokio::sync::Mutex;
use uuid::Uuid;

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
    '/rnnoise_processor.js'
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
    <script src="https://cdn.tailwindcss.com"></script>
    <link href="https://fonts.googleapis.com/css2?family=Inter:wght@400;500;600;700&display=swap" rel="stylesheet">
    <style>
        :root {
            /* Professional monochromatic base with subtle cool tones */
            --bg-primary: #09090b;
            --bg-secondary: #18181b;
            --bg-tertiary: #27272a;
            --bg-elevated: rgba(39, 39, 42, 0.75);
            --bg-elevated-strong: rgba(39, 39, 42, 0.9);
            --border-subtle: rgba(255, 255, 255, 0.06);
            --border-medium: rgba(255, 255, 255, 0.1);
            --border-strong: rgba(255, 255, 255, 0.15);

            /* Refined text hierarchy */
            --text-primary: #fafafa;
            --text-secondary: #a1a1aa;
            --text-muted: #71717a;

            /* Professional accent - understated blue */
            --accent: #3b82f6;
            --accent-hover: #2563eb;
            --accent-glow: rgba(59, 130, 246, 0.25);
            --accent-blue: #3b82f6;
            --accent-dark-blue: #1d4ed8;

            /* Muted status colors */
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

        /* Subtle text rendering for professional appearance */
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
            background: linear-gradient(145deg, var(--bg-secondary) 0%, var(--bg-tertiary) 100%);
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
            content: '';
            position: absolute;
            top: 0;
            left: 0;
            right: 0;
            height: 1px;
            background: linear-gradient(90deg, transparent, rgba(255, 255, 255, 0.1), transparent);
            opacity: 0.6;
            z-index: 10;
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
            background: linear-gradient(145deg, var(--bg-secondary) 0%, var(--bg-tertiary) 100%);
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
            width: 72px;
            height: 72px;
            border-radius: 12px;
            overflow: hidden;
            border: 1px solid var(--border-medium);
            background: linear-gradient(145deg, var(--bg-tertiary), var(--bg-secondary));
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
                width: 96px;
                height: 96px;
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
            background: linear-gradient(135deg, rgba(59, 130, 246, 0.15), rgba(37, 99, 235, 0.05));
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
            z-index: 40;
            transition: all 0.2s cubic-bezier(0.4, 0, 0.2, 1);
            background: linear-gradient(145deg, var(--bg-secondary), var(--bg-tertiary));
        }

        @media (min-width: 768px) {
            .pip-wrapper {
                width: 280px;
                bottom: 240px;
                border-radius: 12px;
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
            margin-right: 10px;
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
            border: 4px solid rgba(16, 185, 129, 0.9) !important;
            box-shadow: 0 0 12px rgba(16, 185, 129, 0.7), 0 0 0 3px rgba(16, 185, 129, 0.5) !important;
            transition: border 0.2s ease-in-out, box-shadow 0.2s ease-in-out;
        }

        #localPipWrapper.speaking-glow {
            box-shadow: 0 0 12px rgba(16, 185, 129, 0.7), 0 0 0 3px rgba(16, 185, 129, 0.5);
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
            background: linear-gradient(90deg, var(--success), #34d399);
            border-radius: 2px;
            transition: width 0.04s linear;
        }

        .taskbar {
            background: var(--bg-elevated-strong);
            border-top: 1px solid var(--border-medium);
            backdrop-filter: blur(32px) saturate(180%) brightness(115%);
            -webkit-backdrop-filter: blur(32px) saturate(180%) brightness(115%);
            padding-bottom: env(safe-area-inset-bottom);
        }

        @media (min-width: 768px) {
            .taskbar {
                padding-bottom: env(safe-area-inset-bottom);
            }
        }

        /* Custom input styling */
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

        /* Button styling */
        .btn-primary {
            background: linear-gradient(135deg, var(--accent) 0%, var(--accent-hover) 100%);
            transition: all 0.2s cubic-bezier(0.4, 0, 0.2, 1);
            border-radius: 10px;
            box-shadow: 0 1px 3px rgba(0, 0, 0, 0.3);
        }
        .btn-primary:hover {
            background: linear-gradient(135deg, var(--accent-hover) 0%, #1d4ed8 100%);
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

        /* Status pill */
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

        /* Improved label styling */
        .label-text {
            color: var(--text-secondary);
            font-size: 0.75rem;
            font-weight: 500;
            letter-spacing: 0.01em;
        }

        /* Empty state */
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

        /* Sidebar Styling */
        #roomSidebar {
            position: fixed;
            left: -320px;
            top: 0;
            bottom: 0;
            width: 320px;
            z-index: 100;
            transition: transform 0.4s cubic-bezier(0.4, 0, 0.2, 1);
            background: rgba(15, 15, 20, 0.8);
            backdrop-filter: blur(24px) saturate(160%);
            -webkit-backdrop-filter: blur(24px) saturate(160%);
            border-right: 1px solid var(--border-medium);
            display: flex;
            flex-direction: column;
        }

        #roomSidebar.open {
            transform: translateX(320px);
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
            background: rgba(59, 130, 246, 0.1);
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


        .sidebar-overlay {
            position: fixed;
            inset: 0;
            background: rgba(0, 0, 0, 0.4);
            backdrop-filter: blur(4px);
            z-index: 90;
            opacity: 0;
            pointer-events: none;
            transition: opacity 0.3s ease;
        }

        .sidebar-overlay.open {
            opacity: 1;
            pointer-events: auto;
        }

        /* Custom Modal Styling */
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
            transform: scale(0.95);
            transition: all 0.3s cubic-bezier(0.34, 1.56, 0.64, 1);
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
    </style>
</head>
<body class="flex flex-col overflow-hidden" style="background-color: var(--bg-primary);">

    <div id="sidebarOverlay" class="sidebar-overlay" onclick="toggleSidebar()"></div>
    
    <div id="roomSidebar">
        <div class="sidebar-header">
            <h2 id="sidebarTitle" class="text-xl font-bold text-white">Channels</h2>
            <button onclick="toggleSidebar()" class="text-zinc-400 hover:text-white transition-colors">
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
                <!-- Data will be injected here -->
            </div>
        </div>
        </div>
    </div>

    <div id="nameModal" class="modal-overlay">
        <div class="modal-content text-center space-y-6">
            <h3 id="modalTitle" class="text-2xl font-bold text-white">Name Channel</h3>
            <div class="space-y-4">
                <input type="text" id="modalInput" placeholder="Enter name..." class="w-full rounded-xl px-4 py-3 text-white transition-all bg-[var(--bg-tertiary)] border border-[var(--border-subtle)] focus:border-[var(--accent)] outline-none">
                <div class="flex gap-3">
                    <button onclick="closeNameModal()" class="btn-secondary flex-1 py-3 text-white rounded-xl font-medium transition-all">Cancel</button>
                    <button id="modalSubmit" class="btn-primary flex-1 py-3 text-white rounded-xl font-medium transition-all">Confirm</button>
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

    <div id="welcomeOverlay" class="fixed inset-0 z-[70] flex flex-col items-center justify-center p-4" style="display: none; background: linear-gradient(180deg, var(--bg-primary) 0%, #0d0d18 100%);">
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

    <div id="configOverlay" class="fixed inset-0 z-[60] flex flex-col items-center justify-center p-4 transition-opacity duration-300 hidden opacity-0" style="background: rgba(5, 5, 8, 0.85); backdrop-filter: blur(16px) saturate(120%);">
        <canvas id="particleCanvasConfig" class="absolute inset-0 pointer-events-none" style="z-index: 1;"></canvas>
        <div id="configPanel" class="glass-panel p-6 md:p-8 rounded-3xl max-w-5xl w-full max-h-[95vh] overflow-y-auto relative z-10">
            <div class="text-center space-y-1 mb-5">
                <h1 class="text-2xl md:text-3xl font-bold tracking-tight" style="color: var(--text-primary);">Setup</h1>
                <p style="color: var(--text-secondary);" class="text-sm font-normal">Configure your stream.</p>
            </div>

            <div class="flex flex-col lg:flex-row gap-6 lg:gap-8">
                <!-- Left: Video Preview -->
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
                        <button onclick="togglePreviewMic()" id="btnPreviewMic" class="btn-secondary flex-1 py-3 text-white rounded-xl font-medium transition-all flex items-center justify-center gap-2">
                            Mute
                        </button>
                        <button onclick="togglePreviewCam()" id="btnPreviewCam" class="btn-secondary flex-1 py-3 text-white rounded-xl font-medium transition-all flex items-center justify-center gap-2">
                            Stop Cam
                        </button>
                    </div>
                </div>

                <!-- Right: Configuration -->
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

    <div id="settingsOverlay" class="fixed inset-0 z-[80] flex items-center justify-center p-4 hidden" style="background: rgba(5, 5, 8, 0.88); backdrop-filter: blur(20px) saturate(140%);">
        <div class="glass-panel p-6 md:p-8 rounded-3xl max-w-5xl w-full max-h-[95vh] overflow-y-auto relative z-10">
             <button onclick="closeSettings()" class="absolute top-5 right-5 transition-colors p-1 rounded-lg hover:bg-white/10" style="color: var(--text-muted);">
                <svg xmlns="http://www.w3.org/2000/svg" width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="18" y1="6" x2="6" y2="18"></line><line x1="6" y1="6" x2="18" y2="18"></line></svg>
            </button>

            <div class="text-center space-y-1 mb-5">
                <h2 class="text-2xl md:text-3xl font-bold tracking-tight" style="color: var(--text-primary);">Settings</h2>
                <p style="color: var(--text-secondary);" class="text-sm font-normal">Update your profile and devices.</p>
            </div>

            <div class="flex flex-col lg:flex-row gap-6 lg:gap-8">
                <!-- Left: Avatar & Nickname -->
                <div class="lg:w-1/2 space-y-4">
                    <div class="flex flex-col items-center gap-4 p-4 rounded-2xl" style="background: var(--bg-secondary); border: 1px solid var(--border-subtle);">
                        <label class="label-text">Avatar</label>
                        <div onclick="document.getElementById('settingsAvatarInput').click()" class="w-32 h-32 rounded-3xl cursor-pointer overflow-hidden flex items-center justify-center transition-all group relative" style="background: var(--bg-tertiary); border: 2px solid var(--border-subtle);">
                            <img id="settingsAvatarPreview" src="" class="hidden w-full h-full object-cover" draggable="false">
                            <span id="settingsAvatarPlaceholder" class="text-6xl" style="color: var(--text-muted);">👤</span>
                             <div class="absolute inset-0 flex items-center justify-center opacity-0 group-hover:opacity-100 transition-opacity text-sm font-semibold" style="background: rgba(0, 0, 0, 0.75); color: var(--text-primary);">Change</div>
                        </div>
                        <input type="file" id="settingsAvatarInput" hidden accept="image/*" onchange="handleSettingsAvatarUpload(this)">
                    </div>

                    <div>
                        <label class="label-text block mb-2">Nickname</label>
                        <input type="text" id="settingsNicknameInput" placeholder="Enter your name" class="w-full rounded-xl px-4 py-2.5 text-white transition-all" style="font-size: 0.875rem;" maxlength="32">
                    </div>
                </div>

                <!-- Right: Device Settings -->
                <div class="lg:w-1/2 space-y-4">
                    <div class="grid grid-cols-1 gap-4">
                         <div>
                            <label class="label-text block mb-2">Microphone</label>
                            <select id="settingsAudioSource" class="w-full rounded-xl px-3 py-2.5 text-sm text-white transition-all">
                            </select>
                            <div class="mic-meter"><div id="settingsMicBar" class="mic-bar"></div></div>
                        </div>
                         <div>
                            <label class="label-text block mb-2">Speaker</label>
                            <div class="flex gap-2">
                                <select id="settingsAudioOutputSource" class="flex-1 min-w-0 rounded-xl px-3 py-2.5 text-sm text-white transition-all">
                                </select>
                                <button onclick="testSpeaker('settingsAudioOutputSource')" class="p-2.5 rounded-xl transition-all" style="background: var(--bg-secondary); color: var(--text-primary);" title="Test Speaker">
                                    <svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5"></polygon><path d="M19.07 4.93a10 10 0 0 1 0 14.14M15.54 8.46a5 5 0 0 1 0 7.07"></path></svg>
                                </button>
                            </div>
                        </div>
                        <div>
                            <label class="label-text block mb-2">Camera</label>
                            <select id="settingsVideoSource" class="w-full rounded-xl px-3 py-2.5 text-sm text-white transition-all">
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
        <div class="flex-none p-4 md:p-5 z-40 flex justify-between items-center">
            <div class="flex items-center gap-3">
                <button id="sidebarToggle" onclick="toggleSidebar()" class="control-btn shadow-xl hidden !w-12 !h-12 md:!w-14 md:!h-14" title="Channels (R)">
                    <svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="3" y1="12" x2="21" y2="12"></line><line x1="3" y1="6" x2="21" y2="6"></line><line x1="3" y1="18" x2="21" y2="18"></line></svg>
                </button>
                <div class="status-pill px-4 py-2 rounded-full flex items-center gap-2">
                    <div id="connectionDot" class="connection-dot"></div>
                    <span id="statusText" class="text-xs md:text-sm font-medium" style="color: var(--text-primary);">Waiting...</span>
                    <button id="btnReconnect" onclick="retryConnection()" class="hidden ml-2 p-1.5 rounded-full transition-all" style="color: var(--text-muted);" title="Retry Connection">
                        <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M21 12a9 9 0 0 0-9-9 9.75 9.75 0 0 0-6.74 2.74L3 8"/><path d="M3 3v5h5"/><path d="M3 12a9 9 0 0 0 9 9 9.75 9.75 0 0 0 6.74-2.74L21 16"/><path d="M16 16h5v5"/></svg>
                    </button>
                </div>
            </div>

            <div id="btnCopy" class="status-pill px-4 py-2 rounded-full cursor-pointer transition-all flex items-center gap-2 hover:border-opacity-30" onclick="copyLink()">
                <span class="text-xs md:text-sm font-medium" style="color: var(--text-primary);">Invite Link</span>
                <svg id="iconCopy" xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect width="14" height="14" x="8" y="8" rx="2" ry="2"/><path d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2"/></svg>
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
                    <div id="localAvatarLayer" class="absolute inset-0 z-20 flex items-center justify-center" style="display: none; background: linear-gradient(145deg, var(--bg-secondary) 0%, #0f0f11 100%);">
                        <img id="localAvatarImg" src="" class="absolute inset-0 w-full h-full object-cover blur-xl opacity-30 hidden" draggable="false">
                        <div class="relative w-14 h-14 md:w-20 md:h-20 rounded-2xl flex items-center justify-center overflow-hidden z-10" style="background: var(--bg-secondary); border: 2px solid var(--border-subtle);">
                             <img id="localAvatarCenterImg" src="" class="w-full h-full object-cover hidden" draggable="false">
                             <div id="localAvatarPlaceholder" class="text-2xl md:text-3xl flex items-center justify-center w-full h-full" style="color: var(--text-muted); line-height: 1;">👤</div>
                        </div>
                    </div>

                    <video id="localVideo" autoplay playsinline muted class="w-full h-full object-cover z-10"></video>
                    <div id="localLabel" class="absolute bottom-2 left-2 px-2.5 py-1 rounded-lg text-[10px] md:text-xs font-medium backdrop-blur-sm z-30" style="background: rgba(0, 0, 0, 0.6); color: var(--text-primary);">
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
                <button class="control-btn" id="btnCam" onclick="toggleCam()" title="Toggle Camera">
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
        // Welcome screen simple particle system (lobby-style)
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

        // Config screen simple particle system (lobby-style)
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
        const wsProtocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
        let wsUrl = roomId ? `${wsProtocol}//${window.location.host}/ws/${roomId}/${channelId}` : '';
        
        let ws;
        let localStream;
        let screenStream;
        let peers = {}; 
        let peerCamStatus = {};
        let peerScreenStatus = {};
        let userNickname = "Guest";
        let userAvatar = null;
        let sidebarOpen = false;
        let globalRoomList = {};
        let isConfigured = false;
        let audioContext;
        let wakeLock = null;
        let currentAudioOutputId = 'default';
        
        // Persistent user ID to prevent duplicates on reconnection
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
        
        // Timer for delayed reconnect status display
        let reconnectStatusTimeout = null;
        const reconnectDelayMs = 5000; // 5 second delay before showing reconnecting

        // WebSocket heartbeat/keep-alive
        let heartbeatInterval = null;
        const heartbeatIntervalMs = 8000; // Send ping every 8 seconds
        const heartbeatTimeoutMs = 5000; // Wait 5 seconds for pong response
        let lastPongTime = Date.now();
        let heartbeatTimeout = null;
        
        const rtcConfig = {
            iceServers: [
                {
                    urls: "{{TURN_URL}}",
                    username: "{{TURN_USERNAME}}",
                    credential: "{{TURN_CREDENTIAL}}"
                }
            ]
        };

        // Calculate exponential backoff with jitter to prevent thundering herd
        function getReconnectDelay(attempt) {
            const exponentialDelay = Math.min(
                baseReconnectionDelay * Math.pow(2, attempt),
                maxReconnectionDelay
            );
            // Add jitter: ±25% of the delay
            const jitter = exponentialDelay * 0.25 * (Math.random() * 2 - 1);
            return Math.max(exponentialDelay + jitter, baseReconnectionDelay);
        }

        function startHeartbeat() {
            stopHeartbeat(); // Clear any existing heartbeat
            lastPongTime = Date.now();

            heartbeatInterval = setInterval(() => {
                if (ws && ws.readyState === WebSocket.OPEN) {
                    // Send ping
                    ws.send(JSON.stringify({ type: 'ping' }));

                    // Set timeout to detect missing pong
                    heartbeatTimeout = setTimeout(() => {
                        const timeSincePong = Date.now() - lastPongTime;
                        if (timeSincePong > heartbeatIntervalMs + heartbeatTimeoutMs) {
                            console.warn('Heartbeat timeout - no pong received, closing connection');
                            ws.close();
                        }
                    }, heartbeatTimeoutMs);
                }
            }, heartbeatIntervalMs);
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
        }

        function handlePong() {
            lastPongTime = Date.now();
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
            
            loadPreferences();
            try {
                localStream = await navigator.mediaDevices.getUserMedia({ audio: true, video: true });
                previewVideo.srcObject = localStream;
                document.getElementById('previewPlaceholder').style.display = 'none';
                updatePreviewButtons();
                await new Promise(r => setTimeout(r, 500));
                await populateDeviceList();
                navigator.mediaDevices.ondevicechange = populateDeviceList;

                await startPreview();

            } catch (e) {
                console.warn("Device access failed", e);
                try {
                    localStream = await navigator.mediaDevices.getUserMedia({ audio: true, video: false });
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

                if (activeAudioId && [...audioSelect.options].some(o => o.value === activeAudioId)) {
                    audioSelect.value = activeAudioId;
                }

                if (currentAudioOutput && [...audioOutputSelect.options].some(o => o.value === currentAudioOutput)) {
                    audioOutputSelect.value = currentAudioOutput;
                }
                
                if (activeVideoId && [...videoSelect.options].some(o => o.value === activeVideoId)) {
                    videoSelect.value = activeVideoId;
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
                
                 if (activeAudioId && [...settingsAudio.options].some(o => o.value === activeAudioId)) {
                    settingsAudio.value = activeAudioId;
                }

                if (activeAudioOutputId && [...settingsAudioOutput.options].some(o => o.value === activeAudioOutputId)) {
                    settingsAudioOutput.value = activeAudioOutputId;
                }
                
                if (activeVideoId && [...settingsVideo.options].some(o => o.value === activeVideoId)) {
                    settingsVideo.value = activeVideoId;
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
                     try { await audioContext.audioWorklet.addModule('/rnnoise_processor.js'); } catch (err) { console.error("Failed to load rnnoise_processor in switchMediaStream", err); }
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
                     
                     setupAudioMonitor(localStream, 'local');
                     setupVolumeMeter(localStream, 'settingsMicBar');
                     
                 } catch (e) {
                     console.error("Audio switch failed", e);
                     alert("Failed to switch microphone: " + e.message);
                 }
             }
             
             if (videoId && videoId !== currentVideoId) {
                 try {
                     if (currentVideoTrack) {
                         currentVideoTrack.stop();
                         localStream.removeTrack(currentVideoTrack);
                     }
                     
                     await new Promise(r => setTimeout(r, 200));

                     const constraints = { video: { deviceId: { exact: videoId } } };
                     const stream = await navigator.mediaDevices.getUserMedia(constraints);
                     const newTrack = stream.getVideoTracks()[0];
                     
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

                 } catch (e) {
                     console.error("Video switch failed", e);
                 }
             }
             
             updateLocalAvatar();
        }

        function setupAudioMonitor(stream, targetId) {
            if (!audioContext) return;
            if (!stream.getAudioTracks().length) return;
            
            if (audioContext.state === 'suspended') {
                audioContext.resume();
            }

            const source = audioContext.createMediaStreamSource(stream);
            const analyser = audioContext.createAnalyser();
            analyser.fftSize = 256;
            source.connect(analyser);
            
            const bufferLength = analyser.frequencyBinCount;
            const dataArray = new Uint8Array(bufferLength);
            
            function checkAudio() {
                if (targetId !== 'local' && !document.getElementById(targetId)) return;
                
                analyser.getByteFrequencyData(dataArray);
                let sum = 0;
                for(let i = 0; i < bufferLength; i++) {
                    sum += dataArray[i];
                }
                const average = sum / bufferLength;
                
                let targetEl;
                if (targetId === 'local') {
                    targetEl = document.getElementById('localPipWrapper');
                } else {
                    const wrapper = document.getElementById(targetId);
                    if (wrapper) targetEl = wrapper.querySelector('.avatar-center');
                }
                
                if (targetEl) {
                    if (average > 10) { 
                        targetEl.classList.add('speaking-glow');
                    } else {
                        targetEl.classList.remove('speaking-glow');
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
                } catch (e) { console.error("Load pref error", e); }
            }
        }

        function savePreferences() {
            localStorage.setItem('rustrooms_profile', JSON.stringify({
                nickname: userNickname,
                avatar: userAvatar,
                audioOutputId: currentAudioOutputId
            }));
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

        function setupVolumeMeter(stream, barId) {
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
            if (audioContext.state === 'suspended') audioContext.resume();

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
                const img = new Image();
                img.onload = function() {
                    const canvas = document.createElement('canvas');
                    const ctx = canvas.getContext('2d');
                    
                    const MAX_SIZE = 2048;
                    let width = img.width;
                    let height = img.height;
                    
                    if (width > height) {
                        if (width > MAX_SIZE) {
                            height *= MAX_SIZE / width;
                            width = MAX_SIZE;
                        }
                    } else {
                        if (height > MAX_SIZE) {
                            width *= MAX_SIZE / height;
                            height = MAX_SIZE;
                        }
                    }
                    
                    canvas.width = width;
                    canvas.height = height;
                    ctx.drawImage(img, 0, 0, width, height);
                    
                    userAvatar = canvas.toDataURL('image/jpeg', 0.8);
                    avatarPreview.src = userAvatar;
                    avatarPreview.classList.remove('hidden');
                    avatarPlaceholder.classList.add('hidden');
                };
                img.src = e.target.result;
            };
            reader.readAsDataURL(file);
        }

        async function startPreview() {
            if (localStream) {
                localStream.getTracks().forEach(track => track.stop());
                if (localStream._originalStream) {
                     localStream._originalStream.getTracks().forEach(track => track.stop());
                }
                localStream = null;
            }

            const audioSource = audioSelect.value;
            const videoSource = videoSelect.value;
            
            const constraints = {
                audio: { 
                    deviceId: audioSource ? { exact: audioSource } : undefined,
                    echoCancellation: true,
                    noiseSuppression: false,
                    autoGainControl: true,
                    sampleRate: 48000
                },
                video: { deviceId: videoSource ? { exact: videoSource } : undefined }
            };

            try {
                let rawStream = await navigator.mediaDevices.getUserMedia(constraints);
                
                 if (rawStream.getAudioTracks().length > 0) {
                     if (!audioContext) audioContext = new (window.AudioContext || window.webkitAudioContext)();
                     
                     try {
                         await audioContext.audioWorklet.addModule('/rnnoise_processor.js');
                     } catch (err) { console.error("Failed to load rnnoise_processor in startPreview", err); }
                     
                     if (audioContext.state === 'suspended') {
                         audioContext.resume().catch(e => {});
                     }

                     const source = audioContext.createMediaStreamSource(rawStream);
                     const worklet = new AudioWorkletNode(audioContext, 'rnnoise-processor');
                     const dest = audioContext.createMediaStreamDestination();
                     
                     source.connect(worklet);
                     worklet.connect(dest);
                     
                     const processedAudio = dest.stream.getAudioTracks()[0];
                     const videoTracks = rawStream.getVideoTracks();
                     
                     localStream = new MediaStream([processedAudio, ...videoTracks]);
                     localStream._originalStream = rawStream;
                } else {
                    localStream = rawStream;
                }

                previewVideo.srcObject = localStream;
                document.getElementById('previewPlaceholder').style.display = 'none';
                updatePreviewButtons();
                setupVolumeMeter(localStream, 'setupMicBar');

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
                    if (!audioTrack || !audioTrack.enabled) {
                         if (btnMic) { btnMic.classList.add('active-red'); btnMic.innerHTML = micOffSvg; }
                    } else {
                         if (btnMic) { btnMic.classList.remove('active-red'); btnMic.innerHTML = micOnSvg; }
                    }

                    const videoTrack = localStream.getVideoTracks()[0];
                    if (!videoTrack || !videoTrack.enabled) {
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
                        ws.send(JSON.stringify({
                            type: 'cam-toggle',
                            data: { enabled: true }
                        }));
                    }
                }
            } catch (e) {
                console.error("Preview failed", e);
                document.getElementById('previewPlaceholder').style.display = 'flex';
                 try {
                    let rawStream = await navigator.mediaDevices.getUserMedia({ audio: true, video: false });
                    
                    if (rawStream.getAudioTracks().length > 0) {
                         if (!audioContext) audioContext = new (window.AudioContext || window.webkitAudioContext)();
                         
                         try {
                             await audioContext.audioWorklet.addModule('/rnnoise_processor.js');
                         } catch (err) { console.error("Failed to load rnnoise_processor in startPreview fallback", err); }
                         
                         if (audioContext.state === 'suspended') {
                             audioContext.resume().catch(e => {});
                         }
    
                         const source = audioContext.createMediaStreamSource(rawStream);
                         const worklet = new AudioWorkletNode(audioContext, 'rnnoise-processor');
                         const dest = audioContext.createMediaStreamDestination();
                         
                         source.connect(worklet);
                         worklet.connect(dest);
                         
                         const processedAudio = dest.stream.getAudioTracks()[0];
                         
                         localStream = new MediaStream([processedAudio]);
                         localStream._originalStream = rawStream;
                    } else {
                        localStream = rawStream;
                    }
                    
                    previewVideo.srcObject = null;
                    setupVolumeMeter(localStream, 'setupMicBar');
                    updatePreviewButtons();
                } catch(e2) {
                    updatePreviewButtons();
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
                 btnMic.disabled = false;
                 btnMic.classList.remove('opacity-50', 'cursor-not-allowed');
                 if (!audioTrack.enabled) {
                     btnMic.classList.add('bg-red-500', 'hover:bg-red-600');
                     btnMic.innerText = "Unmute";
                 } else {
                     btnMic.classList.remove('bg-red-500', 'hover:bg-red-600');
                     btnMic.innerText = "Mute";
                 }
             }

             if (!videoTrack) {
                 btnCam.disabled = true;
                 btnCam.classList.add('opacity-50', 'cursor-not-allowed');
                 btnCam.innerText = "No Cam";
                 document.getElementById('previewPlaceholder').style.display = 'flex';
             } else {
                 btnCam.disabled = false;
                 btnCam.classList.remove('opacity-50', 'cursor-not-allowed');
                 if (!videoTrack.enabled) {
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
            if (!localStream) return;
            const track = localStream.getAudioTracks()[0];
            if (track) {
                track.enabled = !track.enabled;
                updatePreviewButtons();
            }
        }

        function togglePreviewCam() {
             if (!localStream) return;
            const track = localStream.getVideoTracks()[0];
            if (track) {
                track.enabled = !track.enabled;
                updatePreviewButtons();
            }
        }

        async function joinRoom() {
            // Reset the left room flag when joining a new room
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
            }, 300);

            localVideo.srcObject = localStream;
            
            updateLocalLabel();
            updateLocalAvatar();
            const btnMic = document.getElementById('btnMic');
            const btnCam = document.getElementById('btnCam');

            const micOffSvg = `<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="1" y1="1" x2="23" y2="23"></line><path d="M9 9v3a3 3 0 0 0 5.12 2.12M15 9.34V4a3 3 0 0 0-5.94-.6"></path><path d="M17 16.95A7 7 0 0 1 5 12v-2m14 0v2a7 7 0 0 1-.11 1.23"></path><line x1="12" x2="12" y1="19" y2="22"></line></svg>`;
            const camOffSvg = `<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="1" y1="1" x2="23" y2="23"></line><path d="M21 21l-3.5-3.5m-2-2l-2-2m-2-2l-2-2m-2-2l-3.5-3.5"></path><path d="M15 7h5a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2h-5"></path><path d="M4 8v8a2 2 0 0 0 2 2h4.5"></path></svg>`;
            
             if (localStream) {
                const audioTrack = localStream.getAudioTracks()[0];
                const videoTrack = localStream.getVideoTracks()[0];
                
                if (!audioTrack || !audioTrack.enabled) {
                     btnMic.classList.add('active-red');
                     btnMic.innerHTML = micOffSvg;
                }
                if (!videoTrack || !videoTrack.enabled) {
                     btnCam.classList.add('active-red');
                     btnCam.innerHTML = camOffSvg;
                }
                
                setupAudioMonitor(localStream, 'local');
            } else {
                 btnMic.classList.add('active-red');
                 btnMic.innerHTML = micOffSvg;
                 btnCam.classList.add('active-red');
                 btnCam.innerHTML = camOffSvg;
            }

            connectWs();
            
            // Mark setup as done
            // Mark setup as done (Session based now)
            sessionStorage.setItem('rustrooms_setup_done', 'true');

            // Monitor network connectivity changes (airplane mode, network loss)
            window.addEventListener('offline', () => {
                console.warn('Network connection lost (offline)');
                updateStatus('disconnected', 'Network Offline');
                // Check all peer connections and update their status
                updateConnectionStatus();
            });

            window.addEventListener('online', () => {
                // Don't reconnect if user intentionally left the room
                if (hasLeftRoom) {
                    console.log('User left the room, not reconnecting on network restore');
                    return;
                }

                // Don't trigger if already reconnecting (avoid race condition)
                if (isReconnecting) {
                    console.log('Already reconnecting, skipping network restore trigger');
                    return;
                }

                console.log('Network connection restored (online)');
                updateStatus('connecting', 'Reconnecting...');
                // Reset reconnection attempts and trigger reconnection
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
            // Check if we have any active peer connections
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

            // Only show disconnected if we have peers but none are connected
            if (peerIds.length > 0 && !hasConnectedPeers && !hasConnectingPeers) {
                updateStatus('disconnected', 'Connection Lost');
            } else if (hasConnectedPeers) {
                updateStatus('connected', 'Connected');
            }
        }

        function toggleSidebar() {
            const sidebar = document.getElementById('roomSidebar');
            const overlay = document.getElementById('sidebarOverlay');
            sidebar.classList.toggle('open');
            overlay.classList.toggle('open');
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

        function showCustomConfirm(title, message, onConfirm) {
            document.getElementById('confirmTitle').innerText = title;
            document.getElementById('confirmMessage').innerText = message;
            const modal = document.getElementById('confirmModal');
            const submitBtn = document.getElementById('confirmSubmit');
            
            // Remove old onclick to prevent stacking
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
            // Cleanup existing connection
            if (ws) {
                // Prevent auto-reconnect logic from firing during intentional switch
                ws.onclose = null;
                ws.close();
            }
            stopHeartbeat();
            
            // Clear peers
            for (const userId in peers) {
                removePeer(userId);
            }
            peers = {};
            peerCamStatus = {};
            peerScreenStatus = {};
            remoteGrid.innerHTML = '';
            
            // Update state
            roomId = newRoomId;
            channelId = newChannelId;
            
            // Update URL
            const newUrl = `/${roomId}/${channelId}`;
            if (window.location.pathname !== newUrl) {
                history.pushState({ roomId, channelId }, "", newUrl);
            }
            
            // Re-connect
            wsUrl = `${wsProtocol}//${window.location.host}/ws/${roomId}/${channelId}`;
            updateStatus('connecting', 'Connecting...');
            
            // Update UI
            if (typeof updateRoomListUI === 'function') updateRoomListUI();
            
            // Re-establish connection
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
                window.location.reload(); // Fallback for root
            }
        };

        function deleteRoom(targetRoomId, event) {
            if (event) event.stopPropagation();
            
            if (targetRoomId.toLowerCase() === 'general') {
                showCustomAlert("Action Not Allowed", "Cannot delete the General room.");
                return;
            }

            // Check if room is empty
            const roomData = globalRoomList[targetRoomId];
            if (roomData && roomData.users && Object.keys(roomData.users).length > 0) {
                showCustomAlert("Room Not Empty", "You cannot delete a room that still has users in it.");
                return;
            }

            showCustomConfirm("Delete Channel", "Are you sure you want to delete this channel?", () => {
                 if (ws && ws.readyState === WebSocket.OPEN) {
                    ws.send(JSON.stringify({
                        type: 'delete-channel',
                        data: {
                            channelId: targetRoomId
                        }
                    }));
                }
            });
        }

        function renameRoom(targetRoomId, event) {
            if (event) event.stopPropagation();
            
            if (targetRoomId.toLowerCase() === 'general') {
                showCustomAlert("Action Not Allowed", "Cannot rename the General room.");
                return;
            }

            // Check if room is empty
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

        function updateRoomListUI() {
            const container = document.getElementById('roomListContainer');
            if (!container) return;
            
            container.innerHTML = '';
            
            let order = JSON.parse(localStorage.getItem('rustrooms_room_order_' + roomId) || '[]');
            const currentRids = Object.keys(globalRoomList);
            
            // Re-sync order with current rooms
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
                    usersHtml += `
                        <div class="room-user-row">
                            <div class="mini-avatar">
                                ${u.avatar ? `<img src="${u.avatar}">` : `<div class="mini-avatar-placeholder">${u.nickname.charAt(0).toUpperCase()}</div>`}
                            </div>
                            <span class="room-user-name">${u.nickname}</span>
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
                                        <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="3 6 5 6 21 6"></polyline><path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2-2v2"></path><line x1="10" y1="11" x2="10" y2="17"></line><line x1="14" y1="11" x2="14" y2="17"></line></svg>
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

        window.addEventListener('keydown', (e) => {
            if (e.key.toLowerCase() === 'r' && !['INPUT', 'TEXTAREA'].includes(document.activeElement.tagName)) {
                toggleSidebar();
            }
        });

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
                const res = await fetch('/new?password=' + encodeURIComponent(password));
                 if (res.ok) {
                     window.location.href = `/${crypto.randomUUID()}/General`;
                 } else if (res.status === 401) {
                     input.classList.add('ring-2', 'ring-red-500', 'border-red-500');
                     setTimeout(() => input.classList.remove('ring-2', 'ring-red-500', 'border-red-500'), 500);
                     input.value = '';
                     input.placeholder = "Incorrect Password";
                 } else {
                     alert("Error creating room");
                 }
            } catch (e) {
                console.error(e);
                alert("Error creating room");
            }
        }

        if (roomId) {
            loadPreferences();
            const setupDone = sessionStorage.getItem('rustrooms_setup_done') === 'true';
            if (setupDone && roomId) {
                // Auto-join if setup was already done once
                loadDevices().then(() => joinRoom());
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
                            // Clear any pending reconnect status timeout
                            if (reconnectStatusTimeout) {
                                clearTimeout(reconnectStatusTimeout);
                                reconnectStatusTimeout = null;
                            }
                            
                            playNotificationSound('join');
                            reconnectionAttempts = 0;
                            isReconnecting = false;
                            updateStatus('connected', 'Connected');
                            startHeartbeat(); // Start heartbeat ping/pong
                            const camEnabled = localStream && localStream.getVideoTracks()[0] && localStream.getVideoTracks()[0].enabled;
                            const screenEnabled = !!screenStream;
                            const screenHasAudio = screenStream && screenStream.getAudioTracks().length > 0;
                            ws.send(JSON.stringify({
                                type: "join",
                                data: {
                                    userId: persistentUserId,
                                    nickname: userNickname,
                                    avatar: userAvatar,
                                    camEnabled: camEnabled,
                                    screenEnabled: screenEnabled,
                                    screenAudio: screenHasAudio
                                }
                            }));
                            checkEmpty();
                        };
            
                        ws.onmessage = async (event) => {
                            const msg = JSON.parse(event.data);
                            
                            switch (msg.type) {
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
                                    // Check for existing peer OR DOM element to prevent duplicates
                                    if (peers[msg.userId] || document.getElementById(`wrapper-${msg.userId}`)) {
                                        removePeer(msg.userId);
                                    }

                                    if (msg.data.camEnabled !== undefined) {
                                        peerCamStatus[msg.userId] = msg.data.camEnabled;
                                    }
                                    if (msg.data.screenEnabled !== undefined) {
                                        peerScreenStatus[msg.userId] = msg.data.screenEnabled;
                                    }
                                    initPeer(msg.userId, true, msg.data?.nickname, msg.data?.avatar);
                                    
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
                                            screenAudio: myScreenHasAudio
                                        }
                                    }));
                                    break;
                                case 'user-left':
                                    // Don't remove peer if it's our own user ID (reconnection scenario)
                                    if (msg.userId !== persistentUserId) {
                                        playNotificationSound('leave');
                                        removePeer(msg.userId);
                                        delete peerCamStatus[msg.userId];
                                        delete peerScreenStatus[msg.userId];
                                    }
                                    break;
                                case 'user-update':
                                     updatePeerInfo(msg.userId, msg.data.nickname, msg.data.avatar);
                                    break;
                                case 'cam-toggle':
                                    if (msg.data && msg.data.enabled !== undefined) {
                                        peerCamStatus[msg.userId] = msg.data.enabled;
                                    }
                                    break;
                                case 'screen-toggle':
                                    if (msg.data && msg.data.enabled !== undefined) {
                                        peerScreenStatus[msg.userId] = msg.data.enabled;
                                        const v = document.getElementById(`vid-${msg.userId}`);
                                        if (v) v.style.objectFit = msg.data.enabled ? 'contain' : 'contain';
            
                                        if (!msg.data.enabled || !msg.data.hasAudio) {
                                            const row = document.getElementById(`vol-row-screen-${msg.userId}`);
                                            if (row) row.remove();
                                            const aud = document.getElementById(`aud-screen-${msg.userId}`);
                                            if (aud) aud.remove();
                                        }
                                    }
                                    break;
                                case 'identify':
                                    if (msg.data.camEnabled !== undefined) {
                                        peerCamStatus[msg.userId] = msg.data.camEnabled;
                                    }
                                    if (msg.data.screenEnabled !== undefined) {
                                        peerScreenStatus[msg.userId] = msg.data.screenEnabled;
                                    }
                                    if (peers[msg.userId]) {
                                        updatePeerInfo(msg.userId, msg.data.nickname, msg.data.avatar);
                                    } else {
                                        initPeer(msg.userId, false, msg.data.nickname, msg.data.avatar);
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
                            // Stop heartbeat on close
                            stopHeartbeat();

                            // Clear any existing reconnect status timeout
                            if (reconnectStatusTimeout) {
                                clearTimeout(reconnectStatusTimeout);
                                reconnectStatusTimeout = null;
                            }

                            // Don't reconnect if user intentionally left the room
                            if (hasLeftRoom) {
                                console.log('User left the room, not reconnecting');
                                isReconnecting = false;
                                return;
                            }

                            // Prevent race condition: skip if already reconnecting
                            if (isReconnecting) {
                                console.log('Reconnection already in progress, skipping duplicate onclose');
                                return;
                            }

                            isReconnecting = true;
                            reconnectionAttempts++;
                            if (reconnectionAttempts >= maxReconnectionAttempts) {
                                updateStatus('disconnected', 'Disconnected');
                                const btn = document.getElementById('btnReconnect');
                                if (btn) btn.classList.remove('hidden');
                                isReconnecting = false;
                                console.error('WebSocket disconnected after multiple retries. No further attempts will be made.');
                            } else {
                                const delay = getReconnectDelay(reconnectionAttempts);
                                
                                // Set a timeout to show "Reconnecting..." only after 5 seconds
                                // Keep status as "Connected" during brief disconnections
                                reconnectStatusTimeout = setTimeout(() => {
                                    // Only update if we haven't reconnected yet
                                    if (isReconnecting && (!ws || ws.readyState !== WebSocket.OPEN)) {
                                        updateStatus('connecting', `Reconnecting... (Attempt ${reconnectionAttempts}/${maxReconnectionAttempts})`);
                                    }
                                }, reconnectDelayMs);
                                
                                console.log(`Reconnecting in ${Math.round(delay)}ms...`);
                                setTimeout(() => {
                                    // Clear reconnect status timeout since we're reconnecting
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

                    // Reset flags and reconnection attempts when manually reconnecting
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

        function updatePeerInfo(userId, nickname, avatar) {
            const wrapper = document.getElementById(`wrapper-${userId}`);
            if (wrapper) {
                const label = wrapper.querySelector('.absolute.bottom-3.left-3');
                if (label) label.innerText = nickname || "Unknown";
                
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

        function initPeer(userId, initiator, nickname, avatarUrl) {
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
            if (!localStream || localStream.getAudioTracks().length === 0) {
                 pc.addTransceiver('audio', { direction: 'recvonly' });
            }

            pc.ontrack = (event) => {
                // Check if this peer is still valid (hasn't been removed and replaced)
                if (peers[userId] !== pc) {
                    return; // This peer was removed, ignore track event
                }

                let container = document.getElementById(`wrapper-${userId}`);
                if (!container) {
                    container = document.createElement('div');
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
                    
                    // Load persisted volume
                    const savedVol = getVolumeSettings(userId, 'main');
                    vid.volume = savedVol;
                    
                    vid.srcObject = new MediaStream();
                    
                    const avatarLayer = document.createElement('div');
                    avatarLayer.className = 'avatar-layer';
                    
                    setAvatar(avatarLayer, avatarUrl);

                    const label = document.createElement('div');
                    label.className = 'absolute bottom-3 left-3 bg-black/50 px-3 py-1 rounded-full text-sm text-white backdrop-blur-md z-30';
                    label.innerText = displayName;

                    const volControls = document.createElement('div');
                    volControls.id = `vol-controls-${userId}`;
                    volControls.className = 'volume-controls z-30';
                    
                    const fsBtn = document.createElement('button');
                    fsBtn.className = 'absolute top-3 right-3 p-2 rounded-xl bg-black/40 hover:bg-blue-600 text-white backdrop-blur-md transition-all opacity-0 group-hover:opacity-100 scale-90 hover:scale-100 z-30';
                    fsBtn.innerHTML = '<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M8 3H5a2 2 0 0 0-2 2v3m18 0V5a2 2 0 0 0-2-2h-3m0 18h3a2 2 0 0 0 2-2v-3M3 16v3a2 2 0 0 0 2-2h3"/></svg>';
                    fsBtn.onclick = () => toggleFullscreen(userId);
                    fsBtn.title = "Toggle Fullscreen";
                    
                    container.addEventListener('fullscreenchange', () => {
                        if (document.fullscreenElement === container) {
                            fsBtn.innerHTML = '<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M8 3v3a2 2 0 0 1-2 2H3m18 0h-3a2 2 0 0 1-2-2V3m0 18v-3a2 2 0 0 1 2-2h3"/></svg>';
                            fsBtn.classList.add('bg-blue-600');
                        } else {
                            fsBtn.innerHTML = '<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M8 3H5a2 2 0 0 0-2 2v3m18 0V5a2 2 0 0 0-2-2h-3m0 18h3a2 2 0 0 0 2-2v-3M3 16v3a2 2 0 0 0 2-2h3"/></svg>';
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
                    remoteGrid.appendChild(container);
                    checkEmpty();
                }

                const vid = document.getElementById(`vid-${userId}`);
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
                    if (mainStream.getAudioTracks().length === 0) {
                        mainStream.addTrack(event.track);
                        setupAudioMonitor(mainStream, `wrapper-${userId}`);
                        
                        const row = document.createElement('div');
                        row.className = 'vol-row';
                        row.id = `vol-row-main-${userId}`;
                        row.innerHTML = `
                            <button class="text-white hover:text-blue-400" onclick="toggleMute('${userId}', 'main')" id="mute-main-${userId}">
                                <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5"></polygon><path d="M19.07 4.93a10 10 0 0 1 0 14.14M15.54 8.46a5 5 0 0 1 0 7.07"></path></svg>
                            </button>
                            <input type="range" min="0" max="1" step="0.05" value="${getVolumeSettings(userId, 'main')}" oninput="setVolume('${userId}', 'main', this.value)">
                        `;
                        volControls.insertBefore(row, volControls.firstChild);
                        
                        event.track.onended = () => {
                            row.remove();
                        };
                    } else {
                        const savedScreenVol = getVolumeSettings(userId, 'screen');

                        const screenAud = document.createElement('audio');
                        screenAud.id = `aud-screen-${userId}`;
                        screenAud.autoplay = true;
                        attachSinkId(screenAud, currentAudioOutputId);
                        screenAud.volume = savedScreenVol; // Set volume from persisted settings
                        
                        const screenStream = new MediaStream([event.track]);
                        screenAud.srcObject = screenStream;
                        container.appendChild(screenAud);
                        
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
                        
                        setupAudioMonitor(screenStream, `wrapper-${userId}`); // Use setupAudioMonitor for screen audio
                        
                        event.track.onended = () => {
                            screenAud.remove(); // Remove the screen audio element
                            row.remove(); // Remove its volume control row
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
                    // Update status to disconnected if all peers are down
                    updateConnectionStatus();
                } else if (state === 'connected') {
                    updateConnectionStatus();
                }
            };

            pc.onconnectionstatechange = () => {
                const state = pc.connectionState;
                console.log(`Connection state for ${userId.substr(0,4)}: ${state}`);

                if (state === 'disconnected') {
                    // Disconnected state is often temporary - wait before removing
                    console.warn(`Peer ${userId.substr(0,4)} temporarily disconnected, waiting for recovery...`);
                    updateConnectionStatus();
                    // Set a timeout to remove peer if it doesn't recover within 15 seconds
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
                    // Clear any pending timeout and remove peer immediately
                    if (pc._disconnectTimeout) {
                        clearTimeout(pc._disconnectTimeout);
                        pc._disconnectTimeout = null;
                    }
                    console.warn(`Peer ${userId.substr(0,4)} connection ${state}, removing...`);
                    removePeer(userId);
                } else if (state === 'connected') {
                    // Clear any pending timeout on successful reconnection
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
            if (!peers[userId]) initPeer(userId, false, "Unknown", null); 
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
                // Clear any pending disconnect timeout
                if (peers[userId]._disconnectTimeout) {
                    clearTimeout(peers[userId]._disconnectTimeout);
                    peers[userId]._disconnectTimeout = null;
                }
                peers[userId].close();
                delete peers[userId];
            }
            const el = document.getElementById(`wrapper-${userId}`);
            if (el) el.remove();
            // Also remove screen audio element if it exists
            const screenAud = document.getElementById(`aud-screen-${userId}`);
            if (screenAud) screenAud.remove();
            const volRow = document.getElementById(`vol-row-screen-${userId}`);
            if (volRow) volRow.remove();
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
            // Set flag to prevent auto-reconnection
            hasLeftRoom = true;

            playNotificationSound('disconnect');

            // Stop all local media tracks
            if (localStream) {
                localStream.getTracks().forEach(track => track.stop());
                localStream = null;
            }

            // Stop screen sharing
            if (screenStream) {
                screenStream.getTracks().forEach(track => track.stop());
                screenStream = null;
            }

            // Close all peer connections and clean up remote audio/video
            Object.keys(peers).forEach(userId => {
                if (peers[userId]) {
                    peers[userId].close();
                    delete peers[userId];
                }

                // Stop and clear remote video elements (which also play audio)
                const vid = document.getElementById(`vid-${userId}`);
                if (vid) {
                    vid.pause();
                    vid.srcObject = null;
                }

                // Remove screen audio elements
                const screenAud = document.getElementById(`aud-screen-${userId}`);
                if (screenAud) {
                    screenAud.pause();
                    screenAud.srcObject = null;
                    screenAud.remove();
                }

                // Remove volume control rows
                const volRowScreen = document.getElementById(`vol-row-screen-${userId}`);
                if (volRowScreen) volRowScreen.remove();

                // Remove DOM elements
                const el = document.getElementById(`wrapper-${userId}`);
                if (el) el.remove();
            });

            // Close WebSocket connection
            if (ws) {
                ws.close();
                ws = null;
            }

            // Close audio context to stop all audio processing
            if (audioContext && audioContext.state !== 'closed') {
                audioContext.close().catch(e => console.error('Error closing audio context:', e));
                audioContext = null;
            }

            // Reset UI to welcome screen
            const welcomeOverlay = document.getElementById('welcomeOverlay');
            const mainApp = document.querySelector('main');
            const taskbar = document.querySelector('.taskbar');

            if (welcomeOverlay) welcomeOverlay.style.display = 'flex';
            if (mainApp) mainApp.style.display = 'none';
            if (taskbar) taskbar.style.display = 'none';

            checkEmpty();

            // Redirect to root
            sessionStorage.removeItem('rustrooms_setup_done');
            window.location.href = '/';
        }

        function toggleMic() {
            if (!localStream) return;
            const tracks = localStream.getAudioTracks();
            if (tracks.length > 0) {
                const track = tracks[0];
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
            }
        }

        async function toggleCam() {
            const btn = document.getElementById('btnCam');
            if (!localStream) return;
            
            let tracks = localStream.getVideoTracks();
            let justAdded = false;
            
            if (tracks.length === 0) {
                try {
                    const newStream = await navigator.mediaDevices.getUserMedia({ video: true });
                    const newTrack = newStream.getVideoTracks()[0];
                    localStream.addTrack(newTrack);
                    tracks = localStream.getVideoTracks();
                    justAdded = true;

                    if (!screenStream) {
                        for (const userId in peers) {
                            const pc = peers[userId];
                            pc.addTrack(newTrack, localStream);
                            negotiate(userId, pc);
                        }
                    }
                } catch (e) {
                    console.error("Could not add camera", e);
                    alert("Could not access camera. Please check permissions.");
                    return;
                }
            }

            if (tracks.length > 0) {
                const track = tracks[0];
                if (!justAdded) {
                    track.enabled = !track.enabled;
                }
                
                if (!track.enabled) {
                    btn.classList.add('active-red');
                    btn.innerHTML = `<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="1" y1="1" x2="23" y2="23"></line><path d="M21 21l-3.5-3.5m-2-2l-2-2m-2-2l-2-2m-2-2l-3.5-3.5"></path><path d="M15 7h5a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2h-5"></path><path d="M4 8v8a2 2 0 0 0 2 2h4.5"></path></svg>`;
                } else {
                    btn.classList.remove('active-red');
                    btn.innerHTML = `<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M14.5 4h-5L7 7H4a2 2 0 0 0-2 2v9a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2V9a2 2 0 0 0-2-2h-3l-2.5-3z"/><circle cx="12" cy="13" r="3"/></svg>`;
                }
                updateLocalAvatar();
                
                if (ws && ws.readyState === WebSocket.OPEN) {
                    ws.send(JSON.stringify({
                        type: 'cam-toggle',
                        data: { enabled: track.enabled }
                    }));
                }
            }
        }

        async function toggleScreen() {
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
                        data: { enabled: false, hasAudio: false }
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
                            data: { enabled: true, hasAudio: !!screenAudioTrack }
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
            if (!localStream) {
                label.innerText = "You (Offline)";
                return;
            }
            const audioTrack = localStream.getAudioTracks()[0];
            if (audioTrack && audioTrack.enabled) {
                label.innerText = `You (${userNickname})`;
            } else {
                label.innerText = `You (Muted)`;
            }
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

        function openSettings() {
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
            
            populateSettingsDeviceList();
            settingsOverlay.classList.remove('hidden');
            if (localStream) {
                setupVolumeMeter(localStream, 'settingsMicBar');
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
                const img = new Image();
                img.onload = function() {
                    const canvas = document.createElement('canvas');
                    const ctx = canvas.getContext('2d');
                    const MAX_SIZE = 2048;
                    let width = img.width;
                    let height = img.height;
                    
                    if (width > height) {
                        if (width > MAX_SIZE) {
                            height *= MAX_SIZE / width;
                            width = MAX_SIZE;
                        }
                    } else {
                        if (height > MAX_SIZE) {
                            width *= MAX_SIZE / height;
                            height = MAX_SIZE;
                        }
                    }
                    
                    canvas.width = width;
                    canvas.height = height;
                    ctx.drawImage(img, 0, 0, width, height);
                    
                    newAvatarCandidate = canvas.toDataURL('image/jpeg', 0.8);
                    settingsAvatarPreview.src = newAvatarCandidate;
                    settingsAvatarPreview.classList.remove('hidden');
                    settingsAvatarPlaceholder.classList.add('hidden');
                };
                img.src = e.target.result;
            };
            reader.readAsDataURL(file);
        }

        async function saveSettings() {
            const newAudio = document.getElementById('settingsAudioSource').value;
            const newAudioOutput = document.getElementById('settingsAudioOutputSource').value;
            const newVideo = document.getElementById('settingsVideoSource').value;
            
            const currentAudioTrack = localStream ? localStream.getAudioTracks()[0] : null;
            const currentVideoTrack = localStream ? localStream.getVideoTracks()[0] : null;
            
            const currentAudioId = currentAudioTrack ? currentAudioTrack.getSettings().deviceId : "";
            const currentVideoId = currentVideoTrack ? currentVideoTrack.getSettings().deviceId : "";

            if (newAudio !== currentAudioId || newVideo !== currentVideoId) {
                await switchMediaStream(newAudio, newVideo);
            }

            if (newAudioOutput !== currentAudioOutputId) {
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

            let isDragging = false;
            let dragOffset = { x: 0, y: 0 };
            let dragBounds = null;
            let pendingFrame = false;
            let collisionRects = null; // Cached collision rectangles

            function startDrag(clientX, clientY) {
                isDragging = true;
                pip.style.cursor = 'grabbing';
                pip.style.transition = 'none';

                const rect = pip.getBoundingClientRect();
                const taskbarRect = taskbar.getBoundingClientRect();

                pip.style.bottom = 'auto';
                pip.style.right = 'auto';
                pip.style.left = rect.left + 'px';
                pip.style.top = rect.top + 'px';

                dragOffset.x = clientX - rect.left;
                dragOffset.y = clientY - rect.top;

                // Cache static boundaries that don't change during drag
                dragBounds = {
                    minX: 16,
                    maxX: window.innerWidth - rect.width - 16,
                    minY: 16,
                    maxY: window.innerHeight - taskbarRect.height - rect.height - 16
                };

                // Cache collision rectangles once at drag start
                const margin = 16;
                collisionRects = {
                    statusRect: connectionDot && connectionDot.parentElement ? connectionDot.parentElement.getBoundingClientRect() : null,
                    copyRect: btnCopy ? btnCopy.getBoundingClientRect() : null,
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
                if (!isDragging || pendingFrame) return;

                pendingFrame = true;

                requestAnimationFrame(() => {
                    let newX = clientX - dragOffset.x;
                    let newY = clientY - dragOffset.y;

                    if (dragBounds) {
                        newX = Math.max(dragBounds.minX, Math.min(newX, dragBounds.maxX));
                        newY = Math.max(dragBounds.minY, Math.min(newY, dragBounds.maxY));
                    }

                    // Use cached collision rectangles instead of recalculating
                    if (collisionRects) {
                        const { statusRect, copyRect, margin, pipWidth } = collisionRects;

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
                dragBounds = null;
                collisionRects = null;
                pendingFrame = false;
                pip.style.cursor = 'grab';
                pip.style.transition = '';
                document.removeEventListener('mousemove', onMouseMove);
                document.removeEventListener('mouseup', onMouseUp);
            }

            function onTouchEnd() {
                isDragging = false;
                dragBounds = null;
                collisionRects = null;
                pendingFrame = false;
                pip.style.cursor = 'grab';
                pip.style.transition = '';
                document.removeEventListener('touchmove', onTouchMove);
                document.removeEventListener('touchend', onTouchEnd);
                document.removeEventListener('touchcancel', onTouchEnd);
            }
            
            pip.addEventListener('mousedown', onMouseDown);
            pip.addEventListener('touchstart', onTouchStart, { passive: false });

            window.addEventListener('resize', () => {
                if (!pip.style.left) return;

                const pipRect = pip.getBoundingClientRect();
                const taskbarRect = taskbar.getBoundingClientRect();
                const margin = 16;

                const minX = margin;
                const maxX = window.innerWidth - pipRect.width - margin;
                const minY = margin;
                const maxY = window.innerHeight - taskbarRect.height - pipRect.height - margin;
                
                let currentLeft = parseFloat(pip.style.left);
                let currentTop = parseFloat(pip.style.top);
                
                if (isNaN(currentLeft) || isNaN(currentTop)) return;

                let newX = Math.max(minX, Math.min(currentLeft, maxX));
                let newY = Math.max(minY, Math.min(currentTop, maxY));
                
                pip.style.left = newX + 'px';
                pip.style.top = newY + 'px';
            });
        })();
    </script>
</body>
</html>
"###;
    html.replace("{{TURN_URL}}", turn_url)
        .replace("{{TURN_USERNAME}}", turn_username)
        .replace("{{TURN_CREDENTIAL}}", turn_credential)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct UserStatus {
    pub nickname: String,
    pub avatar: Option<String>,
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

#[derive(Clone)]
struct AppState {
    rooms: RoomMap,
    cluster_handle: Option<cluster::ClusterHandle>,
}

#[tokio::main]
async fn main() {
    let rooms: RoomMap = Arc::new(Mutex::new(HashMap::new()));
    
    // Initialize Cluster if IROH_KEY is present
    let cluster_handle = if let Ok(key) = std::env::var("IROH_KEY") {
        match cluster::start_cluster(key, rooms.clone()).await {
            Ok(h) => {
                println!("CLUSTER: Enabled");
                Some(h)
            },
            Err(e) => {
                eprintln!("CLUSTER: Failed to start: {}", e);
                None
            }
        }
    } else {
        None
    };

    let state = AppState { rooms, cluster_handle };

    let app = Router::new()
        .route("/", get(index))
        .route("/new", get(new_room))
        .route("/{room_id}", get(index))
        .route("/{room_id}/{channel_id}", get(index))
        .route("/rnnoise.js", get(rnnoise_js))
        .route("/rnnoise_processor.js", get(rnnoise_processor_js))
        .route("/manifest.json", get(manifest_json))
        .route("/service-worker.js", get(service_worker_js))
        .route("/icon.svg", get(icon_svg))
        .route("/ws/{room_id}/{channel_id}", get(ws_handler))
        .with_state(state);

    let port = 3000;
    let listener = match tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port)).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("ERROR: Failed to bind to port {}: {}", port, e);
            eprintln!("Is the server already running? Try killing the process using this port.");
            std::process::exit(1);
        }
    };
    println!("SERVER RUNNING ON PORT {}", port);
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

async fn index(State(_state): State<AppState>) -> impl IntoResponse {
    let turn_url = std::env::var("TURN_URL").unwrap_or_default();
    let turn_username = std::env::var("TURN_USERNAME").unwrap_or_default();
    let turn_credential = std::env::var("TURN_CREDENTIAL").unwrap_or_default();

    let html = get_html_page(&turn_url, &turn_username, &turn_credential);
    (
        [(
            header::CONTENT_SECURITY_POLICY, 
            "default-src 'self'; script-src 'self' 'unsafe-inline' 'wasm-unsafe-eval' https://cdn.tailwindcss.com; style-src 'self' 'unsafe-inline' https://fonts.googleapis.com; font-src 'self' https://fonts.gstatic.com; img-src 'self' data: https: blob:; connect-src 'self' wss: ws:; media-src 'self' blob:; object-src 'none'; frame-ancestors 'none';"
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

async fn broadcast_channel_list(rooms: &RoomMap, room_id: &str) {
    let rooms_lock = rooms.lock().await;
    let room = match rooms_lock.get(room_id) {
        Some(r) => r,
        None => return,
    };

    let mut channel_list = HashMap::new();

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

    let msg = serde_json::to_string(&SignalMessage {
        msg_type: "room-list".into(), // Keep same type for frontend compatibility
        user_id: None,
        target: None,
        data: Some(serde_json::to_value(channel_list).unwrap()),
    }).unwrap();

    for users in room.values() {
        for (tx, _) in users.values() {
            let _ = tx.try_send(Ok(Message::Text(msg.clone().into())));
        }
    }
}

async fn handle_socket(socket: WebSocket, room_id: String, channel_id: String, state: AppState) {
    let rooms = state.rooms.clone();
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

                            if let Some(ref a) = avatar {
                                if a.len() > 7_000_000 {
                                    avatar = None;
                                }
                            }
                             
                             {
                                let mut rooms_lock = rooms.lock().await;
                                let room = rooms_lock.entry(room_id.clone()).or_insert_with(HashMap::new);
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
                                
                                channel.insert(user_id.clone(), (tx.clone(), UserStatus { nickname: nickname.clone(), avatar: avatar.clone() }));
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

                            
                             // Broadcast to Cluster
                             if let Some(ch) = &state.cluster_handle {
                                 ch.broadcast(cluster::ClusterMessage::Join {
                                     room_id: room_id.clone(),
                                     channel_id: channel_id.clone(),
                                     user_id: user_id.clone(),
                                     nickname: nickname.clone(),
                                     avatar: avatar.clone(),
                                 });
                             }
                             
                             let notify_msg = serde_json::to_string(&SignalMessage {
                                msg_type: "user-joined".into(),
                                user_id: Some(user_id.clone()),
                                target: None,
                                data: notify_data,
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
                            broadcast_channel_list(&rooms, &room_id).await;
                        }
                    } else {
                        if parsed.msg_type == "update-user" {
                            let mut notify_data = parsed.data.clone();
                            let mut nickname = "Guest".to_string();
                            let mut avatar = None;

                            if let Some(serde_json::Value::Object(ref mut map)) = notify_data {
                                if let Some(serde_json::Value::String(n)) = map.get("nickname") {
                                    nickname = n.clone();
                                }
                                if let Some(serde_json::Value::String(a)) = map.get("avatar") {
                                    if a.len() > 7_000_000 {
                                        map.remove("avatar");
                                    } else {
                                        avatar = Some(a.clone());
                                    }
                                }
                            }

                            {
                                let mut rooms_lock = rooms.lock().await;
                                if let Some(room) = rooms_lock.get_mut(&room_id) {
                                    if let Some(channel) = room.get_mut(&channel_id) {
                                        if let Some((_, status)) = channel.get_mut(&user_id) {
                                            status.nickname = nickname;
                                            status.avatar = avatar;
                                        }

                                        if let Some(ch) = &state.cluster_handle {
                                            ch.broadcast(cluster::ClusterMessage::Update {
                                                room_id: room_id.clone(),
                                                channel_id: channel_id.clone(),
                                                user_id: user_id.clone(),
                                                data: notify_data.clone().unwrap_or(serde_json::to_value(HashMap::<String,String>::new()).unwrap()), 
                                            });
                                        }

                                        let notify_msg = serde_json::to_string(&SignalMessage {
                                            msg_type: "user-update".into(),
                                            user_id: Some(user_id.clone()),
                                            target: None,
                                            data: notify_data,
                                        }).unwrap();

                                        for (uid, (tx, _)) in channel.iter() {
                                            if *uid != user_id {
                                                let _ = tx.try_send(Ok(Message::Text(notify_msg.clone().into())));
                                            }
                                        }
                                    }
                                }
                            }
                            broadcast_channel_list(&rooms, &room_id).await;
                        } else if parsed.msg_type == "cam-toggle" {
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
                        } else if parsed.msg_type == "screen-toggle" {
                            let rooms_lock = rooms.lock().await;
                            if let Some(room) = rooms_lock.get(&room_id) {
                                if let Some(channel) = room.get(&channel_id) {
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
                        } else if parsed.msg_type == "delete-channel" {
                            let target_channel_id = parsed.data.as_ref()
                                .and_then(|d| d.get("channelId"))
                                .and_then(|v| v.as_str())
                                .unwrap_or(&channel_id)
                                .to_string();

                            let mut rooms_lock = rooms.lock().await;
                            
                            // Check if channel is empty
                            let can_delete = if let Some(room) = rooms_lock.get(&room_id) {
                                if let Some(current_channel) = room.get(&target_channel_id) {
                                    current_channel.is_empty()
                                } else {
                                    // Channel doesn't exist?
                                    false
                                }
                            } else {
                                false
                            };

                            if can_delete {
                                if let Some(room) = rooms_lock.get_mut(&room_id) {
                                    if let Some(channel) = room.remove(&target_channel_id) {
                                        let close_msg = serde_json::to_string(&SignalMessage {
                                            msg_type: "room-deleted".into(),
                                            user_id: None,
                                            target: None,
                                            data: None,
                                        }).unwrap();
                                        for (tx, _) in channel.values() {
                                            let _ = tx.try_send(Ok(Message::Text(close_msg.clone().into())));
                                        }
                                    }
                                }
                                drop(rooms_lock);
                                broadcast_channel_list(&rooms, &room_id).await;
                            } else {
                                // Optionally notify user they can't delete (handled purely by UI for now)
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
                                
                                // Check if channel is empty
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
                                     broadcast_channel_list(&rooms, &room_id).await;
                                }
                            }
                        } else if let Some(ref target_id) = parsed.target {
                            // Forward generic signal (Offer/Answer/Candidate)
                            // First check if target is valid local?
                            // Actually, if we forward to a Proxy TX, the proxy takes care of it.
                            // If target is NOT found locally, we might need to broadcast?
                            // No, relying on RoomMap logic: If target is in RoomMap (either local or proxy), send to it.
                            // BUT, we need to populate RoomMap with Proxies.
                            // So this code block remains largely unchanged, assuming channel.get() returns the Proxy TX.
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
                                // If not found in local map, maybe it's on another instance but we haven't synced?
                                // Or maybe the user just left.
                                // We can optionally broadcast Signal here if we wanted "stateless" signaling, 
                                // but we are using "stateful" proxies. So if not in map, we assume unreachable.
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

    {
        let mut rooms_lock = rooms.lock().await;

        if is_joined {
            if let Some(room) = rooms_lock.get_mut(&room_id) {
                let mut removed = false;

                // First try to find in the expected channel
                if let Some(channel) = room.get_mut(&channel_id) {
                    if let Some((stored_tx, _)) = channel.get(&user_id) {
                        if stored_tx.same_channel(&tx) {
                            channel.remove(&user_id);
                            removed = true;
                            

                            
                                     if let Some(ch) = &state.cluster_handle {
                                         ch.broadcast(cluster::ClusterMessage::Leave {
                                             room_id: room_id.clone(),
                                             channel_id: channel_id.clone(),
                                             user_id: user_id.clone(),
                                         });
                             }
                            
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
                
                // If not found (e.g. race condition or rename), scan all channels
                if !removed {
                    for channel in room.values_mut() {
                        if let Some((stored_tx, _)) = channel.get(&user_id) {
                            if stored_tx.same_channel(&tx) {
                                channel.remove(&user_id);
                                if !channel.is_empty() {
                                    let notify_msg = serde_json::to_string(&SignalMessage {
                                        msg_type: "user-left".into(),
                                        user_id: Some(user_id.clone()),
                                        target: None,
                                        data: None,
                                    }).unwrap();
                                    
                                    if let Some(ch) = &state.cluster_handle {
                                        ch.broadcast(cluster::ClusterMessage::Leave {
                                            room_id: room_id.clone(),
                                            channel_id: "unknown".to_string(),
                                            user_id: user_id.clone(),
                                        }); 
                                    }

                                    for (_, (tx, _)) in channel.iter() {
                                        let _ = tx.try_send(Ok(Message::Text(notify_msg.clone().into())));
                                    }
                                }
                                break;
                            }
                        }
                    } 
                }
            }
        }
    }
    broadcast_channel_list(&rooms, &room_id).await;
}

mod cluster {
    use super::*;
    use iroh::Endpoint;
     use iroh_gossip::api::Event as GossipEvent;
     use iroh_gossip::{net::Gossip, proto::TopicId};
    use std::collections::HashMap;
    use axum::extract::ws::Message;
    use serde::{Serialize, Deserialize};

    #[derive(Clone, Debug, Serialize, Deserialize)]
    pub enum ClusterMessage {
        Join { room_id: String, channel_id: String, user_id: String, nickname: String, avatar: Option<String> },
        Leave { room_id: String, channel_id: String, user_id: String },
        Update { room_id: String, channel_id: String, user_id: String, data: serde_json::Value },
        Signal { target_user_id: String, payload: String },
    }

    #[derive(Clone)]
    pub struct ClusterHandle {
        cmd_tx: tokio::sync::mpsc::Sender<ClusterMessage>,
        _topic: TopicId,
    }

    impl ClusterHandle {
        pub fn broadcast(&self, msg: ClusterMessage) {
             let _ = self.cmd_tx.try_send(msg);
        }
    }

    pub async fn start_cluster(key: String, rooms: RoomMap) -> anyhow::Result<ClusterHandle> {
        let topic_bytes = blake3::hash(key.as_bytes());
        let topic = TopicId::from_bytes(*topic_bytes.as_bytes());

        // Create the channel immediately so we can return the handle
        let (cmd_tx, mut cmd_rx) = tokio::sync::mpsc::channel(100);
        
        let handle = ClusterHandle { cmd_tx, _topic: topic };
        let handle_clone = handle.clone();

        // Spawn the connection and processing loop in the background
        tokio::spawn(async move {
            println!("CLUSTER: Initializing node...");
            
            // Setup Iroh node
            let endpoint_res = Endpoint::builder().bind().await;
            if let Err(e) = endpoint_res {
                eprintln!("CLUSTER ERROR: Failed to bind endpoint: {}", e);
                return;
            }
            let endpoint = endpoint_res.unwrap();
            
            let gossip = Gossip::builder().spawn(endpoint.clone());

            // Join the topic (this can take time, so it's good we're effectively backgrounded)
            println!("CLUSTER: Joining topic {}...", topic);
            
            let topic_io_res = gossip.subscribe_and_join(topic, vec![]).await;
            if let Err(e) = topic_io_res {
                 eprintln!("CLUSTER ERROR: Failed to join topic: {}", e);
                 return;
            }
            let mut stream = topic_io_res.unwrap();
            
            println!("CLUSTER: Joined topic {}, Node ID: {}", topic, endpoint.secret_key().public());
        
            loop {
                tokio::select! {
                    cmd = cmd_rx.recv() => {
                        if let Some(msg) = cmd {
                            let data = serde_json::to_vec(&msg).unwrap();
                            let _ = stream.broadcast(data.into()).await;
                        } else {
                            break;
                        }
                    }
                    event = stream.next() => {
                        if let Some(res) = event {
                            if let Ok(event) = res {
                                match event {
                                    GossipEvent::Received(msg) => {
                                         if let Ok(cluster_msg) = serde_json::from_slice::<ClusterMessage>(&msg.content) {
                                            handle_cluster_message(cluster_msg, &rooms, &handle_clone).await;
                                         }
                                    }
                                    _ => {}
                                }
                            }
                        } else {
                            break;
                        }
                    }
                }
            }
        });

        Ok(handle)
    }

    async fn handle_cluster_message(msg: ClusterMessage, rooms: &RoomMap, handle: &ClusterHandle) {
         match msg {
             ClusterMessage::Join { room_id, channel_id, user_id, nickname, avatar } => {
                 let mut rooms_lock = rooms.lock().await;
                 let room = rooms_lock.entry(room_id.clone()).or_insert_with(HashMap::new);
                 let channel = room.entry(channel_id.clone()).or_insert_with(HashMap::new);

                 if channel.contains_key(&user_id) {
                     // If user exists, we overwrite/update the proxy.
                     // This handles the case where a node died and the user reconnected elsewhere.
                     println!("CLUSTER: Overwriting existing proxy (re-join) {} in {}/{}", user_id, room_id, channel_id);
                 }

                 // Create Proxy
                 let (tx, mut rx) = tokio::sync::mpsc::channel(5000);
                 let h = handle.clone();
                 let target = user_id.clone();
                 
                 tokio::spawn(async move {
                     while let Some(res) = rx.recv().await {
                         if let Ok(Message::Text(text)) = res {
                             let sig = ClusterMessage::Signal {
                                 target_user_id: target.clone(),
                                 payload: text.to_string(),
                             };
                             h.broadcast(sig);
                         }
                     }
                 });

                 channel.insert(user_id.clone(), (tx, UserStatus { nickname: nickname.clone(), avatar: avatar.clone() }));
                 println!("CLUSTER: Added proxy user {} in {}/{}", user_id, room_id, channel_id);
                 
                  let notify_msg = serde_json::to_string(&SignalMessage {
                        msg_type: "user-joined".into(),
                        user_id: Some(user_id.clone()),
                        target: None,
                        data: Some(serde_json::json!({
                            "nickname": nickname,
                            "avatar": avatar
                        })),
                    }).unwrap();
                 
                 for (uid, (tx, _)) in channel.iter() {
                     if *uid != user_id {
                         let _ = tx.try_send(Ok(Message::Text(notify_msg.clone().into())));
                     }
                 }
             }
             ClusterMessage::Leave { room_id, channel_id, user_id } => {
                  let mut rooms_lock = rooms.lock().await;
                  if let Some(room) = rooms_lock.get_mut(&room_id) {
                      if let Some(channel) = room.get_mut(&channel_id) {
                          if channel.remove(&user_id).is_some() {
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
             ClusterMessage::Update { room_id, channel_id, user_id, data } => {
                  let mut rooms_lock = rooms.lock().await;
                  if let Some(room) = rooms_lock.get_mut(&room_id) {
                      if let Some(channel) = room.get_mut(&channel_id) {
                          if let Some((_, status)) = channel.get_mut(&user_id) {
                                if let Some(map) = data.as_object() {
                                    if let Some(serde_json::Value::String(n)) = map.get("nickname") {
                                        status.nickname = n.clone();
                                    }
                                    if let Some(serde_json::Value::String(a)) = map.get("avatar") {
                                        status.avatar = Some(a.clone());
                                    }
                                }
                          }
                          
                          let notify_msg = serde_json::to_string(&SignalMessage {
                                msg_type: "user-update".into(),
                                user_id: Some(user_id.clone()),
                                target: None,
                                data: Some(data),
                            }).unwrap();
                            for (uid, (tx, _)) in channel.iter() {
                                if *uid != user_id {
                                    let _ = tx.try_send(Ok(Message::Text(notify_msg.clone().into())));
                                }
                            }
                      }
                  }
             }
             ClusterMessage::Signal { target_user_id, payload } => {
                  let rooms_lock = rooms.lock().await;
                  for room in rooms_lock.values() {
                      for channel in room.values() {
                          if let Some((tx, _)) = channel.get(&target_user_id) {
                              let _ = tx.try_send(Ok(Message::Text(payload.clone().into())));
                              return;
                          }
                      }
                  }
             }
         }
    }
}
