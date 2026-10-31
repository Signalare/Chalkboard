// SPDX-License-Identifier: MIT OR Apache-2.0

//! Unix domain socket IPC protocol for agent-to-renderer communication (RND-006).

use serde::{Deserialize, Serialize};

/// Live sold-out state for a single menu item (RND-006, RND-008).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SoldOutItem {
    pub item_id: String,
    pub sold_out: bool,
}

/// Live announcement overlay state (RND-006, RND-008).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnnouncementMsg {
    pub text: String,
    pub visible: bool,
}

/// Commands sent from the agent to the renderer over the Unix socket (RND-006).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum RendererCommand {
    /// Instructs the renderer to switch to a content-addressed snapshot (RND-006).
    LoadSnapshot { hash: String },

    /// Applies live state toggles without a full reload (RND-006, RND-008).
    ApplyLiveState {
        #[serde(default)]
        sold_out: Vec<SoldOutItem>,
        #[serde(default)]
        announcement: Option<AnnouncementMsg>,
    },

    /// Toggles the screen identify overlay (RND-006, RND-009).
    Identify {
        enabled: bool,
        node_name: String,
        screen_name: String,
        ip: String,
    },

    /// Requests a rendered screenshot of the current frame (RND-006, RND-010).
    Screenshot,

    /// Queries the renderer's current mode and active snapshot status (RND-006).
    Status,
}

/// Responses returned by the renderer to the agent over the Unix socket (RND-006).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum RendererResponse {
    /// Command completed successfully.
    Ok {
        #[serde(default)]
        message: Option<String>,
    },

    /// Current status of the renderer state machine.
    Status {
        mode: String,
        active_hash: String,
        fallback_hash: String,
        subpage: u32,
        subpage_total: u32,
    },

    /// Captured frame buffer metadata.
    Screenshot {
        width: u32,
        height: u32,
        frame_hash: u64,
    },

    /// Error encountered executing the command.
    Error { error: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_and_response_serialization() {
        let cmd = RendererCommand::LoadSnapshot {
            hash: "1111222233334444".to_string(),
        };
        let json = serde_json::to_string(&cmd).unwrap();
        assert!(json.contains("\"command\":\"load_snapshot\""));
        let parsed: RendererCommand = serde_json::from_str(&json).unwrap();
        assert_eq!(cmd, parsed);

        let resp = RendererResponse::Status {
            mode: "content".to_string(),
            active_hash: "hash1".to_string(),
            fallback_hash: "hash0".to_string(),
            subpage: 1,
            subpage_total: 3,
        };
        let r_json = serde_json::to_string(&resp).unwrap();
        assert!(r_json.contains("\"status\":\"status\""));
        let r_parsed: RendererResponse = serde_json::from_str(&r_json).unwrap();
        assert_eq!(resp, r_parsed);
    }
}
