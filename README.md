### How to use:

1. Install rust!

2.  From the repo dir just run `cargo build --release`.

3.  A standalone executable for your platform will be generated in ./target/release/

4.  Enjoy!

### Notes:

It uses port 3000 TCP (for the web interface) but can be changed by specifying the PORT env variable to a different value.

### TURN Configuration:

RustRooms requires a TURN server for WebRTC connections to work properly, especially when users are behind restrictive NATs or firewalls. You can configure a third-party TURN server using the following environment variables:

*   `TURN_URL`: The TURN server URL (e.g., `turn:your-turn-server.com:3478`)
*   `TURN_USERNAME`: The TURN server username
*   `TURN_CREDENTIAL`: The TURN server password/credential

For self-hosted TURN servers, you can use [coturn](https://github.com/coturn/coturn) or use a hosted service like [metered.ca](https://www.metered.ca).

### Security:

For a production deployment, it is **highly recommended** to set the following environment variable:

*   `ROOM_CREATION_PASSWORD`: Set this to a strong password to prevent unauthorized room creation. If this is not set, anyone can create rooms.
*   `URL`: When set, restricts access to only requests whose `Host` header matches this value. Useful for preventing access via raw IP or alternative domain names. The value is automatically normalized (scheme and path are stripped).

### Cluster / Distributed Mode:

RustRooms supports running multiple instances in a cluster using DHT-based peer discovery. This is enabled by setting the `KEY` environment variable. **Note: distributed mode is relatively untested and may contain bugs — use with caution in production.**

*   `KEY`: A shared secret that enables cluster mode. All instances sharing the same key will discover and connect to each other automatically.
*   `CLUSTER_SCHEME`: The WebSocket scheme used for inter-instance communication. Set to `wss` for encrypted connections (recommended for production behind a TLS-terminating proxy). Defaults to `ws` (unencrypted).

### Issues & Bug Reports

If you find a bug or issue, please [open an issue on GitHub](https://github.com/nickk024/RustRooms/issues).
