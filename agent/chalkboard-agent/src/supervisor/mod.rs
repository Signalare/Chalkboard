// SPDX-License-Identifier: MIT OR Apache-2.0

//! Process supervisor and watchdog setup (SPEC §5, OS-006).

pub mod process;

pub use process::ProcessSupervisor;
