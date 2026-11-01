// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

use chalkboard_agent::builder::{SnapshotBuilder, read_pointer, save_cached_image};
use chalkboard_agent::ipc::{
    AnnouncementMsg, IpcServer, RendererClient, RendererCommand, RendererResponse, SoldOutItem,
};
use chalkboard_agent::store::{Category, Item, Page, Screen, Store, Template};
use chalkboard_agent::supervisor::ProcessSupervisor;

fn make_temp_dir(label: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("chalkboard-test-{}-{}", label, std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn sto_001_sqlite_wal_mode_and_group_commit() {
    let tmp = make_temp_dir("sto-001");
    let db_path = tmp.join("chalkboard.db");

    let store = Store::open(&db_path, None).expect("failed to open store");

    // Verify WAL mode and synchronous=NORMAL on the store connection (STO-001)
    assert_eq!(store.pragma_journal_mode().unwrap().to_uppercase(), "WAL");
    assert_eq!(store.pragma_synchronous().unwrap(), 1); // 1 = NORMAL

    // Test group commit queuing
    store
        .queue_mutation("INSERT INTO categories (id, title) VALUES ('c1', 'Drinks');")
        .unwrap();
    store
        .queue_mutation("INSERT INTO categories (id, title) VALUES ('c2', 'Food');")
        .unwrap();

    let flushed = store.flush_group_commit().unwrap();
    assert_eq!(flushed, 2);

    let cats = store.get_categories().unwrap();
    assert_eq!(cats.len(), 2);
    assert_eq!(cats[0].title, "Drinks");

    // Test checkpointing
    store.checkpoint_wal().expect("checkpoint must succeed");

    let _ = fs::remove_dir_all(&tmp);
}

#[test]
fn sto_005_corruption_detection_and_backup_recovery() {
    let tmp = make_temp_dir("sto-005");
    let db_path = tmp.join("data.db");
    let backup_path = tmp.join("data.bak");

    // 1. Create a store with initial data and take an atomic backup
    let store = Store::open(&db_path, Some(backup_path.clone())).unwrap();
    store
        .insert_category(&Category {
            id: "cat-1".to_string(),
            title: "Original Bakery".to_string(),
            description: "Fresh bread".to_string(),
            sort_order: 1,
            image: None,
        })
        .unwrap();
    store.create_backup(&backup_path).unwrap();
    drop(store);

    // 2. Corrupt the database file by writing junk into header/pages
    {
        let mut f = File::create(&db_path).unwrap();
        f.write_all(b"CORRUPTED_JUNK_DATA_NOT_SQLITE").unwrap();
    }

    // 3. Reopening the store must detect corruption, quarantine corrupt file, and restore from backup
    let recovered_store =
        Store::open(&db_path, Some(backup_path)).expect("store should auto-recover");
    let cats = recovered_store.get_categories().unwrap();
    assert_eq!(cats.len(), 1);
    assert_eq!(cats[0].title, "Original Bakery");

    let _ = fs::remove_dir_all(&tmp);
}

#[test]
fn pub_003_sto_002_sto_003_snapshot_builder_and_atomic_switch() {
    let tmp = make_temp_dir("pub-003");
    let db_path = tmp.join("agent.db");
    let snapshots_root = tmp.join("snapshots");

    let store = Store::open(&db_path, None).unwrap();

    // Populate Category, Item, Template, Page, Screen
    store
        .insert_category(&Category {
            id: "cat-drinks".to_string(),
            title: "Hot Drinks".to_string(),
            description: "Artisan coffee".to_string(),
            sort_order: 1,
            image: None,
        })
        .unwrap();

    store
        .insert_item(&Item {
            id: "item-cappuccino".to_string(),
            category_id: "cat-drinks".to_string(),
            name: "Cappuccino".to_string(),
            description: "Double espresso with foamed oat milk".to_string(),
            price_primary: "45 kr".to_string(),
            price_secondary: "€4.00".to_string(),
            badge: "FAVORITE".to_string(),
            image_hash: "a1b2c3d4e5f600112233445566778899".to_string(),
            sold_out: false,
            sort_order: 1,
        })
        .unwrap();

    store
        .insert_template(&Template {
            id: "tpl-1".to_string(),
            name: "Classic Grid".to_string(),
            schema_version: 1,
            document_json: "{}".to_string(),
        })
        .unwrap();

    store
        .insert_page(&Page {
            id: "page-1".to_string(),
            template_id: "tpl-1".to_string(),
            title: "COFFEE BAR".to_string(),
            duration_ms: 6000,
            transition: "fade".to_string(),
            transition_ms: 300,
            repeater_capacity: 4,
            schedule_start_min: 360,
            schedule_end_min: 1100,
            category_id: Some("cat-drinks".to_string()),
            sort_order: 1,
        })
        .unwrap();

    store
        .insert_screen(&Screen {
            id: "screen-main".to_string(),
            name: "Bar Counter Screen".to_string(),
            node_id: "pi4-counter".to_string(),
            rotation: 0,
            logical_width: 1920,
            logical_height: 1080,
            sold_out_mode: "badge".to_string(),
            sold_out_badge: "SLUT".to_string(),
            sold_out_dim_alpha: 96,
            playlist: vec!["page-1".to_string()],
        })
        .unwrap();

    let builder = SnapshotBuilder::new(&store, &snapshots_root);
    let hash_v1 = builder.build_and_publish("screen-main", 1).unwrap();

    // Verify pointer was updated atomically (STO-003)
    let current_ptr = read_pointer(&snapshots_root, "current").unwrap();
    assert_eq!(current_ptr, hash_v1);

    // Save pre-rendered image asset (STO-003)
    let pixels = vec![0x00f2c14e; 16];
    save_cached_image(
        &snapshots_root,
        &hash_v1,
        "a1b2c3d4e5f600112233445566778899",
        4,
        4,
        &pixels,
    )
    .unwrap();

    let img_cache_file = snapshots_root
        .join("by-hash")
        .join(&hash_v1)
        .join("render-cache")
        .join("a1b2c3d4e5f600112233445566778899.cbimg");
    assert!(img_cache_file.exists());

    // Update item and build v2 -> previous must point to v1 (STO-004)
    store.set_item_sold_out("item-cappuccino", true).unwrap();
    let hash_v2 = builder.build_and_publish("screen-main", 2).unwrap();
    assert_ne!(hash_v1, hash_v2);

    let current_ptr_v2 = read_pointer(&snapshots_root, "current").unwrap();
    let previous_ptr_v2 = read_pointer(&snapshots_root, "previous").unwrap();
    assert_eq!(current_ptr_v2, hash_v2);
    assert_eq!(previous_ptr_v2, hash_v1);

    // Verify compiled snapshot bundle directly with the C chalkboard-render binary if available!
    let render_bin = PathBuf::from("../../build/render/chalkboard-render");
    if render_bin.exists() {
        let status = Command::new(&render_bin)
            .arg("--snapshots")
            .arg(&snapshots_root)
            .arg("--headless-once")
            .status()
            .unwrap();
        assert!(status.success());
    }

    let _ = fs::remove_dir_all(&tmp);
}

#[test]
fn rnd_006_unix_socket_renderer_ipc() {
    let (server_stream, client_stream) = std::os::unix::net::UnixStream::pair().unwrap();

    // Start mock IPC server handling RND-006 commands
    let server_handle = IpcServer::serve_stream(server_stream, |cmd| match cmd {
        RendererCommand::LoadSnapshot { hash } => RendererResponse::Ok {
            message: Some(format!("loaded {hash}")),
        },
        RendererCommand::ApplyLiveState { sold_out, .. } => RendererResponse::Ok {
            message: Some(format!("applied {} items", sold_out.len())),
        },
        RendererCommand::Identify {
            enabled, node_name, ..
        } => RendererResponse::Ok {
            message: Some(format!("identify={enabled} node={node_name}")),
        },
        RendererCommand::Screenshot => RendererResponse::Screenshot {
            width: 1920,
            height: 1080,
            frame_hash: 0xfeedfacecafebeef,
        },
        RendererCommand::Status => RendererResponse::Status {
            mode: "content".to_string(),
            active_hash: "aaaa1111".to_string(),
            fallback_hash: "bbbb2222".to_string(),
            subpage: 0,
            subpage_total: 2,
        },
    });

    let client = RendererClient::from_stream(client_stream);

    // 1. LoadSnapshot
    match client.load_snapshot("aaaa1111").unwrap() {
        RendererResponse::Ok { message } => {
            assert!(message.unwrap().contains("aaaa1111"));
        }
        _ => panic!("expected Ok"),
    }

    // 2. ApplyLiveState
    let sold_out = vec![SoldOutItem {
        item_id: "espresso".to_string(),
        sold_out: true,
    }];
    let announcement = Some(AnnouncementMsg {
        text: "Closing early today at 18:00".to_string(),
        visible: true,
    });
    match client.apply_live_state(sold_out, announcement).unwrap() {
        RendererResponse::Ok { message } => {
            assert!(message.unwrap().contains("applied 1 items"));
        }
        _ => panic!("expected Ok"),
    }

    // 3. Identify
    match client
        .identify(true, "pi4-front", "Main Board", "192.168.1.10")
        .unwrap()
    {
        RendererResponse::Ok { message } => {
            assert!(message.unwrap().contains("pi4-front"));
        }
        _ => panic!("expected Ok"),
    }

    // 4. Screenshot
    match client.screenshot().unwrap() {
        RendererResponse::Screenshot {
            width,
            height,
            frame_hash,
        } => {
            assert_eq!(width, 1920);
            assert_eq!(height, 1080);
            assert_eq!(frame_hash, 0xfeedfacecafebeef);
        }
        _ => panic!("expected Screenshot"),
    }

    // 5. Status
    match client.status().unwrap() {
        RendererResponse::Status {
            mode, active_hash, ..
        } => {
            assert_eq!(mode, "content");
            assert_eq!(active_hash, "aaaa1111");
        }
        _ => panic!("expected Status"),
    }

    drop(client);
    let _ = server_handle.join();
}

#[test]
fn os_006_supervisor_restarts_crashed_child_under_1s() {
    // Run a sleep command child
    let mut supervisor = ProcessSupervisor::new("sleep", vec!["10".to_string()]);
    supervisor.start().expect("supervisor must spawn child");

    assert!(supervisor.is_running());
    let initial_pid = supervisor.child_pid().expect("child must have PID");

    let start_kill = Instant::now();
    // Simulate crash / kill of the supervised child
    assert!(supervisor.kill_child());

    // OS-006: Supervisor must restart crashed process within 1 s
    let mut restarted = false;
    while start_kill.elapsed() < Duration::from_millis(900) {
        if let Some(new_pid) = supervisor.child_pid() {
            if new_pid != initial_pid && supervisor.restart_count() >= 1 {
                restarted = true;
                break;
            }
        }
        thread::sleep(Duration::from_millis(20));
    }

    assert!(
        restarted,
        "supervisor failed to restart crashed process within 1 s"
    );
    supervisor.stop();
}

#[test]
fn web_001_web_007_http_api_endpoints() {
    let tmp = make_temp_dir("http-api");
    let db_path = tmp.join("chalkboard.db");
    let snapshots_root = tmp.join("snapshots");
    let web_dist = tmp.join("dist");
    fs::create_dir_all(&web_dist).unwrap();
    fs::write(
        web_dist.join("index.html"),
        "<html><body>Chalkboard</body></html>",
    )
    .unwrap();

    let store = Store::open(&db_path, None).unwrap();

    // Helper closure to process HTTP requests directly via handle_connection (WEB-001, WEB-007)
    let send_req = |method: &str, path: &str, body: &str| -> (u16, String) {
        let req = format!(
            "{method} {path} HTTP/1.1\r\n\
             Host: localhost\r\n\
             Content-Type: application/json\r\n\
             Content-Length: {}\r\n\
             Connection: close\r\n\
             \r\n\
             {body}",
            body.len()
        );
        let mut writer = Vec::new();
        chalkboard_agent::http::HttpServer::handle_connection(
            req.as_bytes(),
            &mut writer,
            store.clone(),
            &snapshots_root,
            &web_dist,
            None,
        )
        .unwrap();

        let resp = String::from_utf8(writer).unwrap();
        let status_code: u16 = resp
            .lines()
            .next()
            .and_then(|l| l.split_whitespace().nth(1))
            .and_then(|c| c.parse().ok())
            .unwrap_or(0);
        let body = resp.split("\r\n\r\n").nth(1).unwrap_or("").to_string();
        (status_code, body)
    };

    // 1. GET /api/v1/status
    let (code, body) = send_req("GET", "/api/v1/status", "");
    assert_eq!(code, 200);
    assert!(body.contains("\"status\":\"ok\""));

    // 2. POST /api/v1/categories
    let cat_json = r#"{"id":"cat-specials","title":"Today Specials","description":"Chef picks","sort_order":1,"image":null}"#;
    let (code, body) = send_req("POST", "/api/v1/categories", cat_json);
    assert_eq!(code, 200);
    assert!(body.contains("Today Specials"));

    // 3. GET /api/v1/categories
    let (code, body) = send_req("GET", "/api/v1/categories", "");
    assert_eq!(code, 200);
    assert!(body.contains("cat-specials"));

    // 4. POST /api/v1/items
    let item_json = r#"{"id":"item-soup","category_id":"cat-specials","name":"Tuscan Tomato Soup","description":"With garlic croutons","price_primary":"75 kr","price_secondary":"","badge":"NEW","image_hash":"","sold_out":false,"sort_order":1}"#;
    let (code, body) = send_req("POST", "/api/v1/items", item_json);
    assert_eq!(code, 200);
    assert!(body.contains("Tuscan Tomato Soup"));

    // 5. POST /api/v1/items/item-soup/sold-out (WEB-005)
    let (code, body) = send_req(
        "POST",
        "/api/v1/items/item-soup/sold-out",
        r#"{"sold_out":true}"#,
    );
    assert_eq!(code, 200);
    assert!(body.contains("\"status\":\"ok\""));

    // 6. GET /api/v1/templates
    let (code, body) = send_req("GET", "/api/v1/templates", "");
    assert_eq!(code, 200);
    assert!(body.contains("list-classic"));

    // 7. POST /api/v1/screens
    let screen_json = r#"{"id":"screen-main","name":"Front Screen","node_id":"node-01","rotation":0,"logical_width":1920,"logical_height":1080,"sold_out_mode":"badge","sold_out_badge":"SOLD OUT","sold_out_dim_alpha":102,"playlist":[]}"#;
    let (code, _) = send_req("POST", "/api/v1/screens", screen_json);
    assert_eq!(code, 200);

    // 8. POST /api/v1/publish
    let (code, body) = send_req("POST", "/api/v1/publish", "");
    assert_eq!(code, 200);
    assert!(body.contains("\"status\":\"ok\""));
    assert!(body.contains("\"hash\":"));

    // 9. Static file fallback
    let (code, body) = send_req("GET", "/", "");
    assert_eq!(code, 200);
    assert!(body.contains("Chalkboard"));

    let _ = fs::remove_dir_all(&tmp);
}
