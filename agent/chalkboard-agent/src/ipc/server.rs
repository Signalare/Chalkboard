// SPDX-License-Identifier: MIT OR Apache-2.0

//! Unix domain socket IPC server handling commands from the agent (RND-006).

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};

use super::protocol::{RendererCommand, RendererResponse};

/// Trait implemented by components handling renderer IPC commands.
pub trait IpcCommandHandler: Send + Sync + 'static {
    fn handle(&self, cmd: RendererCommand) -> RendererResponse;
}

impl<F> IpcCommandHandler for F
where
    F: Fn(RendererCommand) -> RendererResponse + Send + Sync + 'static,
{
    fn handle(&self, cmd: RendererCommand) -> RendererResponse {
        self(cmd)
    }
}

/// Standalone Unix domain socket IPC server for test harnesses or renderer daemon loops.
#[derive(Debug)]
pub struct IpcServer {
    socket_path: PathBuf,
    running: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl IpcServer {
    pub fn start<H: IpcCommandHandler>(
        socket_path: impl Into<PathBuf>,
        handler: H,
    ) -> std::io::Result<Self> {
        let socket_path = socket_path.into();
        if let Some(parent) = socket_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = fs::remove_file(&socket_path);

        let listener = UnixListener::bind(&socket_path)?;
        listener.set_nonblocking(true)?;

        let running = Arc::new(AtomicBool::new(true));
        let running_clone = running.clone();
        let handler = Arc::new(handler);

        let handle = thread::spawn(move || {
            while running_clone.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let h = handler.clone();
                        thread::spawn(move || {
                            let _ = Self::handle_connection(stream, &*h);
                        });
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(std::time::Duration::from_millis(10));
                    }
                    Err(_) => break,
                }
            }
        });

        Ok(Self {
            socket_path,
            running,
            handle: Some(handle),
        })
    }

    /// Spawns a background thread serving commands on an existing UnixStream (e.g. socketpair).
    pub fn serve_stream<H: IpcCommandHandler>(stream: UnixStream, handler: H) -> JoinHandle<()> {
        let handler = Arc::new(handler);
        thread::spawn(move || {
            let _ = Self::handle_connection(stream, &*handler);
        })
    }

    pub fn handle_connection(
        stream: UnixStream,
        handler: &dyn IpcCommandHandler,
    ) -> std::io::Result<()> {
        let mut reader = BufReader::new(stream.try_clone()?);
        let mut writer = stream;
        let mut line = String::new();

        while reader.read_line(&mut line)? > 0 {
            let trimmed = line.trim();
            if !trimmed.is_empty() {
                let resp = match serde_json::from_str::<RendererCommand>(trimmed) {
                    Ok(cmd) => handler.handle(cmd),
                    Err(e) => RendererResponse::Error {
                        error: format!("invalid command: {e}"),
                    },
                };
                let resp_json = serde_json::to_string(&resp).unwrap_or_else(|_| "{}".to_string());
                writer.write_all(resp_json.as_bytes())?;
                writer.write_all(b"\n")?;
                writer.flush()?;
            }
            line.clear();
        }
        Ok(())
    }

    pub fn stop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
        let _ = fs::remove_file(&self.socket_path);
    }
}

impl Drop for IpcServer {
    fn drop(&mut self) {
        self.stop();
    }
}
