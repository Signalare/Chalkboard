// SPDX-License-Identifier: MIT OR Apache-2.0

//! Template document model, mirroring `schema/template.v1.json`.
//!
//! Every struct rejects unknown fields so that typos surface as errors
//! instead of being silently ignored. Value ranges and cross-references are
//! checked afterwards by [`crate::validate`].

use std::collections::BTreeMap;

use serde::Deserialize;

/// A complete template document (TPL-001).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Template {
    #[serde(rename = "$schema")]
    pub schema: Option<String>,
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub canvas: Canvas,
    #[serde(default)]
    pub fonts: BTreeMap<String, FontFace>,
    #[serde(default)]
    pub styles: BTreeMap<String, Style>,
    pub sold_out: Option<SoldOut>,
    pub root: Node,
}

/// Logical drawing surface; also fixes the orientation (TPL-002).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Canvas {
    pub width: u32,
    pub height: u32,
    pub background: Option<String>,
}

/// A custom font uploaded as a blob.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FontFace {
    pub src: String,
}

/// A named, reusable set of visual properties (TPL-009).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Style {
    pub font: Option<String>,
    pub size: Option<u32>,
    pub weight: Option<u32>,
    pub italic: Option<bool>,
    pub color: Option<String>,
    pub background: Option<String>,
    pub align: Option<Align>,
    pub line_height: Option<f64>,
    pub letter_spacing: Option<i32>,
    pub padding: Option<Spacing>,
    pub radius: Option<u32>,
    pub border_width: Option<u32>,
    pub border_color: Option<String>,
    pub opacity: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Align {
    Start,
    Center,
    End,
}

/// Uniform, `[vertical, horizontal]` or `[top, right, bottom, left]`.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum Spacing {
    Uniform(u32),
    Sides(Vec<u32>),
}

/// How sold-out items are presented (TPL-008).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SoldOut {
    pub mode: SoldOutMode,
    pub opacity: Option<f64>,
    pub badge_text: Option<String>,
    pub badge_style: Option<StyleRef>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SoldOutMode {
    Hide,
    Dim,
    Badge,
}

/// One style name or a list applied in order.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum StyleRef {
    One(String),
    Many(Vec<String>),
}

impl StyleRef {
    pub fn names(&self) -> Vec<&str> {
        match self {
            StyleRef::One(name) => vec![name.as_str()],
            StyleRef::Many(names) => names.iter().map(String::as_str).collect(),
        }
    }
}

/// Width or height: logical pixels, `"content"` or `"fill"`.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum Dimension {
    Pixels(u32),
    Keyword(String),
}

/// Grid track: logical pixels, `"content"` or `"<n>fr"`.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum Track {
    Pixels(u32),
    Keyword(String),
}

/// Child arrangement, matching LVGL's flex and grid layouts.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase", deny_unknown_fields)]
pub enum Layout {
    #[serde(rename_all = "camelCase")]
    Flex {
        direction: Option<Direction>,
        wrap: Option<bool>,
        gap: Option<u32>,
        justify: Option<Justify>,
        align: Option<CrossAlign>,
    },
    #[serde(rename_all = "camelCase")]
    Grid {
        columns: Vec<Track>,
        rows: Vec<Track>,
        column_gap: Option<u32>,
        row_gap: Option<u32>,
    },
    Absolute,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    Row,
    Column,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Justify {
    Start,
    Center,
    End,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CrossAlign {
    Start,
    Center,
    End,
    Stretch,
}

/// Placement of a child inside a grid parent.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Cell {
    pub column: u32,
    pub row: u32,
    pub column_span: Option<u32>,
    pub row_span: Option<u32>,
}

/// Properties every node shares: identity, styling and placement.
#[derive(Debug, Clone, Default)]
pub struct Common {
    pub id: Option<String>,
    pub style: Option<StyleRef>,
    pub width: Option<Dimension>,
    pub height: Option<Dimension>,
    pub grow: Option<u32>,
    pub x: Option<u32>,
    pub y: Option<u32>,
    pub cell: Option<Cell>,
}

/// Declares one node struct with the shared fields plus its own.
macro_rules! node_struct {
    ($(#[$meta:meta])* $name:ident { $($(#[$fmeta:meta])* $field:ident : $ty:ty),* $(,)? }) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        pub struct $name {
            pub id: Option<String>,
            pub style: Option<StyleRef>,
            pub width: Option<Dimension>,
            pub height: Option<Dimension>,
            pub grow: Option<u32>,
            pub x: Option<u32>,
            pub y: Option<u32>,
            pub cell: Option<Cell>,
            $($(#[$fmeta])* pub $field: $ty),*
        }

        impl $name {
            pub fn common(&self) -> Common {
                Common {
                    id: self.id.clone(),
                    style: self.style.clone(),
                    width: self.width.clone(),
                    height: self.height.clone(),
                    grow: self.grow,
                    x: self.x,
                    y: self.y,
                    cell: self.cell.clone(),
                }
            }
        }
    };
}

node_struct!(
    /// Groups children with a layout.
    Container {
        layout: Option<Layout>,
        #[serde(default)]
        children: Vec<Node>,
    }
);

node_struct!(
    /// Text with `{{bindings}}` (TPL-004).
    Text {
        text: String,
        fit: Fit,
    }
);

node_struct!(
    /// A price, shrunk rather than cut off when it does not fit.
    Price {
        value: String,
        currency: Option<Currency>,
        min_size: u32,
    }
);

node_struct!(
    /// A blob image or an item/category image binding.
    Image {
        src: String,
        scale: Option<Scale>,
    }
);

node_struct!(
    /// A filled or outlined shape.
    Shape {
        shape: ShapeKind,
    }
);

node_struct!(
    /// Repeats `template` for each entry of `source` (TPL-005, TPL-006).
    Repeater {
        source: Source,
        capacity: u32,
        sort: Option<Sort>,
        layout: Option<Layout>,
        template: Box<Node>,
        empty: Option<Box<Node>>,
    }
);

node_struct!(
    /// Shows `then` or `else` depending on `when` (TPL-007).
    Conditional {
        when: Condition,
        then: Box<Node>,
        #[serde(rename = "else")]
        otherwise: Option<Box<Node>>,
    }
);

/// Any template node.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Node {
    Container(Container),
    Text(Text),
    Price(Price),
    Image(Image),
    Shape(Shape),
    Repeater(Repeater),
    Conditional(Conditional),
}

impl Node {
    pub fn kind(&self) -> &'static str {
        match self {
            Node::Container(_) => "container",
            Node::Text(_) => "text",
            Node::Price(_) => "price",
            Node::Image(_) => "image",
            Node::Shape(_) => "shape",
            Node::Repeater(_) => "repeater",
            Node::Conditional(_) => "conditional",
        }
    }

    pub fn common(&self) -> Common {
        match self {
            Node::Container(n) => n.common(),
            Node::Text(n) => n.common(),
            Node::Price(n) => n.common(),
            Node::Image(n) => n.common(),
            Node::Shape(n) => n.common(),
            Node::Repeater(n) => n.common(),
            Node::Conditional(n) => n.common(),
        }
    }
}

/// Text fitting policy.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "mode", rename_all = "lowercase", deny_unknown_fields)]
pub enum Fit {
    #[serde(rename_all = "camelCase")]
    Wrap {
        max_lines: u32,
    },
    #[serde(rename_all = "camelCase")]
    Shrink {
        min_size: u32,
    },
    Clip,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Currency {
    Primary,
    Secondary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Scale {
    Cover,
    Contain,
    Stretch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ShapeKind {
    Rect,
    Ellipse,
    Line,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum Source {
    #[serde(rename = "items")]
    Items,
    #[serde(rename = "categories")]
    Categories,
    #[serde(rename = "category.items")]
    CategoryItems,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Sort {
    pub by: SortBy,
    pub direction: Option<SortDirection>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SortBy {
    Order,
    Name,
    Price,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SortDirection {
    Asc,
    Desc,
}

/// A predicate; exactly one field must be set (checked by the validator).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Condition {
    pub sold_out: Option<bool>,
    pub has_image: Option<bool>,
    pub has_description: Option<bool>,
    pub has_tag: Option<String>,
    pub has_badge: Option<String>,
    pub has_variant: Option<String>,
    pub language: Option<String>,
    pub not: Option<Box<Condition>>,
    pub all: Option<Vec<Condition>>,
    pub any: Option<Vec<Condition>>,
}
