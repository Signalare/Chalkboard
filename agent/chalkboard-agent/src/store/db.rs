// SPDX-License-Identifier: MIT OR Apache-2.0

//! SQLite store in WAL mode with group commit and corruption recovery (STO-001, STO-005).

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OptionalExtension, Result, params};

use super::models::{Category, Item, Page, PriceVariant, Screen, Template};

/// SQLite database wrapper enforcing WAL mode, group commit and corruption recovery.
#[derive(Debug, Clone)]
pub struct Store {
    db_path: PathBuf,
    backup_path: Option<PathBuf>,
    conn: Arc<Mutex<Connection>>,
    write_queue: Arc<Mutex<Vec<String>>>,
    last_flush: Arc<Mutex<Instant>>,
    last_checkpoint: Arc<Mutex<Instant>>,
}

impl Store {
    /// Opens or initializes a SQLite store with WAL mode and `synchronous=NORMAL` (STO-001).
    /// If corruption is detected, discards and recovers from backup or fresh schema (STO-005).
    pub fn open<P: AsRef<Path>>(path: P, backup_path: Option<PathBuf>) -> Result<Self> {
        let db_path = path.as_ref().to_path_buf();
        if let Some(parent) = db_path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        let conn = match Self::open_and_verify(&db_path) {
            Ok(c) => c,
            Err(_) => {
                // STO-005: On database corruption, discard local database and recover.
                Self::quarantine_and_recover(&db_path, backup_path.as_deref())?
            }
        };

        let store = Self {
            db_path,
            backup_path,
            conn: Arc::new(Mutex::new(conn)),
            write_queue: Arc::new(Mutex::new(Vec::new())),
            last_flush: Arc::new(Mutex::new(Instant::now())),
            last_checkpoint: Arc::new(Mutex::new(Instant::now())),
        };

        store.init_schema()?;
        Ok(store)
    }

    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    pub fn backup_path(&self) -> Option<&Path> {
        self.backup_path.as_deref()
    }

    /// Returns the active SQLite journal mode on this store connection.
    pub fn pragma_journal_mode(&self) -> Result<String> {
        let conn = self.conn.lock().unwrap();
        conn.query_row("PRAGMA journal_mode;", [], |r| r.get(0))
    }

    /// Returns the active SQLite synchronous mode on this store connection (1 = NORMAL).
    pub fn pragma_synchronous(&self) -> Result<i32> {
        let conn = self.conn.lock().unwrap();
        conn.query_row("PRAGMA synchronous;", [], |r| r.get(0))
    }

    /// Opens an in-memory store for isolated unit tests.
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        Self::apply_pragmas(&conn)?;
        let store = Self {
            db_path: PathBuf::from(":memory:"),
            backup_path: None,
            conn: Arc::new(Mutex::new(conn)),
            write_queue: Arc::new(Mutex::new(Vec::new())),
            last_flush: Arc::new(Mutex::new(Instant::now())),
            last_checkpoint: Arc::new(Mutex::new(Instant::now())),
        };
        store.init_schema()?;
        Ok(store)
    }

    fn open_and_verify(path: &Path) -> Result<Connection> {
        let conn = Connection::open(path)?;
        Self::apply_pragmas(&conn)?;

        // Quick integrity check on open (STO-005)
        let check_res: String = {
            let mut stmt = conn.prepare("PRAGMA quick_check(1);")?;
            stmt.query_row([], |r| r.get(0))?
        };
        if check_res != "ok" {
            return Err(rusqlite::Error::SqliteFailure(
                rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_CORRUPT),
                Some(format!("quick_check failed: {check_res}")),
            ));
        }

        Ok(conn)
    }

    /// Applies mandatory pragmas: WAL mode, synchronous=NORMAL, autocheckpoint=1000 (STO-001).
    fn apply_pragmas(conn: &Connection) -> Result<()> {
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA wal_autocheckpoint = 1000;
             PRAGMA foreign_keys = ON;",
        )?;
        Ok(())
    }

    /// Discards a corrupted SQLite database, archiving it and recovering from backup or fresh DB.
    fn quarantine_and_recover(path: &Path, backup_path: Option<&Path>) -> Result<Connection> {
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let corrupt_path = path.with_extension(format!("corrupt.{ts}"));

        let _ = fs::rename(path, &corrupt_path);
        let _ = fs::remove_file(path.with_extension("db-wal"));
        let _ = fs::remove_file(path.with_extension("db-shm"));

        if let Some(bak) = backup_path {
            if bak.exists() {
                if let Ok(c) = Self::open_and_verify(bak) {
                    let _ = fs::copy(bak, path);
                    drop(c);
                    if let Ok(c2) = Self::open_and_verify(path) {
                        return Ok(c2);
                    }
                }
            }
        }

        // Recreate clean database
        let conn = Connection::open(path)?;
        Self::apply_pragmas(&conn)?;
        Ok(conn)
    }

    /// Creates tables for Milestone 0.1 entities if they do not exist.
    pub fn init_schema(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS categories (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                description TEXT NOT NULL DEFAULT '',
                sort_order INTEGER NOT NULL DEFAULT 0,
                image TEXT
            );

            CREATE TABLE IF NOT EXISTS items (
                id TEXT PRIMARY KEY,
                category_id TEXT NOT NULL REFERENCES categories(id) ON DELETE CASCADE,
                name TEXT NOT NULL,
                description TEXT NOT NULL DEFAULT '',
                price_primary TEXT NOT NULL DEFAULT '',
                price_secondary TEXT NOT NULL DEFAULT '',
                badge TEXT NOT NULL DEFAULT '',
                image_hash TEXT NOT NULL DEFAULT '',
                sold_out INTEGER NOT NULL DEFAULT 0,
                sort_order INTEGER NOT NULL DEFAULT 0
            );

            CREATE TABLE IF NOT EXISTS price_variants (
                id TEXT PRIMARY KEY,
                item_id TEXT NOT NULL REFERENCES items(id) ON DELETE CASCADE,
                label TEXT NOT NULL,
                amount INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS templates (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                schema_version INTEGER NOT NULL DEFAULT 1,
                document_json TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS pages (
                id TEXT PRIMARY KEY,
                template_id TEXT NOT NULL REFERENCES templates(id) ON DELETE CASCADE,
                title TEXT NOT NULL DEFAULT '',
                duration_ms INTEGER NOT NULL DEFAULT 5000,
                transition TEXT NOT NULL DEFAULT 'fade',
                transition_ms INTEGER NOT NULL DEFAULT 300,
                repeater_capacity INTEGER NOT NULL DEFAULT 4,
                schedule_start_min INTEGER NOT NULL DEFAULT 0,
                schedule_end_min INTEGER NOT NULL DEFAULT 1440,
                category_id TEXT,
                sort_order INTEGER NOT NULL DEFAULT 0
            );

            CREATE TABLE IF NOT EXISTS screens (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                node_id TEXT NOT NULL,
                rotation INTEGER NOT NULL DEFAULT 0,
                logical_width INTEGER NOT NULL DEFAULT 1920,
                logical_height INTEGER NOT NULL DEFAULT 1080,
                sold_out_mode TEXT NOT NULL DEFAULT 'badge',
                sold_out_badge TEXT NOT NULL DEFAULT 'SOLD OUT',
                sold_out_dim_alpha INTEGER NOT NULL DEFAULT 102,
                playlist_json TEXT NOT NULL DEFAULT '[]'
            );

            CREATE TABLE IF NOT EXISTS live_state (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );",
        )?;
        Ok(())
    }

    /// Queues a SQL statement for group commit (STO-001: batches committed at most 1 s apart).
    pub fn queue_mutation(&self, sql: impl Into<String>) -> Result<()> {
        let mut queue = self.write_queue.lock().unwrap();
        queue.push(sql.into());
        let elapsed = self.last_flush.lock().unwrap().elapsed();
        if elapsed >= Duration::from_millis(900) || queue.len() >= 50 {
            drop(queue);
            self.flush_group_commit()?;
        }
        Ok(())
    }

    /// Flushes all queued mutations within a single atomic SQLite transaction (STO-001).
    pub fn flush_group_commit(&self) -> Result<usize> {
        let mut queue = self.write_queue.lock().unwrap();
        if queue.is_empty() {
            return Ok(0);
        }
        let batch: Vec<String> = queue.drain(..).collect();
        drop(queue);

        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let count = batch.len();
        for sql in batch {
            tx.execute_batch(&sql)?;
        }
        tx.commit()?;

        let mut last_flush = self.last_flush.lock().unwrap();
        *last_flush = Instant::now();
        Ok(count)
    }

    /// Triggers passive WAL checkpointing (STO-001: every 1,000 pages or 5 min).
    pub fn checkpoint_wal(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch("PRAGMA wal_checkpoint(PASSIVE);")?;
        let mut last_checkpoint = self.last_checkpoint.lock().unwrap();
        *last_checkpoint = Instant::now();
        Ok(())
    }

    /// Checks if a 5-minute checkpoint interval has elapsed and executes it if needed.
    pub fn check_checkpoint_interval(&self) -> Result<bool> {
        let elapsed = self.last_checkpoint.lock().unwrap().elapsed();
        if elapsed >= Duration::from_secs(300) {
            self.checkpoint_wal()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Creates an atomic online backup of the SQLite database (IO-005, STO-005).
    pub fn create_backup(&self, destination: &Path) -> Result<()> {
        if let Some(parent) = destination.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let conn = self.conn.lock().unwrap();
        let dest_str = destination
            .to_str()
            .ok_or_else(|| rusqlite::Error::InvalidPath(destination.to_path_buf()))?;
        conn.execute(
            &format!("VACUUM INTO '{}';", dest_str.replace('\'', "''")),
            [],
        )?;
        Ok(())
    }

    // --- Entity CRUD operations ---

    pub fn insert_category(&self, cat: &Category) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO categories (id, title, description, sort_order, image)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(id) DO UPDATE SET
                title = excluded.title,
                description = excluded.description,
                sort_order = excluded.sort_order,
                image = excluded.image;",
            params![
                cat.id,
                cat.title,
                cat.description,
                cat.sort_order,
                cat.image
            ],
        )?;
        Ok(())
    }

    pub fn get_categories(&self) -> Result<Vec<Category>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, title, description, sort_order, image FROM categories ORDER BY sort_order ASC, id ASC;",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(Category {
                id: r.get(0)?,
                title: r.get(1)?,
                description: r.get(2)?,
                sort_order: r.get(3)?,
                image: r.get(4)?,
            })
        })?;
        let mut list = Vec::new();
        for cat in rows {
            list.push(cat?);
        }
        Ok(list)
    }

    pub fn insert_item(&self, item: &Item) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO items (id, category_id, name, description, price_primary, price_secondary, badge, image_hash, sold_out, sort_order)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT(id) DO UPDATE SET
                category_id = excluded.category_id,
                name = excluded.name,
                description = excluded.description,
                price_primary = excluded.price_primary,
                price_secondary = excluded.price_secondary,
                badge = excluded.badge,
                image_hash = excluded.image_hash,
                sold_out = excluded.sold_out,
                sort_order = excluded.sort_order;",
            params![
                item.id,
                item.category_id,
                item.name,
                item.description,
                item.price_primary,
                item.price_secondary,
                item.badge,
                item.image_hash,
                if item.sold_out { 1 } else { 0 },
                item.sort_order
            ],
        )?;
        Ok(())
    }

    pub fn get_items_for_category(&self, cat_id: &str) -> Result<Vec<Item>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, category_id, name, description, price_primary, price_secondary, badge, image_hash, sold_out, sort_order
             FROM items WHERE category_id = ?1 ORDER BY sort_order ASC, id ASC;",
        )?;
        let rows = stmt.query_map([cat_id], |r| {
            let sold_int: i32 = r.get(8)?;
            Ok(Item {
                id: r.get(0)?,
                category_id: r.get(1)?,
                name: r.get(2)?,
                description: r.get(3)?,
                price_primary: r.get(4)?,
                price_secondary: r.get(5)?,
                badge: r.get(6)?,
                image_hash: r.get(7)?,
                sold_out: sold_int != 0,
                sort_order: r.get(9)?,
            })
        })?;
        let mut list = Vec::new();
        for item in rows {
            list.push(item?);
        }
        Ok(list)
    }

    pub fn set_item_sold_out(&self, item_id: &str, sold_out: bool) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE items SET sold_out = ?1 WHERE id = ?2;",
            params![if sold_out { 1 } else { 0 }, item_id],
        )?;
        Ok(())
    }

    pub fn insert_price_variant(&self, v: &PriceVariant) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO price_variants (id, item_id, label, amount)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(id) DO UPDATE SET
                item_id = excluded.item_id,
                label = excluded.label,
                amount = excluded.amount;",
            params![v.id, v.item_id, v.label, v.amount],
        )?;
        Ok(())
    }

    pub fn insert_template(&self, t: &Template) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO templates (id, name, schema_version, document_json)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                schema_version = excluded.schema_version,
                document_json = excluded.document_json;",
            params![t.id, t.name, t.schema_version, t.document_json],
        )?;
        Ok(())
    }

    pub fn get_template(&self, id: &str) -> Result<Option<Template>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, schema_version, document_json FROM templates WHERE id = ?1;",
        )?;
        stmt.query_row([id], |r| {
            Ok(Template {
                id: r.get(0)?,
                name: r.get(1)?,
                schema_version: r.get(2)?,
                document_json: r.get(3)?,
            })
        })
        .optional()
    }

    pub fn insert_page(&self, p: &Page) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO pages (id, template_id, title, duration_ms, transition, transition_ms, repeater_capacity, schedule_start_min, schedule_end_min, category_id, sort_order)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
             ON CONFLICT(id) DO UPDATE SET
                template_id = excluded.template_id,
                title = excluded.title,
                duration_ms = excluded.duration_ms,
                transition = excluded.transition,
                transition_ms = excluded.transition_ms,
                repeater_capacity = excluded.repeater_capacity,
                schedule_start_min = excluded.schedule_start_min,
                schedule_end_min = excluded.schedule_end_min,
                category_id = excluded.category_id,
                sort_order = excluded.sort_order;",
            params![
                p.id,
                p.template_id,
                p.title,
                p.duration_ms,
                p.transition,
                p.transition_ms,
                p.repeater_capacity,
                p.schedule_start_min,
                p.schedule_end_min,
                p.category_id,
                p.sort_order
            ],
        )?;
        Ok(())
    }

    pub fn get_page(&self, id: &str) -> Result<Option<Page>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, template_id, title, duration_ms, transition, transition_ms, repeater_capacity, schedule_start_min, schedule_end_min, category_id, sort_order
             FROM pages WHERE id = ?1;",
        )?;
        stmt.query_row([id], |r| {
            Ok(Page {
                id: r.get(0)?,
                template_id: r.get(1)?,
                title: r.get(2)?,
                duration_ms: r.get(3)?,
                transition: r.get(4)?,
                transition_ms: r.get(5)?,
                repeater_capacity: r.get(6)?,
                schedule_start_min: r.get(7)?,
                schedule_end_min: r.get(8)?,
                category_id: r.get(9)?,
                sort_order: r.get(10)?,
            })
        })
        .optional()
    }

    pub fn insert_screen(&self, s: &Screen) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let playlist_json = serde_json::to_string(&s.playlist).unwrap_or_else(|_| "[]".to_string());
        conn.execute(
            "INSERT INTO screens (id, name, node_id, rotation, logical_width, logical_height, sold_out_mode, sold_out_badge, sold_out_dim_alpha, playlist_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                node_id = excluded.node_id,
                rotation = excluded.rotation,
                logical_width = excluded.logical_width,
                logical_height = excluded.logical_height,
                sold_out_mode = excluded.sold_out_mode,
                sold_out_badge = excluded.sold_out_badge,
                sold_out_dim_alpha = excluded.sold_out_dim_alpha,
                playlist_json = excluded.playlist_json;",
            params![
                s.id,
                s.name,
                s.node_id,
                s.rotation,
                s.logical_width,
                s.logical_height,
                s.sold_out_mode,
                s.sold_out_badge,
                s.sold_out_dim_alpha,
                playlist_json
            ],
        )?;
        Ok(())
    }

    pub fn get_screen(&self, id: &str) -> Result<Option<Screen>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, node_id, rotation, logical_width, logical_height, sold_out_mode, sold_out_badge, sold_out_dim_alpha, playlist_json
             FROM screens WHERE id = ?1;",
        )?;
        stmt.query_row([id], |r| {
            let playlist_str: String = r.get(9)?;
            let playlist = serde_json::from_str(&playlist_str).unwrap_or_default();
            Ok(Screen {
                id: r.get(0)?,
                name: r.get(1)?,
                node_id: r.get(2)?,
                rotation: r.get(3)?,
                logical_width: r.get(4)?,
                logical_height: r.get(5)?,
                sold_out_mode: r.get(6)?,
                sold_out_badge: r.get(7)?,
                sold_out_dim_alpha: r.get(8)?,
                playlist,
            })
        })
        .optional()
    }

    /// Sets announcement text and visibility in live_state table (RND-008, PUB-004).
    pub fn set_announcement(&self, text: &str, visible: bool) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO live_state (key, value) VALUES ('announcement_text', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value;",
            params![text],
        )?;
        conn.execute(
            "INSERT INTO live_state (key, value) VALUES ('announcement_visible', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value;",
            params![if visible { "1" } else { "0" }],
        )?;
        Ok(())
    }

    pub fn get_announcement(&self) -> Result<(String, bool)> {
        let conn = self.conn.lock().unwrap();
        let text: String = conn
            .query_row(
                "SELECT value FROM live_state WHERE key = 'announcement_text';",
                [],
                |r| r.get(0),
            )
            .unwrap_or_default();
        let visible_str: String = conn
            .query_row(
                "SELECT value FROM live_state WHERE key = 'announcement_visible';",
                [],
                |r| r.get(0),
            )
            .unwrap_or_else(|_| "0".to_string());
        Ok((text, visible_str == "1"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn in_memory_store_crud_and_live_state() {
        let store = Store::open_in_memory().unwrap();

        assert_eq!(
            store.pragma_journal_mode().unwrap().to_uppercase(),
            "MEMORY"
        );

        // Category
        let cat = Category {
            id: "cat-lunch".to_string(),
            title: "Lunch Specials".to_string(),
            description: "11:00 - 14:00".to_string(),
            sort_order: 1,
            image: Some("img001".to_string()),
        };
        store.insert_category(&cat).unwrap();
        let cats = store.get_categories().unwrap();
        assert_eq!(cats.len(), 1);
        assert_eq!(cats[0], cat);

        // Item
        let item = Item {
            id: "item-pasta".to_string(),
            category_id: "cat-lunch".to_string(),
            name: "Pesto Pasta".to_string(),
            description: "Fresh basil pesto".to_string(),
            price_primary: "95 kr".to_string(),
            price_secondary: "".to_string(),
            badge: "VEGAN".to_string(),
            image_hash: "hash123".to_string(),
            sold_out: false,
            sort_order: 1,
        };
        store.insert_item(&item).unwrap();
        let items = store.get_items_for_category("cat-lunch").unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].name, "Pesto Pasta");

        // Live sold-out toggle
        store.set_item_sold_out("item-pasta", true).unwrap();
        let items_after = store.get_items_for_category("cat-lunch").unwrap();
        assert!(items_after[0].sold_out);

        // Price variant
        let variant = PriceVariant {
            id: "var-large".to_string(),
            item_id: "item-pasta".to_string(),
            label: "Large".to_string(),
            amount: 11500, // 115.00 kr in minor units
        };
        store.insert_price_variant(&variant).unwrap();

        // Template
        let tpl = Template {
            id: "tpl-grid".to_string(),
            name: "Grid Layout".to_string(),
            schema_version: 1,
            document_json: "{}".to_string(),
        };
        store.insert_template(&tpl).unwrap();
        assert_eq!(store.get_template("tpl-grid").unwrap().unwrap(), tpl);

        // Page
        let page = Page {
            id: "page-lunch".to_string(),
            template_id: "tpl-grid".to_string(),
            title: "Lunch Menu".to_string(),
            duration_ms: 7000,
            transition: "slide".to_string(),
            transition_ms: 250,
            repeater_capacity: 6,
            schedule_start_min: 660,
            schedule_end_min: 840,
            category_id: Some("cat-lunch".to_string()),
            sort_order: 1,
        };
        store.insert_page(&page).unwrap();
        assert_eq!(store.get_page("page-lunch").unwrap().unwrap(), page);

        // Screen
        let screen = Screen {
            id: "screen-bar".to_string(),
            name: "Bar Counter".to_string(),
            node_id: "pi4-bar".to_string(),
            rotation: 90,
            logical_width: 1080,
            logical_height: 1920,
            sold_out_mode: "dim".to_string(),
            sold_out_badge: "SLUT".to_string(),
            sold_out_dim_alpha: 80,
            playlist: vec!["page-lunch".to_string()],
        };
        store.insert_screen(&screen).unwrap();
        assert_eq!(store.get_screen("screen-bar").unwrap().unwrap(), screen);

        // Announcement
        store.set_announcement("Happy Hour 16-18!", true).unwrap();
        let (ann_text, ann_vis) = store.get_announcement().unwrap();
        assert_eq!(ann_text, "Happy Hour 16-18!");
        assert!(ann_vis);

        // Checkpoint interval check
        assert!(!store.check_checkpoint_interval().unwrap());
    }
}
