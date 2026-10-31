// SPDX-License-Identifier: MIT OR Apache-2.0

//! Data models for Milestone 0.1 entities (SPEC §9).

use serde::{Deserialize, Serialize};

/// Menu category (SPEC §9).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Category {
    pub id: String,
    pub title: String,
    pub description: String,
    pub sort_order: i32,
    pub image: Option<String>,
}

/// Menu item (SPEC §9, DATA-001..DATA-005).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Item {
    pub id: String,
    pub category_id: String,
    pub name: String,
    pub description: String,
    pub price_primary: String,
    pub price_secondary: String,
    pub badge: String,
    pub image_hash: String,
    pub sold_out: bool,
    pub sort_order: i32,
}

/// Price variant for an item in minor currency units (SPEC §9, DATA-001).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PriceVariant {
    pub id: String,
    pub item_id: String,
    pub label: String,
    pub amount: i64,
}

/// Template entity (SPEC §8, SPEC §9).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Template {
    pub id: String,
    pub name: String,
    pub schema_version: u32,
    pub document_json: String,
}

/// Page configuration referencing a template and binding (SPEC §9, TPL-005, TPL-006).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Page {
    pub id: String,
    pub template_id: String,
    pub title: String,
    pub duration_ms: u32,
    pub transition: String,
    pub transition_ms: u32,
    pub repeater_capacity: u32,
    pub schedule_start_min: u16,
    pub schedule_end_min: u16,
    pub category_id: Option<String>,
    pub sort_order: i32,
}

/// Screen display configuration and playlist (SPEC §9, RND-002, RND-015, RND-016).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Screen {
    pub id: String,
    pub name: String,
    pub node_id: String,
    pub rotation: u32,
    pub logical_width: u32,
    pub logical_height: u32,
    pub sold_out_mode: String,
    pub sold_out_badge: String,
    pub sold_out_dim_alpha: u8,
    pub playlist: Vec<String>,
}
