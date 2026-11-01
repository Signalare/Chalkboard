// SPDX-License-Identifier: MIT OR Apache-2.0

//! HDMI-CEC subsystems, device traits, mock test harness, and services (#25, SPEC §13).

pub mod adapter;
pub mod linux;
pub mod mock;
pub mod protocol;
pub mod service;

pub use adapter::{CecAdapter, CecAdapterStatus, CecError};
pub use mock::{MockCecAdapter, SimulatedSinkState};
pub use protocol::{
    CecMessage, LOG_ADDR_BROADCAST, LOG_ADDR_PLAYBACK_1, LOG_ADDR_TV, OP_ACTIVE_SOURCE,
    OP_GIVE_DEVICE_POWER_STATUS, OP_IMAGE_VIEW_ON, OP_REPORT_DEVICE_POWER_STATUS, OP_SET_OSD_NAME,
    OP_STANDBY, PowerStatus,
};
pub use service::{CecService, CecStatusSummary};
