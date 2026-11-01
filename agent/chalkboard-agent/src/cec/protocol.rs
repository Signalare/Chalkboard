// SPDX-License-Identifier: MIT OR Apache-2.0

//! HDMI-CEC protocol definitions, opcodes, and message serialization (SPEC §13, CEC-001..CEC-003).

use serde::{Deserialize, Serialize};

/// Standard CEC logical addresses (CEC Table 5).
pub const LOG_ADDR_TV: u8 = 0x0;
pub const LOG_ADDR_PLAYBACK_1: u8 = 0x4;
pub const LOG_ADDR_PLAYBACK_2: u8 = 0x8;
pub const LOG_ADDR_PLAYBACK_3: u8 = 0x9;
pub const LOG_ADDR_BROADCAST: u8 = 0xF;

/// Standard CEC opcodes (CEC Table 7).
pub const OP_FEATURE_ABORT: u8 = 0x00;
pub const OP_IMAGE_VIEW_ON: u8 = 0x04;
pub const OP_STANDBY: u8 = 0x36;
pub const OP_SET_OSD_STRING: u8 = 0x64;
pub const OP_GIVE_OSD_NAME: u8 = 0x46;
pub const OP_SET_OSD_NAME: u8 = 0x47;
pub const OP_ROUTING_CHANGE: u8 = 0x80;
pub const OP_ACTIVE_SOURCE: u8 = 0x82;
pub const OP_GIVE_PHYSICAL_ADDR: u8 = 0x83;
pub const OP_REPORT_PHYSICAL_ADDR: u8 = 0x84;
pub const OP_REQUEST_ACTIVE_SOURCE: u8 = 0x85;
pub const OP_SET_STREAM_PATH: u8 = 0x86;
pub const OP_GIVE_DEVICE_POWER_STATUS: u8 = 0x8F;
pub const OP_REPORT_DEVICE_POWER_STATUS: u8 = 0x90;

/// Device power status values reported by CEC opcode 0x90 (CEC Table 28).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PowerStatus {
    On = 0x00,
    Standby = 0x01,
    InTransitionToOn = 0x02,
    InTransitionToStandby = 0x03,
    Unknown = 0xFF,
}

impl PowerStatus {
    pub fn from_u8(val: u8) -> Self {
        match val {
            0x00 => PowerStatus::On,
            0x01 => PowerStatus::Standby,
            0x02 => PowerStatus::InTransitionToOn,
            0x03 => PowerStatus::InTransitionToStandby,
            _ => PowerStatus::Unknown,
        }
    }

    pub fn to_u8(self) -> u8 {
        self as u8
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            PowerStatus::On => "on",
            PowerStatus::Standby => "standby",
            PowerStatus::InTransitionToOn => "transitioning-to-on",
            PowerStatus::InTransitionToStandby => "transitioning-to-standby",
            PowerStatus::Unknown => "unknown",
        }
    }
}

/// A structured HDMI-CEC message frame.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CecMessage {
    pub initiator: u8,
    pub destination: u8,
    pub opcode: Option<u8>,
    pub parameters: Vec<u8>,
}

impl CecMessage {
    /// Create a raw message with optional opcode and parameters.
    pub fn new(initiator: u8, destination: u8, opcode: Option<u8>, parameters: Vec<u8>) -> Self {
        Self {
            initiator: initiator & 0x0F,
            destination: destination & 0x0F,
            opcode,
            parameters,
        }
    }

    /// Construct `Image View On` directed to TV (CEC-003).
    pub fn image_view_on(initiator: u8, destination: u8) -> Self {
        Self::new(initiator, destination, Some(OP_IMAGE_VIEW_ON), Vec::new())
    }

    /// Construct `Active Source` broadcast with physical address (CEC-003).
    pub fn active_source(initiator: u8, physical_address: [u8; 2]) -> Self {
        Self::new(
            initiator,
            LOG_ADDR_BROADCAST,
            Some(OP_ACTIVE_SOURCE),
            physical_address.to_vec(),
        )
    }

    /// Construct `Standby` command directed to TV or broadcast (CEC-005).
    pub fn standby(initiator: u8, destination: u8) -> Self {
        Self::new(initiator, destination, Some(OP_STANDBY), Vec::new())
    }

    /// Construct `Set OSD Name` with truncated ASCII string (≤ 14 characters, CEC-002).
    pub fn set_osd_name(initiator: u8, destination: u8, name: &str) -> Self {
        let ascii_bytes: Vec<u8> = name
            .chars()
            .filter(|c| c.is_ascii() && !c.is_ascii_control())
            .take(14)
            .map(|c| c as u8)
            .collect();
        Self::new(initiator, destination, Some(OP_SET_OSD_NAME), ascii_bytes)
    }

    /// Construct `Give Device Power Status` directed to TV (CEC-006).
    pub fn give_device_power_status(initiator: u8, destination: u8) -> Self {
        Self::new(
            initiator,
            destination,
            Some(OP_GIVE_DEVICE_POWER_STATUS),
            Vec::new(),
        )
    }

    /// Construct `Report Device Power Status` response.
    pub fn report_device_power_status(initiator: u8, destination: u8, status: PowerStatus) -> Self {
        Self::new(
            initiator,
            destination,
            Some(OP_REPORT_DEVICE_POWER_STATUS),
            vec![status.to_u8()],
        )
    }

    /// Serialize message into raw CEC wire bytes.
    pub fn to_raw_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(2 + self.parameters.len());
        let header = (self.initiator << 4) | (self.destination & 0x0F);
        bytes.push(header);
        if let Some(op) = self.opcode {
            bytes.push(op);
            bytes.extend_from_slice(&self.parameters);
        }
        bytes
    }

    /// Parse a CEC message from raw wire bytes.
    pub fn from_raw_bytes(bytes: &[u8]) -> Result<Self, String> {
        if bytes.is_empty() {
            return Err("Empty CEC frame".to_string());
        }
        let initiator = (bytes[0] >> 4) & 0x0F;
        let destination = bytes[0] & 0x0F;

        if bytes.len() == 1 {
            // Polling message (header only)
            return Ok(Self::new(initiator, destination, None, Vec::new()));
        }

        let opcode = Some(bytes[1]);
        let parameters = bytes[2..].to_vec();
        Ok(Self::new(initiator, destination, opcode, parameters))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_image_view_on_serialization() {
        let msg = CecMessage::image_view_on(LOG_ADDR_PLAYBACK_1, LOG_ADDR_TV);
        let bytes = msg.to_raw_bytes();
        assert_eq!(bytes, vec![0x40, 0x04]);

        let parsed = CecMessage::from_raw_bytes(&bytes).unwrap();
        assert_eq!(parsed, msg);
    }

    #[test]
    fn test_active_source_serialization() {
        let msg = CecMessage::active_source(LOG_ADDR_PLAYBACK_1, [0x10, 0x00]);
        let bytes = msg.to_raw_bytes();
        assert_eq!(bytes, vec![0x4F, 0x82, 0x10, 0x00]);

        let parsed = CecMessage::from_raw_bytes(&bytes).unwrap();
        assert_eq!(parsed, msg);
    }

    #[test]
    fn test_set_osd_name_truncation() {
        let long_name = "Super Long Screen Name That Exceeds Fourteen Characters";
        let msg = CecMessage::set_osd_name(LOG_ADDR_PLAYBACK_1, LOG_ADDR_TV, long_name);
        assert_eq!(msg.parameters.len(), 14);
        assert_eq!(
            std::str::from_utf8(&msg.parameters).unwrap(),
            "Super Long Scr"
        );

        let bytes = msg.to_raw_bytes();
        assert_eq!(bytes[0], 0x40);
        assert_eq!(bytes[1], OP_SET_OSD_NAME);
        assert_eq!(&bytes[2..], b"Super Long Scr");
    }

    #[test]
    fn test_power_status_roundtrip() {
        let msg = CecMessage::report_device_power_status(
            LOG_ADDR_TV,
            LOG_ADDR_PLAYBACK_1,
            PowerStatus::Standby,
        );
        let bytes = msg.to_raw_bytes();
        assert_eq!(bytes, vec![0x04, 0x90, 0x01]);

        let parsed = CecMessage::from_raw_bytes(&bytes).unwrap();
        assert_eq!(parsed.opcode, Some(OP_REPORT_DEVICE_POWER_STATUS));
        assert_eq!(
            PowerStatus::from_u8(parsed.parameters[0]),
            PowerStatus::Standby
        );
    }
}
