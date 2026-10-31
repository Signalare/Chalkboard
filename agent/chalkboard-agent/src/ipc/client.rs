// SPDX-License-Identifier: MIT OR Apache-2.0

//! Unix domain socket client for communicating with the renderer (RND-006).

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

use super::protocol::{AnnouncementMsg, RendererCommand, RendererResponse, SoldOutItem};

/// Target for the renderer IPC client.
#[derive(Debug)]
enum Target {
    Path(PathBuf),
    Stream(UnixStream),
}

/// Client for issuing RND-006 commands to `chalkboard-render`.
#[derive(Debug)]
pub struct RendererClient {
    target: Target,
    timeout: Duration,
}

impl RendererClient {
    pub fn new(socket_path: impl Into<PathBuf>) -> Self {
        Self {
            target: Target::Path(socket_path.into()),
            timeout: Duration::from_secs(3),
        }
    }

    /// Creates a client from an established UnixStream (e.g. socketpair or inherited socket).
    pub fn from_stream(stream: UnixStream) -> Self {
        Self {
            target: Target::Stream(stream),
            timeout: Duration::from_secs(3),
        }
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn socket_path(&self) -> Option<&Path> {
        match &self.target {
            Target::Path(p) => Some(p.as_path()),
            Target::Stream(_) => None,
        }
    }

    /// Sends an arbitrary RendererCommand and receives the parsed response.
    pub fn send(&self, cmd: &RendererCommand) -> std::io::Result<RendererResponse> {
        match &self.target {
            Target::Path(path) => {
                let stream = UnixStream::connect(path)?;
                Self::send_over_stream(&stream, cmd, self.timeout)
            }
            Target::Stream(stream) => Self::send_over_stream(stream, cmd, self.timeout),
        }
    }

    fn send_over_stream(
        stream: &UnixStream,
        cmd: &RendererCommand,
        timeout: Duration,
    ) -> std::io::Result<RendererResponse> {
        let _ = stream.set_read_timeout(Some(timeout));
        let _ = stream.set_write_timeout(Some(timeout));

        let mut writer = stream.try_clone()?;
        let json_line = serde_json::to_string(cmd)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;
        writer.write_all(json_line.as_bytes())?;
        writer.write_all(b"\n")?;
        writer.flush()?;

        let mut reader = BufReader::new(stream.try_clone()?);
        let mut line = String::new();
        reader.read_line(&mut line)?;
        if line.trim().is_empty() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "empty response from renderer",
            ));
        }

        let resp: RendererResponse = serde_json::from_str(line.trim())
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;
        Ok(resp)
    }

    pub fn load_snapshot(&self, hash: &str) -> std::io::Result<RendererResponse> {
        self.send(&RendererCommand::LoadSnapshot {
            hash: hash.to_string(),
        })
    }

    pub fn apply_live_state(
        &self,
        sold_out: Vec<SoldOutItem>,
        announcement: Option<AnnouncementMsg>,
    ) -> std::io::Result<RendererResponse> {
        self.send(&RendererCommand::ApplyLiveState {
            sold_out,
            announcement,
        })
    }

    pub fn identify(
        &self,
        enabled: bool,
        node_name: &str,
        screen_name: &str,
        ip: &str,
    ) -> std::io::Result<RendererResponse> {
        self.send(&RendererCommand::Identify {
            enabled,
            node_name: node_name.to_string(),
            screen_name: screen_name.to_string(),
            ip: ip.to_string(),
        })
    }

    pub fn screenshot(&self) -> std::io::Result<RendererResponse> {
        self.send(&RendererCommand::Screenshot)
    }

    pub fn status(&self) -> std::io::Result<RendererResponse> {
        self.send(&RendererCommand::Status)
    }
}
