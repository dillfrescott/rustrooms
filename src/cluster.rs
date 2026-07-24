use crate::state::*;
use axum::{
    extract::{
        Query, State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    response::IntoResponse,
};
use futures::{sink::SinkExt, stream::StreamExt};
use sha1::{Digest, Sha1};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};
use tokio::sync::Mutex;
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message as WsMessage};
use uuid::Uuid;

type PeerUsers = Arc<Mutex<HashSet<(String, String, String)>>>;

pub(crate) async fn cluster_ws_handler(
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
    if let Some(peer_node_id) = params.get("node_id")
        && *peer_node_id == state.node_id
    {
        return (axum::http::StatusCode::BAD_REQUEST, "Self connection").into_response();
    }
    ws.max_message_size(32 * 1024 * 1024)
        .on_upgrade(move |socket| handle_inbound_cluster(socket, state))
}

async fn handle_inbound_cluster(socket: WebSocket, state: AppState) {
    let source_id = Uuid::new_v4().to_string();
    let (mut ws_tx, mut ws_rx) = socket.split();
    let (write_tx, mut write_rx) = tokio::sync::mpsc::channel::<String>(5000);

    let writer = tokio::spawn(async move {
        while let Some(msg) = write_rx.recv().await {
            if ws_tx.send(Message::Text(msg.into())).await.is_err() {
                break;
            }
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
                            "isGif": status.is_gif,
                            "staticFrame": status.static_frame,
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
            if write_tx_fwd.send(msg).await.is_err() {
                break;
            }
        }
    });

    let rooms = state.rooms.clone();
    let remote_users = state.remote_users.clone();
    let peer_users: PeerUsers = Arc::new(Mutex::new(HashSet::new()));
    let peer_users_cleanup = peer_users.clone();

    while let Some(Ok(msg)) = ws_rx.next().await {
        if let Message::Text(text) = msg
            && let Ok(cm) = serde_json::from_str::<ClusterMessage>(&text)
        {
            if !is_valid_cluster_message(&cm) {
                continue;
            }
            track_peer_message(&cm, &peer_users, &state.remote_user_sources, &source_id).await;
            handle_cluster_message(&cm, &rooms, &remote_users, &state).await;
        }
    }

    forwarder.abort();
    writer.abort();
    let dead = peer_users_cleanup.lock().await.clone();
    let affected_rooms = cleanup_dead_remote_users(
        &dead,
        &rooms,
        &remote_users,
        &state.remote_user_sources,
        &source_id,
        &state.channel_creation_times,
        &state.cluster_tx,
    )
    .await;
    for room_id in affected_rooms {
        schedule_empty_room_cleanup(&state, &room_id).await;
    }
}

pub(crate) fn spawn_dht_discovery(state: AppState, port: u16) {
    let key = state.cluster_key.clone().unwrap_or_default();
    tokio::spawn(async move {
        let info_hash = {
            let hash = Sha1::digest(key.as_bytes());
            let mut bytes = [0u8; 20];
            bytes.copy_from_slice(&hash);
            mainline::Id::from_bytes(bytes).expect("SHA1 always produces 20 bytes")
        };
        println!("CLUSTER: DHT infohash = {:?}", info_hash);

        let dht = match tokio::task::spawn_blocking(mainline::Dht::client).await {
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
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;

        let dht_clone = dht.clone();
        let bootstrapped = tokio::task::spawn_blocking(move || dht_clone.bootstrapped())
            .await
            .unwrap_or(false);
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
            })
            .await
            .unwrap_or(Err(mainline::errors::PutQueryError::NoClosestNodes))
            {
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
            })
            .await;

            if let Ok(peers) = peers_result {
                let unique_peers: HashSet<String> = peers
                    .iter()
                    .filter(|p| !(p.ip().is_loopback() && p.port() == port))
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
                                    println!(
                                        "CLUSTER: Connection to {} failed ({}/3): {}",
                                        target_addr, failures, e
                                    );

                                    // NAT Loopback Fallback: If not already 127.0.0.1, try localhost
                                    if !target_addr.starts_with("127.0.0.1")
                                        && let Some(port_idx) = addr_str_clean.rfind(':')
                                    {
                                        let port = &addr_str_clean[port_idx..];
                                        target_addr = format!("127.0.0.1{}", port);
                                        println!(
                                            "CLUSTER: NAT Loopback? Retrying with local fallback: {}",
                                            target_addr
                                        );
                                        continue;
                                    }

                                    if failures >= 3 {
                                        println!(
                                            "CLUSTER: Giving up on {} (will retry if re-discovered)",
                                            addr_str_clean
                                        );
                                        break;
                                    }
                                }
                            }
                            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                        }

                        state_clone
                            .connected_peers
                            .lock()
                            .await
                            .remove(&addr_str_clean);
                    });
                }
            }

            tokio::time::sleep(std::time::Duration::from_secs(30)).await;
        }
    });
}

async fn connect_to_peer(
    url: &str,
    state: &AppState,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let source_id = Uuid::new_v4().to_string();
    let cluster_key = state.cluster_key.as_ref().ok_or("No cluster key")?;
    let mut full_url = url::Url::parse(url)?;
    full_url
        .query_pairs_mut()
        .append_pair("key", cluster_key)
        .append_pair("node_id", &state.node_id);

    let (ws_stream, _) = connect_async(full_url.as_str()).await?;
    println!("CLUSTER: Connected to peer {}", url);

    let (mut write, mut read) = ws_stream.split();
    let (write_tx, mut write_rx) = tokio::sync::mpsc::channel::<String>(5000);

    let writer = tokio::spawn(async move {
        while let Some(msg) = write_rx.recv().await {
            if write.send(WsMessage::Text(msg.into())).await.is_err() {
                break;
            }
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
                            "isGif": status.is_gif,
                            "staticFrame": status.static_frame,
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
            if write_tx_fwd.send(msg).await.is_err() {
                break;
            }
        }
    });

    let rooms = state.rooms.clone();
    let remote_users = state.remote_users.clone();
    let peer_users: PeerUsers = Arc::new(Mutex::new(HashSet::new()));
    let peer_users_cleanup = peer_users.clone();

    while let Some(Ok(msg)) = read.next().await {
        if let WsMessage::Text(text) = msg {
            let text_str: String = text.to_string();
            if let Ok(cm) = serde_json::from_str::<ClusterMessage>(&text_str) {
                if !is_valid_cluster_message(&cm) {
                    continue;
                }
                track_peer_message(&cm, &peer_users, &state.remote_user_sources, &source_id).await;
                handle_cluster_message(&cm, &rooms, &remote_users, state).await;
            }
        }
    }

    forwarder.abort();
    writer.abort();
    let dead = peer_users_cleanup.lock().await.clone();
    let affected_rooms = cleanup_dead_remote_users(
        &dead,
        &rooms,
        &remote_users,
        &state.remote_user_sources,
        &source_id,
        &state.channel_creation_times,
        &state.cluster_tx,
    )
    .await;
    for room_id in affected_rooms {
        schedule_empty_room_cleanup(state, &room_id).await;
    }
    Ok(())
}

fn is_valid_cluster_message(msg: &ClusterMessage) -> bool {
    is_valid_room_id(&msg.room_id)
        && normalize_channel_id(&msg.channel_id).as_deref() == Some(msg.channel_id.as_str())
        && Uuid::parse_str(&msg.user_id).is_ok()
        && Uuid::parse_str(&msg.msg_id).is_ok()
}

async fn track_peer_message(
    msg: &ClusterMessage,
    peer_users: &PeerUsers,
    sources: &RemoteUserSourcesMap,
    source_id: &str,
) {
    let key = (
        msg.room_id.clone(),
        msg.channel_id.clone(),
        msg.user_id.clone(),
    );

    match msg.msg_type.as_str() {
        "user-joined" => {
            peer_users.lock().await.insert(key.clone());
            sources
                .lock()
                .await
                .entry(key)
                .or_default()
                .insert(source_id.to_string());
        }
        "user-left" | "user-kicked" => {
            peer_users.lock().await.remove(&key);
            let mut sources_lock = sources.lock().await;
            if let Some(source_ids) = sources_lock.get_mut(&key) {
                source_ids.remove(source_id);
                if source_ids.is_empty() {
                    sources_lock.remove(&key);
                }
            }
        }
        "rename-channel" => {
            let Some(new_name) = msg
                .data
                .as_ref()
                .and_then(|data| data.get("newName"))
                .and_then(|value| value.as_str())
                .and_then(normalize_channel_id)
            else {
                return;
            };

            let moved_users: Vec<_> = peer_users
                .lock()
                .await
                .iter()
                .filter(|(room_id, channel_id, _)| {
                    room_id == &msg.room_id && channel_id == &msg.channel_id
                })
                .cloned()
                .collect();
            let mut peer_users_lock = peer_users.lock().await;
            let mut sources_lock = sources.lock().await;
            for old_key in moved_users {
                peer_users_lock.remove(&old_key);
                let new_key = (old_key.0.clone(), new_name.clone(), old_key.2.clone());
                peer_users_lock.insert(new_key.clone());
                if let Some(source_ids) = sources_lock.remove(&old_key) {
                    sources_lock.entry(new_key).or_default().extend(source_ids);
                }
            }
        }
        "delete-channel" => {
            let removed_users: Vec<_> = peer_users
                .lock()
                .await
                .iter()
                .filter(|(room_id, channel_id, _)| {
                    room_id == &msg.room_id && channel_id == &msg.channel_id
                })
                .cloned()
                .collect();
            let mut peer_users_lock = peer_users.lock().await;
            let mut sources_lock = sources.lock().await;
            for key in removed_users {
                peer_users_lock.remove(&key);
                sources_lock.remove(&key);
            }
        }
        _ => {}
    }
}

async fn cleanup_dead_remote_users(
    dead: &HashSet<(String, String, String)>,
    rooms: &RoomMap,
    remote_users: &RemoteUsersMap,
    sources: &RemoteUserSourcesMap,
    source_id: &str,
    times: &ChannelCreationTimesMap,
    _cluster_tx: &tokio::sync::broadcast::Sender<String>,
) -> HashSet<String> {
    let mut affected_rooms = HashSet::new();
    for (room_id, channel_id, user_id) in dead {
        let should_remove = {
            let key = (room_id.clone(), channel_id.clone(), user_id.clone());
            let mut sources_lock = sources.lock().await;
            match sources_lock.get_mut(&key) {
                Some(source_ids) => {
                    source_ids.remove(source_id);
                    if source_ids.is_empty() {
                        sources_lock.remove(&key);
                        true
                    } else {
                        false
                    }
                }
                None => true,
            }
        };
        if !should_remove {
            continue;
        }

        let removed = {
            let mut remote_lock = remote_users.lock().await;
            remove_remote_user(&mut remote_lock, room_id, channel_id, user_id)
        };
        if !removed {
            continue;
        }
        {
            let rooms_lock = rooms.lock().await;
            if let Some(room) = rooms_lock.get(room_id)
                && let Some(channel) = room.get(channel_id)
            {
                let notify = serde_json::to_string(&SignalMessage {
                    msg_type: "user-left".into(),
                    user_id: Some(user_id.clone()),
                    target: None,
                    data: None,
                })
                .unwrap();
                for (_, (tx, _)) in channel.iter() {
                    let _ = tx.try_send(Ok(Message::Text(notify.clone().into())));
                }
            }
        }
        affected_rooms.insert(room_id.clone());
    }
    for room_id in &affected_rooms {
        broadcast_channel_list(rooms, remote_users, times, room_id).await;
    }
    affected_rooms
}

async fn schedule_empty_room_cleanup(state: &AppState, room_id: &str) {
    let has_local_room = state.rooms.lock().await.contains_key(room_id);
    let has_remote_users = state
        .remote_users
        .lock()
        .await
        .get(room_id)
        .is_some_and(|room| room.values().any(|channel| !channel.is_empty()));

    if !has_local_room {
        if !has_remote_users {
            state.channel_creation_times.lock().await.remove(room_id);
            state.room_cleanup_generations.lock().await.remove(room_id);
        }
        return;
    }

    let local_is_empty = state
        .rooms
        .lock()
        .await
        .get(room_id)
        .is_some_and(|room| room.values().all(HashMap::is_empty));
    if !local_is_empty || has_remote_users {
        return;
    }

    let generation = {
        let mut generations = state.room_cleanup_generations.lock().await;
        let next = generations.get(room_id).copied().unwrap_or(0) + 1;
        generations.insert(room_id.to_string(), next);
        next
    };

    let state = state.clone();
    let room_id = room_id.to_string();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(ROOM_EMPTY_GRACE_SECS)).await;
        if state
            .room_cleanup_generations
            .lock()
            .await
            .get(&room_id)
            .copied()
            != Some(generation)
        {
            return;
        }

        let has_remote_users = state
            .remote_users
            .lock()
            .await
            .get(&room_id)
            .is_some_and(|room| room.values().any(|channel| !channel.is_empty()));
        if has_remote_users {
            return;
        }

        let removed = {
            let mut rooms = state.rooms.lock().await;
            let is_empty = rooms
                .get(&room_id)
                .is_some_and(|room| room.values().all(HashMap::is_empty));
            if is_empty {
                rooms.remove(&room_id);
                true
            } else {
                false
            }
        };
        if removed {
            state.channel_creation_times.lock().await.remove(&room_id);
            let mut generations = state.room_cleanup_generations.lock().await;
            if generations.get(&room_id).copied() == Some(generation) {
                generations.remove(&room_id);
            }
        }
    });
}

fn remove_remote_user(
    remote_users: &mut HashMap<String, HashMap<String, HashMap<String, UserStatus>>>,
    room_id: &str,
    channel_id: &str,
    user_id: &str,
) -> bool {
    let mut removed = false;
    let mut remove_room = false;
    if let Some(room) = remote_users.get_mut(room_id) {
        let mut remove_channel = false;
        if let Some(channel) = room.get_mut(channel_id) {
            removed = channel.remove(user_id).is_some();
            remove_channel = channel.is_empty();
        }
        if remove_channel {
            room.remove(channel_id);
        }
        remove_room = room.is_empty();
    }
    if remove_room {
        remote_users.remove(room_id);
    }
    removed
}

async fn handle_cluster_message(
    msg: &ClusterMessage,
    rooms: &RoomMap,
    remote_users: &RemoteUsersMap,
    state: &AppState,
) {
    {
        let mut ids = state.recent_cluster_msg_ids.lock().await;
        if ids.contains(&msg.msg_id) {
            return;
        }
        ids.insert(msg.msg_id.clone());

        let mut history = state.cluster_msg_history.lock().await;
        history.push_back(msg.msg_id.clone());
        if history.len() > 1000
            && let Some(oldest) = history.pop_front()
        {
            ids.remove(&oldest);
        }
    }

    match msg.msg_type.as_str() {
        "user-joined" => {
            if let Some(ref status) = msg.status {
                state
                    .room_cleanup_generations
                    .lock()
                    .await
                    .remove(&msg.room_id);
                let is_new_user = {
                    let mut rl = remote_users.lock().await;
                    rl.entry(msg.room_id.clone())
                        .or_default()
                        .entry(msg.channel_id.clone())
                        .or_default()
                        .insert(msg.user_id.clone(), status.clone())
                        .is_none()
                };
                {
                    let mut times = state.channel_creation_times.lock().await;
                    times
                        .entry(msg.room_id.clone())
                        .or_default()
                        .entry(msg.channel_id.clone())
                        .or_insert_with(current_unix_secs);
                }
                if is_new_user {
                    let rooms_lock = rooms.lock().await;
                    if let Some(room) = rooms_lock.get(&msg.room_id)
                        && let Some(channel) = room.get(&msg.channel_id)
                    {
                        let notify = serde_json::to_string(&SignalMessage {
                            msg_type: "user-joined".into(),
                            user_id: Some(msg.user_id.clone()),
                            target: None,
                            data: msg.data.clone(),
                        })
                        .unwrap();
                        for (_, (tx, _)) in channel.iter() {
                            let _ = tx.try_send(Ok(Message::Text(notify.clone().into())));
                        }
                    }
                }
                broadcast_channel_list(
                    rooms,
                    remote_users,
                    &state.channel_creation_times,
                    &msg.room_id,
                )
                .await;
            }
        }
        "user-left" | "user-kicked" => {
            let removed = {
                let mut rl = remote_users.lock().await;
                remove_remote_user(&mut rl, &msg.room_id, &msg.channel_id, &msg.user_id)
            };
            if !removed {
                return;
            }
            {
                let mtype = if msg.msg_type == "user-kicked" {
                    "user-kicked"
                } else {
                    "user-left"
                };
                let rooms_lock = rooms.lock().await;
                if let Some(room) = rooms_lock.get(&msg.room_id)
                    && let Some(channel) = room.get(&msg.channel_id)
                {
                    let notify = serde_json::to_string(&SignalMessage {
                        msg_type: mtype.into(),
                        user_id: Some(msg.user_id.clone()),
                        target: None,
                        data: None,
                    })
                    .unwrap();
                    for (_, (tx, _)) in channel.iter() {
                        let _ = tx.try_send(Ok(Message::Text(notify.clone().into())));
                    }
                }
            }
            broadcast_channel_list(
                rooms,
                remote_users,
                &state.channel_creation_times,
                &msg.room_id,
            )
            .await;
            schedule_empty_room_cleanup(state, &msg.room_id).await;
        }
        "user-update" => {
            if let Some(ref status) = msg.status {
                {
                    let mut rl = remote_users.lock().await;
                    if let Some(room) = rl.get_mut(&msg.room_id)
                        && let Some(channel) = room.get_mut(&msg.channel_id)
                        && let Some(existing) = channel.get_mut(&msg.user_id)
                    {
                        *existing = status.clone();
                    }
                }
                {
                    let rooms_lock = rooms.lock().await;
                    if let Some(room) = rooms_lock.get(&msg.room_id)
                        && let Some(channel) = room.get(&msg.channel_id)
                    {
                        let full_data = serde_json::to_value(status).unwrap();
                        let notify = serde_json::to_string(&SignalMessage {
                            msg_type: "user-update".into(),
                            user_id: Some(msg.user_id.clone()),
                            target: None,
                            data: Some(full_data),
                        })
                        .unwrap();
                        for (_, (tx, _)) in channel.iter() {
                            let _ = tx.try_send(Ok(Message::Text(notify.clone().into())));
                        }
                    }
                }
                broadcast_channel_list(
                    rooms,
                    remote_users,
                    &state.channel_creation_times,
                    &msg.room_id,
                )
                .await;
            }
        }
        "cam-toggle" | "screen-toggle" => {
            if msg.msg_type == "screen-toggle"
                && let Some(enabled) = msg
                    .data
                    .as_ref()
                    .and_then(|d| d.get("enabled"))
                    .and_then(|v| v.as_bool())
            {
                let mut rl = remote_users.lock().await;
                if let Some(room) = rl.get_mut(&msg.room_id)
                    && let Some(channel) = room.get_mut(&msg.channel_id)
                    && let Some(s) = channel.get_mut(&msg.user_id)
                {
                    s.is_screen_sharing = enabled;
                }
            }
            {
                let rooms_lock = rooms.lock().await;
                if let Some(room) = rooms_lock.get(&msg.room_id)
                    && let Some(channel) = room.get(&msg.channel_id)
                {
                    let notify = serde_json::to_string(&SignalMessage {
                        msg_type: msg.msg_type.clone(),
                        user_id: Some(msg.user_id.clone()),
                        target: None,
                        data: msg.data.clone(),
                    })
                    .unwrap();
                    for (_, (tx, _)) in channel.iter() {
                        let _ = tx.try_send(Ok(Message::Text(notify.clone().into())));
                    }
                }
            }
            if msg.msg_type == "screen-toggle" {
                broadcast_channel_list(
                    rooms,
                    remote_users,
                    &state.channel_creation_times,
                    &msg.room_id,
                )
                .await;
            }
        }
        "rename-channel" => {
            if let Some(ref data) = msg.data {
                let new_name = data
                    .get("newName")
                    .and_then(|v| v.as_str())
                    .and_then(normalize_channel_id);
                if let Some(new_name) = new_name {
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
                    })
                    .unwrap();

                    let mut rl = remote_users.lock().await;
                    if let Some(room) = rl.get_mut(&msg.room_id)
                        && let Some(channel_data) = room.remove(&msg.channel_id)
                    {
                        room.insert(new_name.clone(), channel_data);
                    }
                    drop(rl);

                    {
                        let mut times = state.channel_creation_times.lock().await;
                        if let Some(room_times) = times.get_mut(&msg.room_id)
                            && let Some(created_at) = room_times.remove(&msg.channel_id)
                        {
                            room_times.insert(new_name.clone(), created_at);
                        }
                    }

                    // Forward rename-channel to local WebSocket clients in this room
                    let rooms_lock = rooms.lock().await;
                    if let Some(room) = rooms_lock.get(&msg.room_id) {
                        for (_ch_name, channel) in room.iter() {
                            for (_uid, (tx, _)) in channel.iter() {
                                let _ =
                                    tx.try_send(Ok(Message::Text(rename_notify.clone().into())));
                            }
                        }
                    }
                    drop(rooms_lock);

                    broadcast_channel_list(
                        rooms,
                        remote_users,
                        &state.channel_creation_times,
                        &msg.room_id,
                    )
                    .await;
                }
            }
        }
        "delete-channel" => {
            let mut rl = remote_users.lock().await;
            if let Some(room) = rl.get_mut(&msg.room_id) {
                room.remove(&msg.channel_id);
                if room.is_empty() {
                    rl.remove(&msg.room_id);
                }
            }
            drop(rl);
            {
                let mut times = state.channel_creation_times.lock().await;
                if let Some(room_times) = times.get_mut(&msg.room_id) {
                    room_times.remove(&msg.channel_id);
                    if room_times.is_empty() {
                        times.remove(&msg.room_id);
                    }
                }
            }
            broadcast_channel_list(
                rooms,
                remote_users,
                &state.channel_creation_times,
                &msg.room_id,
            )
            .await;
        }
        "signal" => {
            if let Some(ref signal_json) = msg.signal_msg
                && let Ok(signal) = serde_json::from_str::<SignalMessage>(signal_json)
            {
                let target_uid = signal.target.as_ref().cloned().unwrap_or_default();
                if !target_uid.is_empty() {
                    let rooms_lock = rooms.lock().await;
                    if let Some(room) = rooms_lock.get(&msg.room_id)
                        && let Some(channel) = room.get(&msg.channel_id)
                        && let Some((target_tx, _)) = channel.get(&target_uid)
                    {
                        let forwarded = serde_json::to_string(&signal).unwrap();
                        let _ = target_tx.try_send(Ok(Message::Text(forwarded.into())));
                    }
                }
            }
        }
        _ => {}
    }
}

pub(crate) fn cluster_broadcast(
    cluster_tx: &tokio::sync::broadcast::Sender<String>,
    msg: &ClusterMessage,
) {
    let mut msg_with_id = msg.clone();
    if msg_with_id.msg_id.is_empty() {
        msg_with_id.msg_id = Uuid::new_v4().to_string();
    }
    if let Ok(json) = serde_json::to_string(&msg_with_id) {
        let _ = cluster_tx.send(json);
    }
}

pub(crate) async fn broadcast_channel_list(
    rooms: &RoomMap,
    remote_users: &RemoteUsersMap,
    times: &ChannelCreationTimesMap,
    room_id: &str,
) {
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
            let created_at = times_lock
                .get(room_id)
                .and_then(|t| t.get(cid))
                .copied()
                .unwrap_or(0);
            channel_list.insert(
                cid.clone(),
                RoomStatus {
                    name: cid.clone(),
                    users: user_map,
                    created_at,
                },
            );
        }
    }

    if let Some(remote_room) = remote_room {
        for (cid, users) in remote_room.iter() {
            let created_at = times_lock
                .get(room_id)
                .and_then(|t| t.get(cid))
                .copied()
                .unwrap_or(0);
            let entry = channel_list
                .entry(cid.clone())
                .or_insert_with(|| RoomStatus {
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
    })
    .unwrap();

    if let Some(room) = local_room {
        for users in room.values() {
            for (tx, _) in users.values() {
                let _ = tx.try_send(Ok(Message::Text(msg.clone().into())));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removing_the_last_remote_user_prunes_empty_parents() {
        let mut remote_users = HashMap::new();
        remote_users
            .entry("room".to_string())
            .or_insert_with(HashMap::new)
            .entry("General".to_string())
            .or_insert_with(HashMap::new)
            .insert(
                Uuid::nil().to_string(),
                UserStatus {
                    nickname: "Guest".to_string(),
                    avatar: None,
                    is_gif: false,
                    static_frame: None,
                    is_muted: false,
                    is_deafened: false,
                    is_screen_sharing: false,
                    is_low_bandwidth_mode: false,
                    is_on_the_go_mode: false,
                },
            );

        assert!(remove_remote_user(
            &mut remote_users,
            "room",
            "General",
            &Uuid::nil().to_string()
        ));
        assert!(remote_users.is_empty());
    }
}
