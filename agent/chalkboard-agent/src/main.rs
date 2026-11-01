// SPDX-License-Identifier: MIT OR Apache-2.0

//! `chalkboard-agent`: discovery, replication, HTTP API and web app, HDMI-CEC,
//! time, provisioning and updates (SPEC §3).

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use chalkboard_agent::banner;
use chalkboard_agent::cec::CecService;
use chalkboard_agent::http::HttpServer;
use chalkboard_agent::store::{Category, Item, Page, Screen, Store, Template};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("{}", banner());
        return Ok(());
    }

    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("{}", banner());
        println!("\nUsage: chalkboard-agent [OPTIONS]\n");
        println!("Options:");
        println!("  --bind <ADDR>      Address to bind HTTP server [default: 127.0.0.1:8080]");
        println!("  --db <PATH>        SQLite database path [default: build/data/chalkboard.db]");
        println!("  --web-dist <PATH>  Web static assets directory [default: web/dist]");
        println!("  --snapshots <PATH> Snapshots root directory [default: build/snapshots]");
        println!(
            "  --mock-hardware    Enable mock hardware adapters for offline development (#25)"
        );
        println!("  --version, -V      Print version banner");
        println!("  --help, -h         Print help information");
        return Ok(());
    }

    println!("{}", banner());

    let mut bind_addr = "127.0.0.1:8080".to_string();
    let mut db_path = "build/data/chalkboard.db".to_string();
    let mut web_dist = "web/dist".to_string();
    let mut snapshots = "build/snapshots".to_string();
    let mut mock_hardware = std::env::var("CHALKBOARD_MOCK_HARDWARE")
        .map(|v| v == "1" || v == "true")
        .unwrap_or(false);

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--bind" if i + 1 < args.len() => {
                bind_addr = args[i + 1].clone();
                i += 2;
            }
            "--db" if i + 1 < args.len() => {
                db_path = args[i + 1].clone();
                i += 2;
            }
            "--web-dist" if i + 1 < args.len() => {
                web_dist = args[i + 1].clone();
                i += 2;
            }
            "--snapshots" if i + 1 < args.len() => {
                snapshots = args[i + 1].clone();
                i += 2;
            }
            "--mock-hardware" => {
                mock_hardware = true;
                i += 1;
            }
            _ => {
                i += 1;
            }
        }
    }

    if let Some(parent) = Path::new(&db_path).parent() {
        fs::create_dir_all(parent)?;
    }
    fs::create_dir_all(&snapshots)?;

    let store = Store::open(&db_path, None)?;
    seed_demo_data_if_empty(&store)?;

    let cec_service = CecService::new_auto(mock_hardware, "Chalkboard");
    println!(
        "CEC adapter initialized: {} (mock={})",
        cec_service.primary().device_name(),
        cec_service.primary().is_mock()
    );

    let server = HttpServer::start(
        &bind_addr,
        store,
        PathBuf::from(&snapshots),
        PathBuf::from(&web_dist),
        None,
        Some(cec_service),
    )?;

    println!(
        "Chalkboard server running at: http://{}",
        server.local_addr()
    );
    println!("API root: http://{}/api/v1", server.local_addr());
    println!("Serving UI from: {}", web_dist);
    println!("Press Ctrl+C to terminate.");

    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    while r.load(Ordering::SeqCst) {
        std::thread::sleep(std::time::Duration::from_millis(500));
    }

    Ok(())
}

fn seed_demo_data_if_empty(store: &Store) -> rusqlite::Result<()> {
    if !store.get_categories()?.is_empty() {
        return Ok(());
    }

    let cat_pizza = Category {
        id: "cat-pizzas".to_string(),
        title: "Artisan Pizzas".to_string(),
        description: "Hand-stretched sourdough pizzas baked in our stone oven".to_string(),
        sort_order: 1,
        image: None,
    };
    let cat_drinks = Category {
        id: "cat-drinks".to_string(),
        title: "Drinks & Refreshments".to_string(),
        description: "Cold drinks, local sodas, and craft beverages".to_string(),
        sort_order: 2,
        image: None,
    };
    store.insert_category(&cat_pizza)?;
    store.insert_category(&cat_drinks)?;

    store.insert_item(&Item {
        id: "item-margherita".to_string(),
        category_id: "cat-pizzas".to_string(),
        name: "Margherita Classica".to_string(),
        description:
            "San Marzano tomatoes, fresh fior di latte mozzarella, basil, extra virgin olive oil"
                .to_string(),
        price_primary: "125 kr".to_string(),
        price_secondary: "Slice 45 kr".to_string(),
        badge: "Chef's Choice".to_string(),
        image_hash: "".to_string(),
        sold_out: false,
        sort_order: 1,
    })?;
    store.insert_item(&Item {
        id: "item-diavola".to_string(),
        category_id: "cat-pizzas".to_string(),
        name: "Diavola Piccante".to_string(),
        description: "Spicy calabrese salami, chili flakes, mozzarella, tomato sauce".to_string(),
        price_primary: "145 kr".to_string(),
        price_secondary: "".to_string(),
        badge: "Spicy".to_string(),
        image_hash: "".to_string(),
        sold_out: false,
        sort_order: 2,
    })?;
    store.insert_item(&Item {
        id: "item-quattro".to_string(),
        category_id: "cat-pizzas".to_string(),
        name: "Quattro Formaggi".to_string(),
        description: "Gorgonzola DOP, taleggio, smoked provola, mozzarella, walnuts".to_string(),
        price_primary: "155 kr".to_string(),
        price_secondary: "".to_string(),
        badge: "Vegetarian".to_string(),
        image_hash: "".to_string(),
        sold_out: true,
        sort_order: 3,
    })?;
    store.insert_item(&Item {
        id: "item-limonata".to_string(),
        category_id: "cat-drinks".to_string(),
        name: "San Pellegrino Limonata".to_string(),
        description: "Crisp sparkling lemonade made with Sicilian lemons 33cl".to_string(),
        price_primary: "38 kr".to_string(),
        price_secondary: "".to_string(),
        badge: "".to_string(),
        image_hash: "".to_string(),
        sold_out: false,
        sort_order: 1,
    })?;
    store.insert_item(&Item {
        id: "item-ipa".to_string(),
        category_id: "cat-drinks".to_string(),
        name: "Local Craft IPA".to_string(),
        description: "Locally brewed IPA with citrus and pine hop aromas 33cl".to_string(),
        price_primary: "79 kr".to_string(),
        price_secondary: "".to_string(),
        badge: "Local".to_string(),
        image_hash: "".to_string(),
        sold_out: false,
        sort_order: 2,
    })?;

    store.insert_screen(&Screen {
        id: "screen-main".to_string(),
        name: "Counter Main Display".to_string(),
        node_id: "node-1".to_string(),
        rotation: 0,
        logical_width: 1920,
        logical_height: 1080,
        sold_out_mode: "dim".to_string(),
        sold_out_badge: "SOLD OUT".to_string(),
        sold_out_dim_alpha: 120,
        playlist: vec!["page-pizzas".to_string()],
    })?;

    store.insert_template(&Template {
        id: "classic-list".to_string(),
        name: "Classic List".to_string(),
        schema_version: 1,
        document_json: "{}".to_string(),
    })?;
    store.insert_template(&Template {
        id: "two-column".to_string(),
        name: "Two Column".to_string(),
        schema_version: 1,
        document_json: "{}".to_string(),
    })?;
    store.insert_template(&Template {
        id: "grid-images".to_string(),
        name: "Grid Images".to_string(),
        schema_version: 1,
        document_json: "{}".to_string(),
    })?;
    store.insert_template(&Template {
        id: "highlight".to_string(),
        name: "Hero Highlight".to_string(),
        schema_version: 1,
        document_json: "{}".to_string(),
    })?;

    store.insert_page(&Page {
        id: "page-pizzas".to_string(),
        template_id: "classic-list".to_string(),
        title: "Artisan Pizzas".to_string(),
        duration_ms: 10000,
        transition: "crossfade".to_string(),
        transition_ms: 600,
        repeater_capacity: 8,
        schedule_start_min: 0,
        schedule_end_min: 1440,
        category_id: Some("cat-pizzas".to_string()),
        sort_order: 1,
    })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::banner;

    #[test]
    fn banner_names_the_binary_and_version() {
        assert_eq!(
            banner(),
            format!("chalkboard-agent {}", env!("CARGO_PKG_VERSION"))
        );
    }
}
