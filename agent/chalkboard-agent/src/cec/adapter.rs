// SPDX-License-Identifier: MIT OR Apache-2.0

//! Abstract CEC adapter interface for physical and mock hardware devices (CEC-001, CEC-006).

use serde::{Deserialize, Serialize};
use std::fmt;

use super::protocol::{CecMessage, PowerStatus};

/// Errors encountered during HDMI-CEC communication.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CecError {
    DeviceNotFound(String),
    PermissionDenied(String),
    TransmitNack,
    TransmitTimeout,
    InvalidParameter(String),
    BusError(String),
    NotSupported(String),
}

impl fmt::Display for CecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CecError::DeviceNotFound(dev) => write!(f, "CEC device not found: {dev}"),
            CecError::PermissionDenied(msg) => {
                write!(f, "Permission denied accessing CEC device: {msg}")
            }
            CecError::TransmitNack => write!(f, "CEC transmission not acknowledged (NACK)"),
            CecError::TransmitTimeout => write!(f, "CEC transmission timed out"),
            CecError::InvalidParameter(msg) => write!(f, "Invalid CEC parameter: {msg}"),
            CecError::BusError(msg) => write!(f, "CEC bus error: {msg}"),
            CecError::NotSupported(msg) => write!(f, "CEC operation not supported: {msg}"),
        }
    }
}

impl std::error::Error for CecError {}

/// Detailed status information for a CEC adapter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CecAdapterStatus {
    pub device: String,
    pub logical_address: u8,
    pub physical_address: String,
    pub power_status: PowerStatus,
    pub active_source: bool,
    pub osd_name: String,
    pub is_mock: bool,
}

/// Abstract trait representing an HDMI-CEC hardware or mock adapter.
pub trait CecAdapter: fmt::Debug + Send + Sync {
    /// Return the device path or mock identifier (e.g., `/dev/cec0` or `mock-cec0`).
    fn device_name(&self) -> &str;

    /// Return true if this is a software mock adapter.
    fn is_mock(&self) -> bool;

    /// Return the logical address allocated for this playback device (e.g. 0x4).
    fn logical_address(&self) -> u8;

    /// Return the physical address (e.g. `[0x10, 0x00]` for HDMI port 1.0.0.0).
    fn physical_address(&self) -> [u8; 2];

    /// Return the configured OSD name for this device.
    fn osd_name(&self) -> String;

    /// Set the OSD name for this device (CEC-002).
    fn set_osd_name(&self, name: &str) -> Result<(), CecError>;

    /// Transmit a CEC frame onto the bus.
    fn transmit(&self, msg: &CecMessage) -> Result<(), CecError>;

    /// Receive/poll a CEC frame from the bus (non-blocking).
    fn receive(&self) -> Result<Option<CecMessage>, CecError>;

    /// Query the power status of the connected TV/sink (CEC-006).
    fn query_power_status(&self) -> Result<PowerStatus, CecError>;

    /// Check if this device is currently the active source.
    fn is_active_source(&self) -> bool;

    /// Get current overall status summary for reporting.
    fn get_status(&self) -> CecAdapterStatus {
        let phys = self.physical_address();
        let phys_str = format!(
            "{}.{}.{}.{}",
            (phys[0] >> 4),
            (phys[0] & 0x0F),
            (phys[1] >> 4),
            (phys[1] & 0x0F)
        );
        let power_status = self.query_power_status().unwrap_or(PowerStatus::Unknown);
        CecAdapterStatus {
            device: self.device_name().to_string(),
            logical_address: self.logical_address(),
            physical_address: phys_str,
            power_status,
            active_source: self.is_active_source(),
            osd_name: self.osd_name(),
            is_mock: self.is_mock(),
        }
    }
}
