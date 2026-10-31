// SPDX-License-Identifier: MIT OR Apache-2.0

//! Snapshot builder compiling store entities into atomic binary snapshots (PUB-003, STO-002, STO-003).

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::store::{Category, Page, Screen, Store};

pub const SNAPSHOT_MAGIC: u64 = 0x31504e534243; // "CBSNP1"
pub const IMAGE_MAGIC: u64 = 0x31474d494243; // "CBIMG1"

pub const MAX_PAGES: usize = 8;
pub const MAX_ITEMS: usize = 32;
pub const MAX_TEXT_LEN: usize = 128;
pub const MAX_HASH_HEX: usize = 65;

pub const SNAPSHOT_PAYLOAD_SIZE: usize = 216176;
pub const SNAPSHOT_HEADER_SIZE: usize = 24;

/// Computes the 64-bit FNV-1a checksum matching chalkboard-render (STO-003).
pub fn fnv1a_64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for &b in bytes {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

/// Atomically writes data to a file via temp file, fsync, rename, and directory fsync (STO-002).
pub fn write_file_atomic(dir: &Path, file_name: &str, data: &[u8]) -> std::io::Result<()> {
    fs::create_dir_all(dir)?;
    let tmp_name = format!(".{file_name}.tmp.{}", std::process::id());
    let tmp_path = dir.join(&tmp_name);
    let final_path = dir.join(file_name);

    {
        let mut f = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&tmp_path)?;
        f.write_all(data)?;
        f.sync_all()?;
    }

    fs::rename(&tmp_path, &final_path)?;

    // Fsync the parent directory to guarantee metadata persistence
    if let Ok(dir_file) = File::open(dir) {
        let _ = dir_file.sync_all();
    }
    Ok(())
}

/// Atomically updates a snapshot pointer file (`current` or `previous`) (STO-003, STO-004).
pub fn set_pointer_atomic(
    snapshots_root: &Path,
    pointer: &str,
    sha256_hex: &str,
) -> std::io::Result<()> {
    let content = format!("{sha256_hex}\n");
    write_file_atomic(snapshots_root, pointer, content.as_bytes())
}

/// Reads the hash from a snapshot pointer file.
pub fn read_pointer(snapshots_root: &Path, pointer: &str) -> Option<String> {
    let path = snapshots_root.join(pointer);
    let mut content = String::new();
    File::open(path).ok()?.read_to_string(&mut content).ok()?;
    let trimmed = content.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Writes a pre-rendered image asset to the snapshot render-cache (STO-003).
pub fn save_cached_image(
    snapshots_root: &Path,
    snapshot_hash: &str,
    image_hash: &str,
    width: u32,
    height: u32,
    pixels: &[u32],
) -> std::io::Result<()> {
    let cache_dir = snapshots_root
        .join("by-hash")
        .join(snapshot_hash)
        .join("render-cache");
    fs::create_dir_all(&cache_dir)?;

    let pixel_bytes: &[u8] = unsafe {
        std::slice::from_raw_parts(pixels.as_ptr() as *const u8, std::mem::size_of_val(pixels))
    };
    let checksum = fnv1a_64(pixel_bytes);

    let mut buf = Vec::with_capacity(24 + pixel_bytes.len());
    buf.extend_from_slice(&IMAGE_MAGIC.to_ne_bytes());
    buf.extend_from_slice(&width.to_ne_bytes());
    buf.extend_from_slice(&height.to_ne_bytes());
    buf.extend_from_slice(&checksum.to_ne_bytes());
    buf.extend_from_slice(pixel_bytes);

    let file_name = format!("{image_hash}.cbimg");
    write_file_atomic(&cache_dir, &file_name, &buf)
}

/// Snapshot compiler producing byte-exact `snapshot.cb` binary bundles.
#[derive(Debug)]
pub struct SnapshotBuilder<'a> {
    store: &'a Store,
    snapshots_root: PathBuf,
}

impl<'a> SnapshotBuilder<'a> {
    pub fn new(store: &'a Store, snapshots_root: impl Into<PathBuf>) -> Self {
        Self {
            store,
            snapshots_root: snapshots_root.into(),
        }
    }

    /// Compiles active screen, pages and items into an atomic binary snapshot bundle (PUB-003).
    pub fn build_and_publish(
        &self,
        screen_id: &str,
        publication_id: u64,
    ) -> std::io::Result<String> {
        let screen = self
            .store
            .get_screen(screen_id)
            .map_err(|e| std::io::Error::other(e.to_string()))?
            .unwrap_or_else(|| Screen {
                id: screen_id.to_string(),
                name: "Default Screen".to_string(),
                node_id: "node-01".to_string(),
                rotation: 0,
                logical_width: 1920,
                logical_height: 1080,
                sold_out_mode: "badge".to_string(),
                sold_out_badge: "SOLD OUT".to_string(),
                sold_out_dim_alpha: 102,
                playlist: Vec::new(),
            });

        // Collect pages in playlist order
        let mut pages = Vec::new();
        for page_id in &screen.playlist {
            if let Ok(Some(page)) = self.store.get_page(page_id) {
                let items = if let Some(cat_id) = &page.category_id {
                    self.store
                        .get_items_for_category(cat_id)
                        .unwrap_or_default()
                } else {
                    Vec::new()
                };
                pages.push((page, items));
                if pages.len() >= MAX_PAGES {
                    break;
                }
            }
        }

        // If no pages were found, provide one default page from the first category
        if pages.is_empty() {
            let categories = self.store.get_categories().unwrap_or_default();
            let first_cat = categories.into_iter().next().unwrap_or_else(|| Category {
                id: "cat-default".to_string(),
                title: "Menu".to_string(),
                description: String::new(),
                sort_order: 0,
                image: None,
            });
            let items = self
                .store
                .get_items_for_category(&first_cat.id)
                .unwrap_or_default();
            pages.push((
                Page {
                    id: "page-default".to_string(),
                    template_id: "tpl-default".to_string(),
                    title: first_cat.title,
                    duration_ms: 5000,
                    transition: "fade".to_string(),
                    transition_ms: 300,
                    repeater_capacity: 4,
                    schedule_start_min: 0,
                    schedule_end_min: 1440,
                    category_id: Some(first_cat.id),
                    sort_order: 0,
                },
                items,
            ));
        }

        // Allocate exact 216,176-byte payload buffer
        let mut payload = vec![0u8; SNAPSHOT_PAYLOAD_SIZE];

        // 1. schema_version = 1 (u32 at offset 0)
        payload[0..4].copy_from_slice(&1u32.to_ne_bytes());
        // 2. publication_id at offset 8 (u64)
        payload[8..16].copy_from_slice(&publication_id.to_ne_bytes());
        // offset 16..81: sha256_hex (will populate after computing sha256)
        // 3. logical_width at offset 84 (u32)
        payload[84..88].copy_from_slice(&screen.logical_width.to_ne_bytes());
        // 4. logical_height at offset 88 (u32)
        payload[88..92].copy_from_slice(&screen.logical_height.to_ne_bytes());
        // 5. rotation at offset 92 (u32)
        payload[92..96].copy_from_slice(&screen.rotation.to_ne_bytes());
        // 6. sold_out_mode at offset 96 (u32: 0=hide, 1=dim, 2=badge)
        let mode_u32: u32 = match screen.sold_out_mode.as_str() {
            "hide" => 0,
            "dim" => 1,
            _ => 2,
        };
        payload[96..100].copy_from_slice(&mode_u32.to_ne_bytes());
        // 7. sold_out_badge at offset 100 (128 bytes)
        Self::copy_str_into(&mut payload[100..228], &screen.sold_out_badge);
        // 8. sold_out_dim_alpha at offset 228 (u8)
        payload[228] = screen.sold_out_dim_alpha;
        // 9. page_count at offset 232 (u32)
        let page_count = pages.len().min(MAX_PAGES) as u32;
        payload[232..236].copy_from_slice(&page_count.to_ne_bytes());

        // 10. Pages starting at offset 236 (each page is 26,992 bytes)
        for (page_idx, (page, items)) in pages.iter().take(MAX_PAGES).enumerate() {
            let page_offset = 236 + (page_idx * 26992);
            let page_slice = &mut payload[page_offset..page_offset + 26992];

            // page.id (128 bytes) at offset 0
            Self::copy_str_into(&mut page_slice[0..128], &page.id);
            // page.title (128 bytes) at offset 128
            Self::copy_str_into(&mut page_slice[128..256], &page.title);
            // page.heading_font at 256 (u32: 0=Inter, 1=Noto Sans, 2=Oswald, 3=Playfair)
            let h_font: u32 = 2; // Oswald for headers
            page_slice[256..260].copy_from_slice(&h_font.to_ne_bytes());
            // page.body_font at 260 (u32: 0=Inter)
            let b_font: u32 = 0;
            page_slice[260..264].copy_from_slice(&b_font.to_ne_bytes());
            // colors
            page_slice[264..268].copy_from_slice(&0x0014110fu32.to_ne_bytes()); // bg
            page_slice[268..272].copy_from_slice(&0x00f2c14eu32.to_ne_bytes()); // heading
            page_slice[272..276].copy_from_slice(&0x00f5f1eau32.to_ne_bytes()); // text
            page_slice[276..280].copy_from_slice(&0x0026211du32.to_ne_bytes()); // accent
            // duration_ms at 280 (u32)
            page_slice[280..284].copy_from_slice(&page.duration_ms.to_ne_bytes());
            // transition at 284 (u32: 0=cut, 1=fade, 2=slide)
            let trans_u32: u32 = match page.transition.as_str() {
                "cut" => 0,
                "slide" => 2,
                _ => 1,
            };
            page_slice[284..288].copy_from_slice(&trans_u32.to_ne_bytes());
            // transition_ms at 288 (u32)
            page_slice[288..292].copy_from_slice(&page.transition_ms.to_ne_bytes());
            // repeater_capacity at 292 (u32)
            page_slice[292..296].copy_from_slice(&page.repeater_capacity.to_ne_bytes());
            // schedule_start_min at 296 (u16)
            page_slice[296..298].copy_from_slice(&page.schedule_start_min.to_ne_bytes());
            // schedule_end_min at 298 (u16)
            page_slice[298..300].copy_from_slice(&page.schedule_end_min.to_ne_bytes());
            // item_count at 300 (u32)
            let item_count = items.len().min(MAX_ITEMS) as u32;
            page_slice[300..304].copy_from_slice(&item_count.to_ne_bytes());

            // Items starting at offset 304 (each item is 834 bytes)
            for (item_idx, item) in items.iter().take(MAX_ITEMS).enumerate() {
                let item_offset = 304 + (item_idx * 834);
                let item_slice = &mut page_slice[item_offset..item_offset + 834];

                Self::copy_str_into(&mut item_slice[0..128], &item.id);
                Self::copy_str_into(&mut item_slice[128..256], &item.name);
                Self::copy_str_into(&mut item_slice[256..384], &item.description);
                Self::copy_str_into(&mut item_slice[384..512], &item.price_primary);
                Self::copy_str_into(&mut item_slice[512..640], &item.price_secondary);
                Self::copy_str_into(&mut item_slice[640..768], &item.badge);
                Self::copy_str_into(&mut item_slice[768..833], &item.image_hash);
                item_slice[833] = if item.sold_out { 1 } else { 0 };
            }
        }

        // Compute content hash (SHA-256) of the compiled payload
        let mut hasher = Sha256::new();
        hasher.update(&payload[0..16]); // schema_version + pub_id
        hasher.update(&payload[84..]); // everything after sha256_hex field
        let digest = hasher.finalize();
        let sha256_hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();

        // Store sha256_hex at offset 16..81
        Self::copy_str_into(&mut payload[16..81], &sha256_hex);

        // Build header
        let payload_checksum = fnv1a_64(&payload);
        let mut bundle = Vec::with_capacity(SNAPSHOT_HEADER_SIZE + payload.len());
        bundle.extend_from_slice(&SNAPSHOT_MAGIC.to_ne_bytes());
        bundle.extend_from_slice(&payload_checksum.to_ne_bytes());
        bundle.extend_from_slice(&(SNAPSHOT_PAYLOAD_SIZE as u64).to_ne_bytes());
        bundle.extend_from_slice(&payload);

        // Write atomic snapshot file to /by-hash/<sha256>/snapshot.cb (STO-002, STO-003)
        let snap_dir = self.snapshots_root.join("by-hash").join(&sha256_hex);
        write_file_atomic(&snap_dir, "snapshot.cb", &bundle)?;

        // Update pointers: previous <- existing current, current <- sha256_hex (STO-004)
        if let Some(existing_current) = read_pointer(&self.snapshots_root, "current") {
            if existing_current != sha256_hex {
                let _ = set_pointer_atomic(&self.snapshots_root, "previous", &existing_current);
            }
        }
        set_pointer_atomic(&self.snapshots_root, "current", &sha256_hex)?;

        Ok(sha256_hex)
    }

    fn copy_str_into(dest: &mut [u8], src: &str) {
        let bytes = src.as_bytes();
        let max_len = dest.len() - 1;
        let copy_len = bytes.len().min(max_len);
        dest[..copy_len].copy_from_slice(&bytes[..copy_len]);
        dest[copy_len] = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fnv1a_matches_reference() {
        let empty_hash = fnv1a_64(&[]);
        assert_eq!(empty_hash, 0xcbf29ce484222325);

        let data = b"Chalkboard Menu Board";
        let hash = fnv1a_64(data);
        assert_ne!(hash, 0);
    }

    #[test]
    fn pointers_and_atomic_file_ops() {
        let tmp = std::env::temp_dir().join(format!("cb-snap-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();

        assert_eq!(read_pointer(&tmp, "nonexistent"), None);

        set_pointer_atomic(&tmp, "current", "deadbeef").unwrap();
        assert_eq!(read_pointer(&tmp, "current"), Some("deadbeef".to_string()));

        let _ = fs::remove_dir_all(&tmp);
    }
}
