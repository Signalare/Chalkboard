// SPDX-License-Identifier: MIT OR Apache-2.0

//! In-memory mock HDMI-CEC device adapter and simulation test harness (#25, CEC-001..CEC-006).

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use super::adapter::{CecAdapter, CecError};
use super::protocol::{
    CecMessage, LOG_ADDR_BROADCAST, LOG_ADDR_PLAYBACK_1, LOG_ADDR_TV, OP_ACTIVE_SOURCE,
    OP_GIVE_DEVICE_POWER_STATUS, OP_IMAGE_VIEW_ON, OP_REPORT_DEVICE_POWER_STATUS, OP_SET_OSD_NAME,
    OP_STANDBY, PowerStatus,
};

/// Simulated state of the connected sink / TV.
#[derive(Debug, Clone)]
pub struct SimulatedSinkState {
    pub power: PowerStatus,
    pub active_source: Option<[u8; 2]>,
    pub registered_osd_name: Option<String>,
}

/// In-memory mock CEC adapter for offline development and headless testing.
#[derive(Debug, Clone)]
pub struct MockCecAdapter {
    device_name: String,
    logical_address: u8,
    physical_address: [u8; 2],
    osd_name: Arc<Mutex<String>>,
    sink_state: Arc<Mutex<SimulatedSinkState>>,
    transmitted_history: Arc<Mutex<Vec<CecMessage>>>,
    incoming_queue: Arc<Mutex<VecDeque<CecMessage>>>,
    simulate_nack: Arc<AtomicBool>,
    simulate_timeout: Arc<AtomicBool>,
}

impl Default for MockCecAdapter {
    fn default() -> Self {
        Self::new("mock-cec0", LOG_ADDR_PLAYBACK_1, [0x10, 0x00], "Chalkboard")
    }
}

impl MockCecAdapter {
    /// Create a new mock CEC adapter with custom parameters.
    pub fn new(
        device_name: &str,
        logical_address: u8,
        physical_address: [u8; 2],
        initial_osd_name: &str,
    ) -> Self {
        Self {
            device_name: device_name.to_string(),
            logical_address,
            physical_address,
            osd_name: Arc::new(Mutex::new(initial_osd_name.to_string())),
            sink_state: Arc::new(Mutex::new(SimulatedSinkState {
                power: PowerStatus::Standby,
                active_source: None,
                registered_osd_name: None,
            })),
            transmitted_history: Arc::new(Mutex::new(Vec::new())),
            incoming_queue: Arc::new(Mutex::new(VecDeque::new())),
            simulate_nack: Arc::new(AtomicBool::new(false)),
            simulate_timeout: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Retrieve a snapshot of all messages transmitted by this adapter.
    pub fn transmitted_messages(&self) -> Vec<CecMessage> {
        self.transmitted_history.lock().unwrap().clone()
    }

    /// Clear transmitted history.
    pub fn clear_history(&self) {
        self.transmitted_history.lock().unwrap().clear();
    }

    /// Retrieve the current simulated TV state.
    pub fn sink_state(&self) -> SimulatedSinkState {
        self.sink_state.lock().unwrap().clone()
    }

    /// Manually configure the simulated TV power state.
    pub fn set_sink_power(&self, power: PowerStatus) {
        self.sink_state.lock().unwrap().power = power;
    }

    /// Enable or disable transmission NACK simulation.
    pub fn set_simulate_nack(&self, enable: bool) {
        self.simulate_nack.store(enable, Ordering::SeqCst);
    }

    /// Enable or disable transmission timeout simulation.
    pub fn set_simulate_timeout(&self, enable: bool) {
        self.simulate_timeout.store(enable, Ordering::SeqCst);
    }

    /// Inject an incoming CEC message into the adapter's receive queue (e.g. TV event).
    pub fn inject_incoming(&self, msg: CecMessage) {
        self.incoming_queue.lock().unwrap().push_back(msg);
    }

    /// Simulate the TV waking up spontaneously.
    pub fn simulate_tv_wake(&self) {
        self.set_sink_power(PowerStatus::On);
        // Sink broadcasts routing change or reports power status
        let event = CecMessage::report_device_power_status(
            LOG_ADDR_TV,
            LOG_ADDR_BROADCAST,
            PowerStatus::On,
        );
        self.inject_incoming(event);
    }

    /// Simulate the TV going into standby.
    pub fn simulate_tv_standby(&self) {
        self.set_sink_power(PowerStatus::Standby);
        let event = CecMessage::standby(LOG_ADDR_TV, LOG_ADDR_BROADCAST);
        self.inject_incoming(event);
    }
}

impl CecAdapter for MockCecAdapter {
    fn device_name(&self) -> &str {
        &self.device_name
    }

    fn is_mock(&self) -> bool {
        true
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

        // Broadcast or send Set OSD Name to TV
        let msg = CecMessage::set_osd_name(self.logical_address, LOG_ADDR_TV, &truncated);
        self.transmit(&msg)
    }

    fn transmit(&self, msg: &CecMessage) -> Result<(), CecError> {
        if self.simulate_timeout.load(Ordering::SeqCst) {
            return Err(CecError::TransmitTimeout);
        }
        if self.simulate_nack.load(Ordering::SeqCst) {
            return Err(CecError::TransmitNack);
        }

        self.transmitted_history.lock().unwrap().push(msg.clone());

        // Process simulated TV state reactions
        if let Some(op) = msg.opcode {
            let mut sink = self.sink_state.lock().unwrap();
            match op {
                OP_IMAGE_VIEW_ON => {
                    sink.power = PowerStatus::On;
                }
                OP_STANDBY => {
                    sink.power = PowerStatus::Standby;
                    if sink.active_source == Some(self.physical_address) {
                        sink.active_source = None;
                    }
                }
                OP_ACTIVE_SOURCE => {
                    if msg.parameters.len() >= 2 {
                        sink.active_source = Some([msg.parameters[0], msg.parameters[1]]);
                    }
                }
                OP_SET_OSD_NAME => {
                    if let Ok(name) = std::str::from_utf8(&msg.parameters) {
                        sink.registered_osd_name = Some(name.to_string());
                    }
                }
                OP_GIVE_DEVICE_POWER_STATUS => {
                    let current_power = sink.power;
                    // Automatically enqueue a response to the initiator
                    let reply = CecMessage::report_device_power_status(
                        msg.destination,
                        msg.initiator,
                        current_power,
                    );
                    self.incoming_queue.lock().unwrap().push_back(reply);
                }
                _ => {}
            }
        }

        Ok(())
    }

    fn receive(&self) -> Result<Option<CecMessage>, CecError> {
        Ok(self.incoming_queue.lock().unwrap().pop_front())
    }

    fn query_power_status(&self) -> Result<PowerStatus, CecError> {
        // Transmit Give Device Power Status query to TV
        let query = CecMessage::give_device_power_status(self.logical_address, LOG_ADDR_TV);
        self.transmit(&query)?;

        // In mock mode, the response is processed immediately into the queue
        while let Some(msg) = self.receive()? {
            if msg.opcode == Some(OP_REPORT_DEVICE_POWER_STATUS) && !msg.parameters.is_empty() {
                let status = PowerStatus::from_u8(msg.parameters[0]);
                self.sink_state.lock().unwrap().power = status;
                return Ok(status);
            }
        }

        Ok(self.sink_state.lock().unwrap().power)
    }

    fn is_active_source(&self) -> bool {
        self.sink_state.lock().unwrap().active_source == Some(self.physical_address)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_image_view_on_powers_on_tv() {
        let mock = MockCecAdapter::default();
        assert_eq!(mock.sink_state().power, PowerStatus::Standby);

        let msg = CecMessage::image_view_on(LOG_ADDR_PLAYBACK_1, LOG_ADDR_TV);
        mock.transmit(&msg).unwrap();

        assert_eq!(mock.sink_state().power, PowerStatus::On);
        assert_eq!(mock.transmitted_messages().len(), 1);
        assert_eq!(mock.transmitted_messages()[0], msg);
    }

    #[test]
    fn test_mock_standby_powers_down_tv() {
        let mock = MockCecAdapter::default();
        mock.set_sink_power(PowerStatus::On);

        let msg = CecMessage::standby(LOG_ADDR_PLAYBACK_1, LOG_ADDR_TV);
        mock.transmit(&msg).unwrap();

        assert_eq!(mock.sink_state().power, PowerStatus::Standby);
    }

    #[test]
    fn test_mock_active_source_tracking() {
        let mock = MockCecAdapter::default();
        assert!(!mock.is_active_source());

        let msg = CecMessage::active_source(LOG_ADDR_PLAYBACK_1, [0x10, 0x00]);
        mock.transmit(&msg).unwrap();

        assert!(mock.is_active_source());
        assert_eq!(mock.sink_state().active_source, Some([0x10, 0x00]));
    }

    #[test]
    fn test_mock_osd_name_setting() {
        let mock = MockCecAdapter::default();
        mock.set_osd_name("CounterScreen").unwrap();

        assert_eq!(mock.osd_name(), "CounterScreen");
        assert_eq!(
            mock.sink_state().registered_osd_name,
            Some("CounterScreen".to_string())
        );
    }

    #[test]
    fn test_mock_query_power_status() {
        let mock = MockCecAdapter::default();
        mock.set_sink_power(PowerStatus::On);

        let status = mock.query_power_status().unwrap();
        assert_eq!(status, PowerStatus::On);

        mock.set_sink_power(PowerStatus::Standby);
        let status = mock.query_power_status().unwrap();
        assert_eq!(status, PowerStatus::Standby);
    }

    #[test]
    fn test_mock_error_injection() {
        let mock = MockCecAdapter::default();
        let msg = CecMessage::image_view_on(LOG_ADDR_PLAYBACK_1, LOG_ADDR_TV);

        // NACK simulation
        mock.set_simulate_nack(true);
        assert_eq!(mock.transmit(&msg), Err(CecError::TransmitNack));
        mock.set_simulate_nack(false);

        // Timeout simulation
        mock.set_simulate_timeout(true);
        assert_eq!(mock.transmit(&msg), Err(CecError::TransmitTimeout));
        mock.set_simulate_timeout(false);

        // Succeeded when errors cleared
        assert!(mock.transmit(&msg).is_ok());
    }

    #[test]
    fn test_mock_incoming_injection() {
        let mock = MockCecAdapter::default();
        assert!(mock.receive().unwrap().is_none());

        let incoming = CecMessage::standby(LOG_ADDR_TV, LOG_ADDR_BROADCAST);
        mock.inject_incoming(incoming.clone());

        assert_eq!(mock.receive().unwrap(), Some(incoming));
        assert!(mock.receive().unwrap().is_none());
    }
}
