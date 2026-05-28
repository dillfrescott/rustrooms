use futures::{SinkExt, StreamExt};
use serde_json::json;
use std::env;
use std::time::Duration;
use tokio::time::sleep;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::protocol::Message;
use uuid::Uuid;

const FIRST_NAMES: &[&str] = &[
    "Happy", "Sad", "Angry", "Rusty", "Axum", "Tokio", "Hyper", "Fast", "Secure",
    "Silly", "Cool", "Turbo", "Smart", "Lazy", "Crazy", "Funky", "Brave", "Silent",
    "Clumsy", "Jolly", "Mighty", "Grand", "Super", "Mega", "Giga", "Cyber", "Crypto",
    "Quick", "Sleek", "Fancy", "Shiny", "Cosmic", "Lunar", "Solar", "Quantum", "Dynamic"
];

const LAST_NAMES: &[&str] = &[
    "Crab", "Coder", "Dev", "Hacker", "User", "Runner", "Client", "Bot", "Ghost",
    "Ninja", "Wizard", "Knight", "Pirate", "Samurai", "Pilot", "Captain", "Gamer",
    "Geek", "Nerd", "Guru", "Master", "Explorer", "Hunter", "Seeker", "Walker", "Stalker",
    "Networker", "Packeteer", "Socket", "Compiler", "Builder", "Terminator", "Surfer"
];

struct Args {
    server_url: String,
    room_id: String,
    channel_id: String,
    count: usize,
    delay_ms: u64,
}

fn parse_args() -> Args {
    let args: Vec<String> = env::args().collect();
    let mut server_url = "ws://127.0.0.1:3000".to_string();
    let mut room_id = "lobby".to_string();
    let mut channel_id = "General".to_string();
    let mut count = 10;
    let mut delay_ms = 200;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--url" | "-u" => {
                if i + 1 < args.len() {
                    server_url = args[i + 1].clone();
                    i += 2;
                } else {
                    eprintln!("Error: Missing value for {}", args[i]);
                    std::process::exit(1);
                }
            }
            "--room" | "-r" => {
                if i + 1 < args.len() {
                    room_id = args[i + 1].clone();
                    i += 2;
                } else {
                    eprintln!("Error: Missing value for {}", args[i]);
                    std::process::exit(1);
                }
            }
            "--channel" | "-c" => {
                if i + 1 < args.len() {
                    channel_id = args[i + 1].clone();
                    i += 2;
                } else {
                    eprintln!("Error: Missing value for {}", args[i]);
                    std::process::exit(1);
                }
            }
            "--count" | "-n" => {
                if i + 1 < args.len() {
                    if let Ok(c) = args[i + 1].parse::<usize>() {
                        count = c;
                    } else {
                        eprintln!("Error: Invalid count {}", args[i + 1]);
                        std::process::exit(1);
                    }
                    i += 2;
                } else {
                    eprintln!("Error: Missing value for {}", args[i]);
                    std::process::exit(1);
                }
            }
            "--delay" | "-d" => {
                if i + 1 < args.len() {
                    if let Ok(d) = args[i + 1].parse::<u64>() {
                        delay_ms = d;
                    } else {
                        eprintln!("Error: Invalid delay {}", args[i + 1]);
                        std::process::exit(1);
                    }
                    i += 2;
                } else {
                    eprintln!("Error: Missing value for {}", args[i]);
                    std::process::exit(1);
                }
            }
            "--help" | "-h" => {
                print_help();
                std::process::exit(0);
            }
            _ => {
                eprintln!("Unknown argument: {}", args[i]);
                print_help();
                std::process::exit(1);
            }
        }
    }

    // Standardize URL to not have a trailing slash
    if server_url.ends_with('/') {
        server_url.pop();
    }

    Args {
        server_url,
        room_id,
        channel_id,
        count,
        delay_ms,
    }
}

fn print_help() {
    println!("RustRooms Dummy Users Generator");
    println!("Usage:");
    println!("  cargo run --bin dummy_clients [options]");
    println!("\nOptions:");
    println!("  -u, --url <url>       Server WebSocket URL (default: ws://127.0.0.1:3000)");
    println!("  -r, --room <id>       Room ID to join (default: lobby)");
    println!("  -c, --channel <id>    Channel ID to join (default: General)");
    println!("  -n, --count <num>     Number of dummy users to connect (default: 10)");
    println!("  -d, --delay <ms>      Delay in milliseconds between connections (default: 200)");
    println!("  -h, --help            Show this help message");
}

fn percent_encode(s: &str) -> String {
    let mut encoded = String::new();
    for b in s.bytes() {
        match b {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(b as char);
            }
            _ => {
                encoded.push_str(&format!("%{:02X}", b));
            }
        }
    }
    encoded
}

fn get_random_nickname() -> String {
    let u = Uuid::new_v4();
    let bytes = u.as_bytes();
    let first_idx = (bytes[0] as usize) % FIRST_NAMES.len();
    let last_idx = (bytes[1] as usize) % LAST_NAMES.len();
    let num = ((bytes[2] as u32) << 8 | (bytes[3] as u32)) % 1000;
    format!("{} {}{}", FIRST_NAMES[first_idx], LAST_NAMES[last_idx], num)
}

#[tokio::main]
async fn main() {
    let args = parse_args();

    println!("===============================================");
    println!("Starting RustRooms Dummy Client Simulation");
    println!("Server URL: {}", args.server_url);
    println!("Room ID:    {}", args.room_id);
    println!("Channel ID: {}", args.channel_id);
    println!("Count:      {}", args.count);
    println!("Delay:      {} ms", args.delay_ms);
    println!("===============================================");

    let mut handles = vec![];

    for i in 1..=args.count {
        let nickname = get_random_nickname();
        let server_url = args.server_url.clone();
        let room_id = args.room_id.clone();
        let channel_id = args.channel_id.clone();

        let handle = tokio::spawn(async move {
            start_dummy_client(i, server_url, room_id, channel_id, nickname).await;
        });

        handles.push(handle);

        if i < args.count && args.delay_ms > 0 {
            sleep(Duration::from_millis(args.delay_ms)).await;
        }
    }

    println!("\nAll {} dummy client tasks spawned. Press Ctrl+C to terminate.\n", args.count);

    // Keep the main thread alive waiting for all tasks to complete
    for handle in handles {
        let _ = handle.await;
    }
}

async fn start_dummy_client(
    client_num: usize,
    server_url: String,
    room_id: String,
    channel_id: String,
    nickname: String,
) {
    let ws_url = format!("{}/ws/{}/{}", server_url, room_id, percent_encode(&channel_id));

    println!("[Client {:2}] Connecting as '{}'...", client_num, nickname);

    let (ws_stream, _) = match connect_async(&ws_url).await {
        Ok(res) => res,
        Err(e) => {
            eprintln!("[Client {:2}] Error connecting: {}", client_num, e);
            return;
        }
    };

    println!("[Client {:2}] Connected. Sending join message...", client_num);
    let (mut write, mut read) = ws_stream.split();

    // Prepare join payload
    let join_payload = json!({
        "type": "join",
        "data": {
            "nickname": nickname,
            "isMuted": false,
            "isDeafened": false,
            "screenEnabled": false,
            "isGif": false,
            "avatar": null
        }
    });

    let join_msg = join_payload.to_string();
    if let Err(e) = write.send(Message::Text(join_msg.into())).await {
        eprintln!("[Client {:2}] Error sending join message: {}", client_num, e);
        return;
    }

    // Ping channel sender
    let (ping_tx, mut ping_rx) = tokio::sync::mpsc::channel::<()>(5);
    
    // Periodically send ping every 5 seconds (heartbeat)
    let client_num_ping = client_num;
    let ping_handle = tokio::spawn(async move {
        loop {
            sleep(Duration::from_secs(5)).await;
            if ping_tx.send(()).await.is_err() {
                break;
            }
        }
    });

    loop {
        tokio::select! {
            _ = ping_rx.recv() => {
                let ping_payload = json!({
                    "type": "ping"
                });
                if let Err(e) = write.send(Message::Text(ping_payload.to_string().into())).await {
                    eprintln!("[Client {:2}] Error sending ping: {}", client_num_ping, e);
                    break;
                }
            }
            msg_res = read.next() => {
                match msg_res {
                    Some(Ok(Message::Text(_text))) => {
                        // Successfully received text message, just ignore
                    }
                    Some(Ok(Message::Close(_))) => {
                        println!("[Client {:2}] Connection closed by server", client_num);
                        break;
                    }
                    Some(Err(e)) => {
                        eprintln!("[Client {:2}] Connection error: {}", client_num, e);
                        break;
                    }
                    None => {
                        println!("[Client {:2}] Connection terminated", client_num);
                        break;
                    }
                    _ => {}
                }
            }
        }
    }

    ping_handle.abort();
}
