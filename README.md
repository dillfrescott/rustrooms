### How to use:

1. Install rust!

2.  From the repo dir just run `cargo build --release`.

3.  A standalone executable for your platform will be generated in ./target/release/

4.  Enjoy!

### Notes:

It uses port 3000 TCP (for the web interface).

### TURN/STUN Configuration:

RustRooms requires a TURN/STUN server for WebRTC connections to work properly, especially when users are behind restrictive NATs or firewalls. You can configure a third-party TURN server using the following environment variables:

*   `TURN_URL`: The TURN server URL (e.g., `turn:your-turn-server.com:3478`)
*   `TURN_USERNAME`: The TURN server username
*   `TURN_CREDENTIAL`: The TURN server password/credential

If these environment variables are not set, the application will default to using a public TURN server.

For self-hosted TURN servers, you can use [coturn](https://github.com/coturn/coturn) or use a service like [Express TURN](https://expressturn.com/).

### Security:

For a production deployment, it is **highly recommended** to set the following environment variable:

*   `ROOM_CREATION_PASSWORD`: Set this to a strong password to prevent unauthorized room creation. If this is not set, anyone can create rooms.
