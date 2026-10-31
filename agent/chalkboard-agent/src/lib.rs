// SPDX-License-Identifier: MIT OR Apache-2.0

//! `chalkboard-agent`: discovery, replication, HTTP API and web app, HDMI-CEC,
//! time, provisioning, SQLite storage, renderer IPC and snapshot builder (SPEC §3).

pub use chalkboard_board as board;
pub use chalkboard_template as template;

pub mod builder;
pub mod ipc;
pub mod store;
pub mod supervisor;

/// Name and version printed at startup.
pub fn banner() -> String {
    format!("chalkboard-agent {}", env!("CARGO_PKG_VERSION"))
}
