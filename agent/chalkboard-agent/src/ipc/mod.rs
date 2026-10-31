// SPDX-License-Identifier: MIT OR Apache-2.0

//! Renderer IPC communication layer (SPEC §7, RND-006).

pub mod client;
pub mod protocol;
pub mod server;

pub use client::RendererClient;
pub use protocol::*;
pub use server::{IpcCommandHandler, IpcServer};
