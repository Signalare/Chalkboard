// SPDX-License-Identifier: MIT OR Apache-2.0

//! Linux kernel CEC adapter (/dev/cec0, /dev/cec1) implementation (CEC-001..CEC-003).

use std::path::Path;
use std::sync::Mutex;

use super::adapter::{CecAdapter, CecError};
use super::protocol::{
    CecMessage, LOG_ADDR_PLAYBACK_1, LOG_ADDR_TV, OP_ACTIVE_SOURCE, PowerStatus,
};

#[cfg(target_os = "linux")]
#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
struct RawCecMsg {
    tx_ts: u64,
    rx_ts: u64,
    len: u32,
    timeout: u32,
    sequence: u32,
    flags: u32,
    msg: [u8; 16],
    reply: u8,
    rx_status: u8,
    tx_status: u8,
    tx_arb_lost_cnt: u8,
    tx_nack_cnt: u8,
    tx_low_drive_cnt: u8,
    tx_error_cnt: u8,
}

#[cfg(target_os = "linux")]
const CEC_TRANSMIT: libc::c_ulong = 0xc0386105;
#[cfg(target_os = "linux")]
const CEC_RECEIVE: libc::c_ulong = 0xc0386106;
#[cfg(target_os = "linux")]
const CEC_ADAP_G_PHYS_ADDR: libc::c_ulong = 0x80026101;

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

        #[allow(unused_mut)]
        let mut phys = [0x10, 0x00];

        #[cfg(target_os = "linux")]
        {
            use std::fs::OpenOptions;
            use std::os::unix::io::AsRawFd;

            if let Ok(file) = OpenOptions::new().read(true).write(true).open(device_path) {
                let mut raw_phys: u16 = 0;
                let ret =
                    unsafe { libc::ioctl(file.as_raw_fd(), CEC_ADAP_G_PHYS_ADDR, &mut raw_phys) };
                if ret == 0 && raw_phys != 0xffff {
                    phys = [((raw_phys >> 8) & 0xff) as u8, (raw_phys & 0xff) as u8];
                }
            }
        }

        // Default to Playback 1 logical address (0x4) and root HDMI port [0x10, 0x00]
        Ok(Self {
            device_path: device_path.to_string(),
            logical_address: LOG_ADDR_PLAYBACK_1,
            physical_address: phys,
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

            let mut raw_msg = RawCecMsg::default();
            raw_msg.len = raw.len() as u32;
            raw_msg.timeout = 1000;
            raw_msg.msg[..raw.len()].copy_from_slice(&raw);

            let ret = unsafe { libc::ioctl(file.as_raw_fd(), CEC_TRANSMIT, &mut raw_msg) };
            if ret < 0 {
                return Err(CecError::BusError(
                    std::io::Error::last_os_error().to_string(),
                ));
            }

            // Record active source locally if we sent it
            if msg.opcode == Some(OP_ACTIVE_SOURCE) {
                *self.active_source.lock().unwrap() = true;
            }

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
            use std::fs::OpenOptions;
            use std::os::unix::fs::OpenOptionsExt;
            use std::os::unix::io::AsRawFd;

            let file = OpenOptions::new()
                .read(true)
                .write(true)
                .custom_flags(libc::O_NONBLOCK)
                .open(&self.device_path)
                .map_err(|e| CecError::PermissionDenied(e.to_string()))?;

            let mut raw_msg = RawCecMsg::default();
            raw_msg.timeout = 500;
            let ret = unsafe { libc::ioctl(file.as_raw_fd(), CEC_RECEIVE, &mut raw_msg) };
            if ret == 0 && raw_msg.len > 0 {
                let slice = &raw_msg.msg[..raw_msg.len as usize];
                if let Ok(msg) = CecMessage::from_raw_bytes(slice) {
                    return Ok(Some(msg));
                }
            }
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
            use std::fs::OpenOptions;
            use std::os::unix::fs::OpenOptionsExt;
            use std::os::unix::io::AsRawFd;

            let file = OpenOptions::new()
                .read(true)
                .write(true)
                .custom_flags(libc::O_NONBLOCK)
                .open(&self.device_path)
                .map_err(|e| CecError::PermissionDenied(e.to_string()))?;

            let query = CecMessage::give_device_power_status(self.logical_address, LOG_ADDR_TV);
            let raw = query.to_raw_bytes();

            let mut raw_msg = RawCecMsg::default();
            raw_msg.len = raw.len() as u32;
            raw_msg.timeout = 1000;
            raw_msg.reply = 0x90; // CEC_MSG_REPORT_POWER_STATUS
            raw_msg.msg[..raw.len()].copy_from_slice(&raw);

            let ret = unsafe { libc::ioctl(file.as_raw_fd(), CEC_TRANSMIT, &mut raw_msg) };
            if ret == 0 && (raw_msg.rx_status & 1) != 0 && raw_msg.len >= 3 {
                Ok(PowerStatus::from_u8(raw_msg.msg[2]))
            } else {
                Ok(PowerStatus::Unknown)
            }
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
