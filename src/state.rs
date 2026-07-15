use axum::extract::ws::Message;
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::Arc,
};
use tokio::sync::Mutex;
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UserStatus {
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
pub(crate) struct RoomStatus {
    pub(crate) name: String,
    pub(crate) users: HashMap<String, UserStatus>,
    #[serde(default)]
    pub created_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct SignalMessage {
    #[serde(rename = "type")]
    pub msg_type: String,
    pub target: Option<String>,
    pub data: Option<serde_json::Value>,
    #[serde(rename = "userId")]
    pub user_id: Option<String>,
}

pub(crate) type UserTx = tokio::sync::mpsc::Sender<Result<Message, axum::Error>>;
pub(crate) type ChannelMap = HashMap<String, HashMap<String, (UserTx, UserStatus)>>;
pub(crate) type RoomMap = Arc<Mutex<HashMap<String, ChannelMap>>>;
pub(crate) type RoomCleanupMap = Arc<Mutex<HashMap<String, u64>>>;
pub(crate) type RemoteUsersMap =
    Arc<Mutex<HashMap<String, HashMap<String, HashMap<String, UserStatus>>>>>;
pub(crate) type ChannelCreationTimesMap = Arc<Mutex<HashMap<String, HashMap<String, u64>>>>;
pub(crate) const ROOM_EMPTY_GRACE_SECS: u64 = 120;
pub(crate) const MAX_ROOM_ID_LEN: usize = 64;
pub(crate) const MAX_CHANNEL_ID_LEN: usize = 32;
pub(crate) const MAX_NICKNAME_LEN: usize = 32;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ClusterMessage {
    #[serde(rename = "type")]
    pub(crate) msg_type: String,
    pub(crate) room_id: String,
    pub(crate) channel_id: String,
    pub(crate) user_id: String,
    pub(crate) msg_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) status: Option<UserStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) data: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) signal_msg: Option<String>,
}

#[derive(Clone)]
pub(crate) struct AppState {
    pub(crate) rooms: RoomMap,
    pub(crate) room_cleanup_generations: RoomCleanupMap,
    pub(crate) room_creation_password: Option<String>,
    pub(crate) cluster_tx: tokio::sync::broadcast::Sender<String>,
    pub(crate) remote_users: RemoteUsersMap,
    pub(crate) channel_creation_times: ChannelCreationTimesMap,
    pub(crate) cluster_key: Option<String>,
    pub(crate) cluster_scheme: String,
    pub(crate) allowed_url: Option<String>,
    pub(crate) connected_peers: Arc<Mutex<HashSet<String>>>,
    pub recent_cluster_msg_ids: Arc<Mutex<HashSet<String>>>,
    pub cluster_msg_history: Arc<Mutex<VecDeque<String>>>,
    pub(crate) node_id: String,
}
