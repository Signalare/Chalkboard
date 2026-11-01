// SPDX-License-Identifier: MIT OR Apache-2.0

//! CEC service coordinating adapters, power commands, and status reporting (CEC-001..CEC-006).

use serde::{Deserialize, Serialize};
use std::sync::Arc;

use super::adapter::{CecAdapter, CecAdapterStatus, CecError};
use super::mock::MockCecAdapter;
use super::protocol::{CecMessage, LOG_ADDR_TV, PowerStatus};

/// High-level CEC status summary reported to web app and management APIs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CecStatusSummary {
    pub enabled: bool,
    pub is_mock: bool,
    pub primary_power: PowerStatus,
    pub active_source: bool,
    pub adapters: Vec<CecAdapterStatus>,
}

/// Service managing HDMI-CEC adapters for physical or simulated displays.
#[derive(Debug, Clone)]
pub struct CecService {
    primary: Arc<dyn CecAdapter>,
    secondary: Option<Arc<dyn CecAdapter>>,
}

impl CecService {
    /// Create a CEC service explicitly using an in-memory mock adapter.
    pub fn new_mock(osd_name: &str) -> Self {
        let mock = MockCecAdapter::new("mock-cec0", 0x4, [0x10, 0x00], osd_name);
        Self {
            primary: Arc::new(mock),
            secondary: None,
        }
    }

    /// Automatically create a CEC service: uses physical adapters on Linux if present,
    /// or falls back to `MockCecAdapter` if `force_mock` is true or hardware is absent.
    pub fn new_auto(force_mock: bool, osd_name: &str) -> Self {
        if force_mock {
            return Self::new_mock(osd_name);
        }

        #[cfg(target_os = "linux")]
        {
            if let Ok(primary_linux) = super::linux::LinuxCecAdapter::open("/dev/cec0", osd_name) {
                let secondary_linux =
                    super::linux::LinuxCecAdapter::open("/dev/cec1", osd_name).ok();
                return Self {
                    primary: Arc::new(primary_linux),
                    secondary: secondary_linux.map(|s| Arc::new(s) as Arc<dyn CecAdapter>),
                };
            }
        }

        // Default fallback to mock device harness (#25)
        Self::new_mock(osd_name)
    }

    /// Access reference to primary adapter.
    pub fn primary(&self) -> &dyn CecAdapter {
        self.primary.as_ref()
    }

    /// Send `Image View On` to turn on the TV, followed by `Active Source` (CEC-003).
    pub fn power_on(&self) -> Result<(), CecError> {
        let log_addr = self.primary.logical_address();
        let phys_addr = self.primary.physical_address();

        // 1. Send Image View On (0x04) directly to TV (0x0)
        let img_on = CecMessage::image_view_on(log_addr, LOG_ADDR_TV);
        self.primary.transmit(&img_on)?;

        // 2. Broadcast Active Source (0x82) with physical address
        let active = CecMessage::active_source(log_addr, phys_addr);
        self.primary.transmit(&active)?;

        Ok(())
    }

    /// Send `Standby` (0x36) command to power down the connected TV (CEC-005).
    pub fn standby(&self) -> Result<(), CecError> {
        let log_addr = self.primary.logical_address();
        let standby = CecMessage::standby(log_addr, LOG_ADDR_TV);
        self.primary.transmit(&standby)?;
        Ok(())
    }

    /// Broadcast `Active Source` (0x82) to take over display input (CEC-003, CEC-005).
    pub fn assert_active_source(&self) -> Result<(), CecError> {
        let log_addr = self.primary.logical_address();
        let phys_addr = self.primary.physical_address();
        let active = CecMessage::active_source(log_addr, phys_addr);
        self.primary.transmit(&active)?;
        Ok(())
    }

    /// Set the OSD screen name (CEC-002).
    pub fn set_osd_name(&self, name: &str) -> Result<(), CecError> {
        self.primary.set_osd_name(name)
    }

    /// Query current power status from the primary sink (CEC-006).
    pub fn query_power_status(&self) -> Result<PowerStatus, CecError> {
        self.primary.query_power_status()
    }

    /// Generate an aggregated status report.
    pub fn get_status(&self) -> CecStatusSummary {
        let mut adapters = vec![self.primary.get_status()];
        if let Some(sec) = &self.secondary {
            adapters.push(sec.get_status());
        }

        let primary_power = adapters[0].power_status;
        let active_source = adapters[0].active_source;
        let is_mock = adapters[0].is_mock;

        CecStatusSummary {
            enabled: true,
            is_mock,
            primary_power,
            active_source,
            adapters,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_service_power_on_and_standby() {
        let service = CecService::new_mock("TestScreen");
        let status = service.get_status();
        assert!(status.is_mock);
        assert_eq!(status.primary_power, PowerStatus::Standby);

        // Power on
        service.power_on().unwrap();
        let status = service.get_status();
        assert_eq!(status.primary_power, PowerStatus::On);
        assert!(status.active_source);

        // Standby
        service.standby().unwrap();
        let status = service.get_status();
        assert_eq!(status.primary_power, PowerStatus::Standby);
        assert!(!status.active_source);
    }

    #[test]
    fn test_service_assert_active_source() {
        let service = CecService::new_mock("TestScreen");
        assert!(!service.get_status().active_source);

        service.assert_active_source().unwrap();
        assert!(service.get_status().active_source);
    }
}
