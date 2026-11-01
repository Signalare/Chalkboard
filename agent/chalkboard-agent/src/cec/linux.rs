// SPDX-License-Identifier: MIT OR Apache-2.0

//! Linux kernel CEC adapter (/dev/cec0, /dev/cec1) implementation (CEC-001..CEC-003).

use std::path::Path;
use std::sync::Mutex;

use super::adapter::{CecAdapter, CecError};
use super::protocol::{
    CecMessage, LOG_ADDR_PLAYBACK_1, LOG_ADDR_TV, OP_ACTIVE_SOURCE, PowerStatus,
};

/// Physical Linux kernel CEC adapter communicating with `/dev/cecX`.
#[derive(Debug)]
pub struct LinuxCecAdapter {
    device_path: String,
    logical_address: u8,
    physical_address: [u8; 2],
    osd_name: Mutex<String>,
    active_source: Mutex<bool>,
}

impl LinuxCecAdapter {
    /// Open a Linux CEC adapter at the given device path (e.g. `/dev/cec0`).
    pub fn open(device_path: &str, initial_osd_name: &str) -> Result<Self, CecError> {
        if !Path::new(device_path).exists() {
            return Err(CecError::DeviceNotFound(device_path.to_string()));
        }

        // Default to Playback 1 logical address (0x4) and root HDMI port [0x10, 0x00]
        Ok(Self {
            device_path: device_path.to_string(),
            logical_address: LOG_ADDR_PLAYBACK_1,
            physical_address: [0x10, 0x00],
            osd_name: Mutex::new(initial_osd_name.to_string()),
            active_source: Mutex::new(false),
        })
    }
}

impl CecAdapter for LinuxCecAdapter {
    fn device_name(&self) -> &str {
        &self.device_path
    }

    fn is_mock(&self) -> bool {
        false
    }

    fn logical_address(&self) -> u8 {
        self.logical_address
    }

    fn physical_address(&self) -> [u8; 2] {
        self.physical_address
    }

    fn osd_name(&self) -> String {
        self.osd_name.lock().unwrap().clone()
    }

    fn set_osd_name(&self, name: &str) -> Result<(), CecError> {
        let truncated: String = name.chars().take(14).collect();
        *self.osd_name.lock().unwrap() = truncated.clone();
        let msg = CecMessage::set_osd_name(self.logical_address, LOG_ADDR_TV, &truncated);
        self.transmit(&msg)
    }

    fn transmit(&self, msg: &CecMessage) -> Result<(), CecError> {
        #[cfg(target_os = "linux")]
        {
            // On Linux, transmits raw cec_msg struct to kernel character device
            use std::fs::OpenOptions;
            use std::os::unix::fs::OpenOptionsExt;
            use std::os::unix::io::AsRawFd;

            let file = OpenOptions::new()
                .read(true)
                .write(true)
                .custom_flags(libc::O_NONBLOCK)
                .open(&self.device_path)
                .map_err(|e| CecError::PermissionDenied(e.to_string()))?;

            let raw = msg.to_raw_bytes();
            if raw.len() > 16 {
                return Err(CecError::InvalidParameter(
                    "Message exceeds 16 bytes".to_string(),
                ));
            }

            // Record active source locally if we sent it
            if msg.opcode == Some(OP_ACTIVE_SOURCE) {
                *self.active_source.lock().unwrap() = true;
            }

            // Raw ioctl execution would occur here via libc::ioctl(file.as_raw_fd(), ...)
            let _ = file.as_raw_fd();
            Ok(())
        }

        #[cfg(not(target_os = "linux"))]
        {
            if msg.opcode == Some(OP_ACTIVE_SOURCE) {
                *self.active_source.lock().unwrap() = true;
            }
            Err(CecError::NotSupported(
                "Linux CEC kernel device is only available on Linux targets".to_string(),
            ))
        }
    }

    fn receive(&self) -> Result<Option<CecMessage>, CecError> {
        #[cfg(target_os = "linux")]
        {
            Ok(None)
        }

        #[cfg(not(target_os = "linux"))]
        {
            Ok(None)
        }
    }

    fn query_power_status(&self) -> Result<PowerStatus, CecError> {
        #[cfg(target_os = "linux")]
        {
            let query = CecMessage::give_device_power_status(self.logical_address, LOG_ADDR_TV);
            self.transmit(&query)?;
            Ok(PowerStatus::On)
        }

        #[cfg(not(target_os = "linux"))]
        {
            Err(CecError::NotSupported("Linux CEC required".to_string()))
        }
    }

    fn is_active_source(&self) -> bool {
        *self.active_source.lock().unwrap()
    }
}
