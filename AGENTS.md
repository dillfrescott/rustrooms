# AGENTS.md

## Project Overview

RustRooms is a self-contained WebRTC video conferencing server. The entire application (Rust backend + HTML/JS/CSS frontend) lives in a single file: `src/main.rs` (~9000+ lines). Static assets in `src/assets/` are compiled into the binary via `include_str!`/`include_bytes!` — they are NOT served from disk at runtime.

## Build & Run

```bash
cargo build --release          # binary at ./target/release/rust_rooms
cargo build                    # debug build for faster iteration
cargo run                      # dev run (debug)
```

No test suite, no linting config, no formatter config exists. Run `cargo check` before committing to catch compile errors.

## Architecture (what you'd miss without help)

**Single binary, no modules.** `src/main.rs` contains everything:
- Server-side Axum routes, WebSocket handlers, and cluster logic
- The entire client-side SPA is embedded as raw HTML/JS/CSS strings inside Rust functions (e.g., `get_html_page()`)
- Static assets (Tailwind, Croppie, Inter fonts) are in `src/assets/` and baked in at compile time

**Routing model:** URL path = `/{room_id}/{channel_id}`. Channel names are URL-encoded. The default channel is `"General"`. Rooms are created on first join.

**Cluster mode** (env var `KEY`): Instances discover each other via BitTorrent DHT (`mainline` crate). Inter-instance traffic uses WebSocket at `/cluster-ws`. Relatively untested.

**WebRTC signaling:** The server is a pure signaling relay. Peers exchange SDP offers/answers and ICE candidates through WebSocket messages. TURN server config is injected into the HTML at render time via env vars.

## Environment Variables

| Variable | Purpose | Default |
|---|---|---|
| `PORT` | HTTP listen port | `3000` |
| `ROOM_CREATION_PASSWORD` | Password required to create rooms | none (open) |
| `URL` | Restrict to specific Host header | none |
| `TURN_URL` | TURN server URL for WebRTC | empty |
| `TURN_USERNAME` | TURN credentials | empty |
| `TURN_CREDENTIAL` | TURN credentials | empty |
| `KEY` | Enables cluster mode (shared secret) | disabled |
| `CLUSTER_SCHEME` | `ws` or `wss` for inter-instance WS | `ws` |

## Key Gotchas

- **Editing the frontend means editing Rust strings.** The HTML/JS/CSS is inside `get_html_page()` and other functions as raw string literals. Changes require recompilation.
- **`src/rnnoise.js` and `src/rnnoise_processor.js`** are also embedded via `include_str!`. They implement RNNoise audio worklet for noise suppression.
- **No hot reload.** There is no dev server or watch mode. Rebuild after every change.
- **Cargo.lock is gitignored.** This is unusual for a binary crate — lockfile is not committed.
- **Rust edition 2024** in `Cargo.toml`. Requires a recent Rust toolchain (1.85+).
- **Content Security Policy** is set in the `index` handler. If adding external resources, update the CSP header.
- **WebSocket max message size** is 8 MB (for avatar data). Don't assume default limits.
- **Room cleanup:** Empty rooms are deleted after a timeout via `room_cleanup_generations`.
- **Origin validation:** The WS handler checks Origin header against Host to prevent cross-origin hijacking.

## File Map

| Path | What it is |
|---|---|
| `src/main.rs` | Everything — server, routes, WS handlers, embedded frontend |
| `src/rnnoise.js` | RNNoise WASM loader (embedded) |
| `src/rnnoise_processor.js` | AudioWorklet processor (embedded) |
| `src/assets/` | Tailwind JS, Croppie JS/CSS, Inter font files (embedded) |
| `Dockerfile` | Multi-stage build: `rust:1-bookworm` → `debian:bookworm-slim` |
| `.github/workflows/docker-publish.yml` | Pushes to Docker Hub on main branch push |
