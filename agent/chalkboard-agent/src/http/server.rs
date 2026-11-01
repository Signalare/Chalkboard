// SPDX-License-Identifier: MIT OR Apache-2.0

//! Embedded HTTP server serving `/api/v1` and the static single-page web app (WEB-001, WEB-007).

use std::fs::File;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};

use serde::Deserialize;

use crate::builder::SnapshotBuilder;
use crate::ipc::RendererClient;
use crate::store::{Category, Item, Page, Screen, Store, Template};

/// Toggle payload for sold-out endpoints (WEB-005).
#[derive(Debug, Deserialize)]
struct SoldOutPayload {
    sold_out: bool,
}

/// Announcement payload (RND-008, WEB-004).
#[derive(Debug, Deserialize)]
struct AnnouncementPayload {
    text: String,
    visible: bool,
}

/// Embedded HTTP server for agent REST API and web UI.
#[derive(Debug)]
pub struct HttpServer {
    local_addr: std::net::SocketAddr,
    running: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl HttpServer {
    pub fn start(
        bind_addr: &str,
        store: Store,
        snapshots_root: impl Into<PathBuf>,
        web_dist_dir: impl Into<PathBuf>,
        ipc_client: Option<RendererClient>,
    ) -> std::io::Result<Self> {
        let listener = TcpListener::bind(bind_addr)?;
        let local_addr = listener.local_addr()?;
        listener.set_nonblocking(true)?;

        let running = Arc::new(AtomicBool::new(true));
        let running_clone = running.clone();
        let snapshots_root = snapshots_root.into();
        let web_dist_dir = web_dist_dir.into();
        let ipc_client = Arc::new(ipc_client);

        let handle = thread::spawn(move || {
            while running_clone.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let st = store.clone();
                        let snap_root = snapshots_root.clone();
                        let dist_dir = web_dist_dir.clone();
                        let ipc = ipc_client.clone();
                        thread::spawn(move || {
                            if let Ok(reader) = stream.try_clone() {
                                let _ = Self::handle_connection(
                                    reader,
                                    stream,
                                    st,
                                    &snap_root,
                                    &dist_dir,
                                    ipc.as_ref().as_ref(),
                                );
                            }
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
            local_addr,
            running,
            handle: Some(handle),
        })
    }

    pub fn local_addr(&self) -> std::net::SocketAddr {
        self.local_addr
    }

    pub fn handle_connection<R: Read, W: Write>(
        reader: R,
        mut writer: W,
        store: Store,
        snapshots_root: &Path,
        web_dist_dir: &Path,
        ipc_client: Option<&RendererClient>,
    ) -> std::io::Result<()> {
        let mut reader = BufReader::new(reader);
        let mut request_line = String::new();
        if reader.read_line(&mut request_line)? == 0 {
            return Ok(());
        }

        let parts: Vec<&str> = request_line.split_whitespace().collect();
        if parts.len() < 2 {
            return Ok(());
        }
        let method = parts[0];
        let uri = parts[1];

        // Parse headers
        let mut content_length = 0usize;
        loop {
            let mut line = String::new();
            if reader.read_line(&mut line)? == 0 || line.trim().is_empty() {
                break;
            }
            let lower = line.to_ascii_lowercase();
            if lower.starts_with("content-length:") {
                if let Some(val) = line.split(':').nth(1) {
                    content_length = val.trim().parse().unwrap_or(0);
                }
            }
        }

        // Read body if present
        let mut body = vec![0u8; content_length];
        if content_length > 0 {
            reader.read_exact(&mut body)?;
        }

        // Handle CORS Preflight
        if method == "OPTIONS" {
            let resp = "HTTP/1.1 204 No Content\r\n\
                        Access-Control-Allow-Origin: *\r\n\
                        Access-Control-Allow-Methods: GET, POST, DELETE, OPTIONS\r\n\
                        Access-Control-Allow-Headers: Content-Type\r\n\
                        Content-Length: 0\r\n\r\n";
            writer.write_all(resp.as_bytes())?;
            return Ok(());
        }

        // Dispatch API routes
        if uri.starts_with("/api/v1") {
            Self::route_api(
                &mut writer,
                method,
                uri,
                &body,
                &store,
                snapshots_root,
                ipc_client,
            )
        } else {
            Self::serve_static(&mut writer, method, uri, web_dist_dir)
        }
    }

    fn route_api<W: Write>(
        writer: &mut W,
        method: &str,
        uri: &str,
        body: &[u8],
        store: &Store,
        snapshots_root: &Path,
        ipc_client: Option<&RendererClient>,
    ) -> std::io::Result<()> {
        let path = uri.split('?').next().unwrap_or(uri);

        match (method, path) {
            ("GET", "/api/v1/status") => {
                let current_hash =
                    crate::builder::read_pointer(snapshots_root, "current").unwrap_or_default();
                let prev_hash =
                    crate::builder::read_pointer(snapshots_root, "previous").unwrap_or_default();
                let (ann_text, ann_vis) = store.get_announcement().unwrap_or_default();
                let json = serde_json::json!({
                    "status": "ok",
                    "version": env!("CARGO_PKG_VERSION"),
                    "current_snapshot": current_hash,
                    "previous_snapshot": prev_hash,
                    "announcement": {
                        "text": ann_text,
                        "visible": ann_vis,
                    }
                });
                Self::send_json(writer, 200, &json.to_string())
            }

            ("GET", "/api/v1/categories") => {
                let cats = store.get_categories().unwrap_or_default();
                let json = serde_json::to_string(&cats).unwrap_or_else(|_| "[]".to_string());
                Self::send_json(writer, 200, &json)
            }

            ("POST", "/api/v1/categories") => {
                if let Ok(cat) = serde_json::from_slice::<Category>(body) {
                    store.insert_category(&cat).map_err(std::io::Error::other)?;
                    let json = serde_json::to_string(&cat).unwrap_or_default();
                    Self::send_json(writer, 200, &json)
                } else {
                    Self::send_json(writer, 400, r#"{"error":"invalid category payload"}"#)
                }
            }

            ("GET", "/api/v1/items") => {
                let items = store.get_items().unwrap_or_default();
                let json = serde_json::to_string(&items).unwrap_or_else(|_| "[]".to_string());
                Self::send_json(writer, 200, &json)
            }

            ("POST", "/api/v1/items") => {
                if let Ok(item) = serde_json::from_slice::<Item>(body) {
                    store.insert_item(&item).map_err(std::io::Error::other)?;
                    let json = serde_json::to_string(&item).unwrap_or_default();
                    Self::send_json(writer, 200, &json)
                } else {
                    Self::send_json(writer, 400, r#"{"error":"invalid item payload"}"#)
                }
            }

            ("GET", "/api/v1/templates") => {
                let mut tpls = store.get_templates().unwrap_or_default();
                if tpls.is_empty() {
                    // Seed standard starter templates from specification
                    tpls = vec![
                        Template {
                            id: "list-classic".to_string(),
                            name: "Classic list".to_string(),
                            schema_version: 1,
                            document_json: r#"{"id":"list-classic","name":"Classic list"}"#
                                .to_string(),
                        },
                        Template {
                            id: "two-column".to_string(),
                            name: "Two column".to_string(),
                            schema_version: 1,
                            document_json: r#"{"id":"two-column","name":"Two column"}"#.to_string(),
                        },
                        Template {
                            id: "grid-images".to_string(),
                            name: "Grid with images".to_string(),
                            schema_version: 1,
                            document_json: r#"{"id":"grid-images","name":"Grid with images"}"#
                                .to_string(),
                        },
                        Template {
                            id: "highlight".to_string(),
                            name: "Daily highlight".to_string(),
                            schema_version: 1,
                            document_json: r#"{"id":"highlight","name":"Daily highlight"}"#
                                .to_string(),
                        },
                    ];
                }
                let json = serde_json::to_string(&tpls).unwrap_or_else(|_| "[]".to_string());
                Self::send_json(writer, 200, &json)
            }

            ("GET", "/api/v1/pages") => {
                let pages = store.get_pages().unwrap_or_default();
                let json = serde_json::to_string(&pages).unwrap_or_else(|_| "[]".to_string());
                Self::send_json(writer, 200, &json)
            }

            ("POST", "/api/v1/pages") => {
                if let Ok(p) = serde_json::from_slice::<Page>(body) {
                    store.insert_page(&p).map_err(std::io::Error::other)?;
                    let json = serde_json::to_string(&p).unwrap_or_default();
                    Self::send_json(writer, 200, &json)
                } else {
                    Self::send_json(writer, 400, r#"{"error":"invalid page payload"}"#)
                }
            }

            ("GET", "/api/v1/screens") => {
                let screens = store.get_screens().unwrap_or_default();
                let json = serde_json::to_string(&screens).unwrap_or_else(|_| "[]".to_string());
                Self::send_json(writer, 200, &json)
            }

            ("POST", "/api/v1/screens") => {
                if let Ok(s) = serde_json::from_slice::<Screen>(body) {
                    store.insert_screen(&s).map_err(std::io::Error::other)?;
                    let json = serde_json::to_string(&s).unwrap_or_default();
                    Self::send_json(writer, 200, &json)
                } else {
                    Self::send_json(writer, 400, r#"{"error":"invalid screen payload"}"#)
                }
            }

            ("GET", "/api/v1/announcements") => {
                let (text, visible) = store.get_announcement().unwrap_or_default();
                let json = serde_json::json!({
                    "text": text,
                    "visible": visible,
                });
                Self::send_json(writer, 200, &json.to_string())
            }

            ("POST", "/api/v1/announcements") => {
                if let Ok(p) = serde_json::from_slice::<AnnouncementPayload>(body) {
                    store
                        .set_announcement(&p.text, p.visible)
                        .map_err(std::io::Error::other)?;
                    if let Some(client) = ipc_client {
                        let _ = client.apply_live_state(
                            Vec::new(),
                            Some(crate::ipc::AnnouncementMsg {
                                text: p.text.clone(),
                                visible: p.visible,
                            }),
                        );
                    }
                    Self::send_json(writer, 200, r#"{"status":"ok"}"#)
                } else {
                    Self::send_json(writer, 400, r#"{"error":"invalid announcement payload"}"#)
                }
            }

            ("POST", "/api/v1/publish") => {
                let screens = store.get_screens().unwrap_or_default();
                let screen_id = screens
                    .first()
                    .map(|s| s.id.as_str())
                    .unwrap_or("screen-main");
                let builder = SnapshotBuilder::new(store, snapshots_root);
                let hash = builder
                    .build_and_publish(screen_id, 1)
                    .map_err(std::io::Error::other)?;

                if let Some(client) = ipc_client {
                    let _ = client.load_snapshot(&hash);
                }

                let json = serde_json::json!({
                    "status": "ok",
                    "hash": hash,
                });
                Self::send_json(writer, 200, &json.to_string())
            }

            _ => {
                // Check parameterized paths
                if method == "DELETE" && path.starts_with("/api/v1/categories/") {
                    let id = &path["/api/v1/categories/".len()..];
                    store.delete_category(id).map_err(std::io::Error::other)?;
                    return Self::send_json(writer, 200, r#"{"status":"ok"}"#);
                }
                if method == "DELETE" && path.starts_with("/api/v1/items/") {
                    let id = &path["/api/v1/items/".len()..];
                    store.delete_item(id).map_err(std::io::Error::other)?;
                    return Self::send_json(writer, 200, r#"{"status":"ok"}"#);
                }
                if method == "DELETE" && path.starts_with("/api/v1/pages/") {
                    let id = &path["/api/v1/pages/".len()..];
                    store.delete_page(id).map_err(std::io::Error::other)?;
                    return Self::send_json(writer, 200, r#"{"status":"ok"}"#);
                }
                if method == "POST"
                    && path.starts_with("/api/v1/items/")
                    && path.ends_with("/sold-out")
                {
                    let end_idx = path.len() - "/sold-out".len();
                    let id = &path["/api/v1/items/".len()..end_idx];
                    if let Ok(p) = serde_json::from_slice::<SoldOutPayload>(body) {
                        store
                            .set_item_sold_out(id, p.sold_out)
                            .map_err(std::io::Error::other)?;
                        if let Some(client) = ipc_client {
                            let _ = client.apply_live_state(
                                vec![crate::ipc::SoldOutItem {
                                    item_id: id.to_string(),
                                    sold_out: p.sold_out,
                                }],
                                None,
                            );
                        }
                        return Self::send_json(writer, 200, r#"{"status":"ok"}"#);
                    }
                    return Self::send_json(writer, 400, r#"{"error":"invalid sold_out payload"}"#);
                }

                Self::send_json(writer, 404, r#"{"error":"not found"}"#)
            }
        }
    }

    fn serve_static<W: Write>(
        writer: &mut W,
        _method: &str,
        uri: &str,
        web_dist_dir: &Path,
    ) -> std::io::Result<()> {
        let raw_path = uri.split('?').next().unwrap_or(uri);
        let trimmed = raw_path.trim_start_matches('/');

        let file_path = if trimmed.is_empty() {
            web_dist_dir.join("index.html")
        } else {
            let candidate = web_dist_dir.join(trimmed);
            if candidate.exists() && candidate.is_file() {
                candidate
            } else {
                // SPA fallback (WEB-002)
                web_dist_dir.join("index.html")
            }
        };

        if let Ok(mut f) = File::open(&file_path) {
            let mut contents = Vec::new();
            f.read_to_end(&mut contents)?;
            let mime = Self::mime_for_path(&file_path);
            let header = format!(
                "HTTP/1.1 200 OK\r\n\
                 Content-Type: {mime}\r\n\
                 Content-Length: {}\r\n\
                 Cache-Control: public, max-age=3600\r\n\
                 \r\n",
                contents.len()
            );
            writer.write_all(header.as_bytes())?;
            writer.write_all(&contents)?;
        } else {
            let body = "<h1>Chalkboard Web App</h1><p>Running in single-node mode.</p>";
            let resp = format!(
                "HTTP/1.1 200 OK\r\n\
                 Content-Type: text/html; charset=utf-8\r\n\
                 Content-Length: {}\r\n\
                 \r\n\
                 {body}",
                body.len()
            );
            writer.write_all(resp.as_bytes())?;
        }
        Ok(())
    }

    fn mime_for_path(path: &Path) -> &'static str {
        match path.extension().and_then(|s| s.to_str()).unwrap_or("") {
            "html" => "text/html; charset=utf-8",
            "js" | "mjs" => "application/javascript",
            "css" => "text/css",
            "json" => "application/json",
            "svg" => "image/svg+xml",
            "png" => "image/png",
            "jpg" | "jpeg" => "image/jpeg",
            "woff2" => "font/woff2",
            _ => "application/octet-stream",
        }
    }

    fn send_json<W: Write>(writer: &mut W, status: u16, json: &str) -> std::io::Result<()> {
        let reason = match status {
            200 => "OK",
            204 => "No Content",
            400 => "Bad Request",
            404 => "Not Found",
            _ => "Error",
        };
        let response = format!(
            "HTTP/1.1 {status} {reason}\r\n\
             Content-Type: application/json\r\n\
             Access-Control-Allow-Origin: *\r\n\
             Content-Length: {}\r\n\
             Connection: close\r\n\
             \r\n\
             {json}",
            json.len()
        );
        writer.write_all(response.as_bytes())
    }

    pub fn stop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

impl Drop for HttpServer {
    fn drop(&mut self) {
        self.stop();
    }
}
