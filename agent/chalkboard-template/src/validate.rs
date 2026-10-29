// SPDX-License-Identifier: MIT OR Apache-2.0

//! Semantic checks that the JSON structure alone cannot express (TPL-010):
//! value ranges, references to styles and fonts, unique ids, binding paths
//! in their scope, and placement properties that match the parent layout.

use std::collections::BTreeSet;

use crate::Issue;
use crate::binding::{self, Scope, did_you_mean, is_price_path};
use crate::model::*;

/// Font families shipped in the image (RND-013).
pub const SHIPPED_FONTS: &[&str] = &["Inter", "Noto Sans", "Oswald", "Playfair Display"];

pub const MAX_NODES: usize = 2000;
pub const MAX_DEPTH: usize = 32;
const MAX_LENGTH: u32 = 8192;

/// The parent's layout, which decides which placement properties are valid.
#[derive(Clone, Copy)]
enum Parent<'a> {
    Root,
    Flex,
    Grid {
        columns: usize,
        rows: usize,
    },
    Absolute,
    /// `then`/`else`/`template`/`empty`: placement belongs to the wrapper.
    Wrapper(&'a str),
}

struct Checker<'a> {
    template: &'a Template,
    issues: Vec<Issue>,
    ids: BTreeSet<String>,
    nodes: usize,
}

pub fn check(template: &Template) -> Vec<Issue> {
    let mut c = Checker {
        template,
        issues: Vec::new(),
        ids: BTreeSet::new(),
        nodes: 0,
    };
    c.document();
    c.issues
}

impl Checker<'_> {
    fn issue(&mut self, path: &str, message: impl Into<String>) {
        self.issues.push(Issue {
            path: path.to_string(),
            message: message.into(),
        });
    }

    fn document(&mut self) {
        let t = self.template;
        if t.schema_version != 1 {
            self.issue(
                "schemaVersion",
                format!(
                    "unsupported schema version {}; expected 1",
                    t.schema_version
                ),
            );
        }
        if !is_identifier(&t.id) {
            self.issue(
                "id",
                "must be 1–63 lower-case letters, digits or '-', starting with a letter or digit",
            );
        }
        check_text_len(self, "name", &t.name, 1, 100);
        if let Some(d) = &t.description {
            check_text_len(self, "description", d, 0, 1000);
        }
        for (dim, v) in [("width", t.canvas.width), ("height", t.canvas.height)] {
            if !(320..=3840).contains(&v) {
                self.issue(&format!("canvas.{dim}"), format!("{v} is outside 320–3840"));
            }
        }
        if let Some(bg) = &t.canvas.background {
            self.color("canvas.background", bg);
        }
        if t.fonts.len() > 16 {
            self.issue("fonts", "at most 16 custom fonts");
        }
        for (name, face) in &t.fonts {
            let path = format!("fonts.{name}");
            self.name(&path, name);
            if SHIPPED_FONTS.contains(&name.as_str()) {
                self.issue(
                    &path,
                    format!("'{name}' is a shipped font and cannot be redefined"),
                );
            }
            if !is_blob_ref(&face.src) {
                self.issue(
                    &format!("{path}.src"),
                    "must be a blob reference: blob:<64 hex characters>",
                );
            }
        }
        if t.styles.len() > 128 {
            self.issue("styles", "at most 128 styles");
        }
        for (name, style) in &t.styles {
            self.style(&format!("styles.{name}"), name, style);
        }
        if let Some(sold_out) = &t.sold_out {
            self.sold_out(sold_out);
        }
        self.node("root", &t.root, Scope::default(), Parent::Root, 1);
        if self.nodes > MAX_NODES {
            self.issue(
                "root",
                format!("{} nodes; at most {MAX_NODES} allowed", self.nodes),
            );
        }
    }

    fn style(&mut self, path: &str, name: &str, s: &Style) {
        self.name(path, name);
        if let Some(font) = &s.font {
            if !SHIPPED_FONTS.contains(&font.as_str()) && !self.template.fonts.contains_key(font) {
                let known: Vec<&str> = SHIPPED_FONTS
                    .iter()
                    .copied()
                    .chain(self.template.fonts.keys().map(String::as_str))
                    .collect();
                self.issue(
                    &format!("{path}.font"),
                    format!(
                        "unknown font '{font}'{}",
                        did_you_mean(font, known.into_iter())
                    ),
                );
            }
        }
        if let Some(size) = s.size {
            self.range(&format!("{path}.size"), size, 6, 1000);
        }
        if let Some(w) = s.weight {
            if ![300, 400, 500, 600, 700, 800, 900].contains(&w) {
                self.issue(
                    &format!("{path}.weight"),
                    format!("{w} is not one of 300, 400, …, 900"),
                );
            }
        }
        for (field, color) in [
            ("color", &s.color),
            ("background", &s.background),
            ("borderColor", &s.border_color),
        ] {
            if let Some(color) = color {
                self.color(&format!("{path}.{field}"), color);
            }
        }
        if let Some(lh) = s.line_height {
            if !(0.5..=3.0).contains(&lh) {
                self.issue(
                    &format!("{path}.lineHeight"),
                    format!("{lh} is outside 0.5–3"),
                );
            }
        }
        if let Some(ls) = s.letter_spacing {
            if !(-50..=200).contains(&ls) {
                self.issue(
                    &format!("{path}.letterSpacing"),
                    format!("{ls} is outside -50–200"),
                );
            }
        }
        if let Some(op) = s.opacity {
            self.unit_interval(&format!("{path}.opacity"), op);
        }
        if let Some(padding) = &s.padding {
            match padding {
                Spacing::Uniform(v) => self.length(&format!("{path}.padding"), *v),
                Spacing::Sides(sides) => {
                    if sides.len() != 2 && sides.len() != 4 {
                        self.issue(&format!("{path}.padding"), "needs 1, 2 or 4 values");
                    }
                    for v in sides {
                        self.length(&format!("{path}.padding"), *v);
                    }
                }
            }
        }
        for (field, v) in [("radius", s.radius), ("borderWidth", s.border_width)] {
            if let Some(v) = v {
                self.length(&format!("{path}.{field}"), v);
            }
        }
    }

    fn sold_out(&mut self, s: &SoldOut) {
        if let Some(op) = s.opacity {
            self.unit_interval("soldOut.opacity", op);
        }
        if s.mode == SoldOutMode::Badge && s.badge_text.as_deref().unwrap_or("").is_empty() {
            self.issue("soldOut.badgeText", "required when mode is 'badge'");
        }
        if let Some(text) = &s.badge_text {
            check_text_len(self, "soldOut.badgeText", text, 1, 40);
        }
        if let Some(style) = &s.badge_style {
            self.style_ref("soldOut.badgeStyle", style);
        }
    }

    fn node(&mut self, path: &str, node: &Node, scope: Scope, parent: Parent, depth: usize) {
        self.nodes += 1;
        if depth > MAX_DEPTH {
            self.issue(path, format!("nesting deeper than {MAX_DEPTH} levels"));
            return;
        }
        self.common(path, node, parent);
        match node {
            Node::Container(n) => {
                if n.children.len() > 200 {
                    self.issue(&format!("{path}.children"), "at most 200 children");
                }
                let child_parent = self.layout(&format!("{path}.layout"), n.layout.as_ref());
                for (i, child) in n.children.iter().enumerate() {
                    self.node(
                        &format!("{path}.children[{i}]"),
                        child,
                        scope,
                        child_parent,
                        depth + 1,
                    );
                }
            }
            Node::Text(n) => {
                if n.text.chars().count() > 2000 {
                    self.issue(&format!("{path}.text"), "longer than 2000 characters");
                }
                match binding::parse(&n.text) {
                    Ok(bindings) => {
                        for b in bindings {
                            if let Err(e) = binding::check(&b, scope) {
                                self.issue(&format!("{path}.text"), e);
                            }
                        }
                    }
                    Err(e) => self.issue(&format!("{path}.text"), e),
                }
                match n.fit {
                    Fit::Wrap { max_lines } => {
                        self.range(&format!("{path}.fit.maxLines"), max_lines, 1, 20)
                    }
                    Fit::Shrink { min_size } => {
                        self.min_size(&format!("{path}.fit.minSize"), min_size, n.style.as_ref())
                    }
                    Fit::Clip => {}
                }
            }
            Node::Price(n) => {
                if !scope.item {
                    self.issue(
                        path,
                        "a price is only available inside a repeater over items",
                    );
                }
                if !is_price_path(&n.value) {
                    self.issue(
                        &format!("{path}.value"),
                        format!(
                            "'{}' is not a price; use item.price or item.prices.<variant>",
                            n.value
                        ),
                    );
                }
                self.min_size(&format!("{path}.minSize"), n.min_size, n.style.as_ref());
            }
            Node::Image(n) => {
                self.image_src(&format!("{path}.src"), &n.src, scope);
                for (dim, v) in [("width", &n.width), ("height", &n.height)] {
                    match v {
                        None => self.issue(
                            &format!("{path}.{dim}"),
                            "an image needs an explicit width and height",
                        ),
                        Some(Dimension::Keyword(k)) if k == "content" => self.issue(
                            &format!("{path}.{dim}"),
                            "use pixels or 'fill' for images, not 'content'",
                        ),
                        _ => {}
                    }
                }
            }
            Node::Shape(n) => {
                if n.width.is_none() || n.height.is_none() {
                    self.issue(path, "a shape needs an explicit width and height");
                }
            }
            Node::Repeater(n) => {
                let inner = match n.source {
                    Source::Items => Scope {
                        item: true,
                        ..scope
                    },
                    Source::Categories => Scope {
                        category: true,
                        item: false,
                    },
                    Source::CategoryItems => {
                        if !scope.category {
                            self.issue(
                                &format!("{path}.source"),
                                "'category.items' needs an enclosing repeater over categories",
                            );
                        }
                        Scope {
                            category: true,
                            item: true,
                        }
                    }
                };
                self.range(&format!("{path}.capacity"), n.capacity, 1, 200);
                let child_parent = self.layout(&format!("{path}.layout"), n.layout.as_ref());
                let entry_parent = match child_parent {
                    Parent::Absolute => {
                        self.issue(
                            &format!("{path}.layout"),
                            "repeated entries cannot use an absolute layout",
                        );
                        Parent::Flex
                    }
                    p => p,
                };
                self.node(
                    &format!("{path}.template"),
                    &n.template,
                    inner,
                    entry_parent,
                    depth + 1,
                );
                if let Some(empty) = &n.empty {
                    self.node(
                        &format!("{path}.empty"),
                        empty,
                        scope,
                        Parent::Wrapper("empty"),
                        depth + 1,
                    );
                }
            }
            Node::Conditional(n) => {
                self.condition(&format!("{path}.when"), &n.when, scope, 0);
                // The branches take the conditional's own place in the parent.
                self.node(
                    &format!("{path}.then"),
                    &n.then,
                    scope,
                    Parent::Wrapper("then"),
                    depth + 1,
                );
                if let Some(otherwise) = &n.otherwise {
                    self.node(
                        &format!("{path}.else"),
                        otherwise,
                        scope,
                        Parent::Wrapper("else"),
                        depth + 1,
                    );
                }
            }
        }
    }

    fn common(&mut self, path: &str, node: &Node, parent: Parent) {
        let c = node.common();
        if let Some(id) = &c.id {
            self.name(&format!("{path}.id"), id);
            if !self.ids.insert(id.clone()) {
                self.issue(&format!("{path}.id"), format!("duplicate id '{id}'"));
            }
        }
        if let Some(style) = &c.style {
            self.style_ref(&format!("{path}.style"), style);
        }
        for (dim, v) in [("width", &c.width), ("height", &c.height)] {
            match v {
                Some(Dimension::Pixels(px)) => self.length(&format!("{path}.{dim}"), *px),
                Some(Dimension::Keyword(k)) if k != "content" && k != "fill" => self.issue(
                    &format!("{path}.{dim}"),
                    format!("'{k}' is not pixels, 'content' or 'fill'"),
                ),
                _ => {}
            }
        }
        let positioned = c.x.is_some() || c.y.is_some();
        match parent {
            Parent::Absolute => {
                for (axis, v) in [("x", c.x), ("y", c.y)] {
                    if let Some(v) = v {
                        self.length(&format!("{path}.{axis}"), v);
                    }
                }
            }
            Parent::Wrapper(slot) => {
                if positioned || c.cell.is_some() || c.grow.is_some() {
                    self.issue(path, format!("placement (x/y/cell/grow) belongs on the enclosing node, not in '{slot}'"));
                }
            }
            _ if positioned => self.issue(path, "x/y are only allowed inside an absolute layout"),
            _ => {}
        }
        if let Some(grow) = c.grow {
            if !matches!(parent, Parent::Flex | Parent::Wrapper(_)) {
                self.issue(
                    &format!("{path}.grow"),
                    "grow is only allowed inside a flex layout",
                );
            }
            self.range(&format!("{path}.grow"), grow, 0, 100);
        }
        if let Some(cell) = &c.cell {
            match parent {
                Parent::Grid { columns, rows } => {
                    let span_c = cell.column_span.unwrap_or(1);
                    let span_r = cell.row_span.unwrap_or(1);
                    if (cell.column + span_c) as usize > columns {
                        self.issue(
                            &format!("{path}.cell"),
                            format!(
                                "column {} + span {span_c} exceeds the grid's {columns} columns",
                                cell.column
                            ),
                        );
                    }
                    if (cell.row + span_r) as usize > rows {
                        self.issue(
                            &format!("{path}.cell"),
                            format!(
                                "row {} + span {span_r} exceeds the grid's {rows} rows",
                                cell.row
                            ),
                        );
                    }
                }
                Parent::Wrapper(_) => {}
                _ => self.issue(
                    &format!("{path}.cell"),
                    "cell is only allowed inside a grid layout",
                ),
            }
        }
    }

    /// Validates a layout and returns how its children are placed.
    fn layout(&mut self, path: &str, layout: Option<&Layout>) -> Parent<'static> {
        match layout {
            None => Parent::Flex,
            Some(Layout::Flex { gap, .. }) => {
                if let Some(g) = gap {
                    self.length(&format!("{path}.gap"), *g);
                }
                Parent::Flex
            }
            Some(Layout::Grid {
                columns,
                rows,
                column_gap,
                row_gap,
            }) => {
                for (axis, tracks) in [("columns", columns), ("rows", rows)] {
                    if tracks.is_empty() || tracks.len() > 24 {
                        self.issue(&format!("{path}.{axis}"), "needs 1–24 tracks");
                    }
                    for (i, t) in tracks.iter().enumerate() {
                        self.track(&format!("{path}.{axis}[{i}]"), t);
                    }
                }
                for (field, gap) in [("columnGap", column_gap), ("rowGap", row_gap)] {
                    if let Some(g) = gap {
                        self.length(&format!("{path}.{field}"), *g);
                    }
                }
                Parent::Grid {
                    columns: columns.len(),
                    rows: rows.len(),
                }
            }
            Some(Layout::Absolute) => Parent::Absolute,
        }
    }

    fn track(&mut self, path: &str, t: &Track) {
        match t {
            Track::Pixels(px) => self.length(path, *px),
            Track::Keyword(k) if k == "content" => {}
            Track::Keyword(k) => {
                let ok = k
                    .strip_suffix("fr")
                    .and_then(|n| n.parse::<u32>().ok())
                    .is_some_and(|n| (1..=99).contains(&n));
                if !ok {
                    self.issue(
                        path,
                        format!("'{k}' is not pixels, 'content' or a fraction like '1fr'"),
                    );
                }
            }
        }
    }

    fn condition(&mut self, path: &str, c: &Condition, scope: Scope, depth: usize) {
        if depth > 8 {
            self.issue(path, "conditions nested deeper than 8 levels");
            return;
        }
        let set = [
            c.sold_out.is_some(),
            c.has_image.is_some(),
            c.has_description.is_some(),
            c.has_tag.is_some(),
            c.has_badge.is_some(),
            c.has_variant.is_some(),
            c.language.is_some(),
            c.not.is_some(),
            c.all.is_some(),
            c.any.is_some(),
        ]
        .into_iter()
        .filter(|s| *s)
        .count();
        if set != 1 {
            self.issue(
                path,
                format!(
                    "a condition needs exactly one key, found {set}; combine with 'all' or 'any'"
                ),
            );
        }
        let needs_item = c.sold_out.is_some() || c.has_badge.is_some() || c.has_variant.is_some();
        let needs_entry =
            c.has_image.is_some() || c.has_description.is_some() || c.has_tag.is_some();
        if needs_item && !scope.item {
            self.issue(
                path,
                "soldOut/hasBadge/hasVariant are only available inside a repeater over items",
            );
        }
        if needs_entry && !scope.item && !scope.category {
            self.issue(
                path,
                "hasImage/hasDescription/hasTag need an enclosing repeater",
            );
        }
        if let Some(v) = &c.has_variant {
            if v.is_empty()
                || v.len() > 32
                || !v
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
            {
                self.issue(path, format!("'{v}' is not a valid variant label"));
            }
        }
        for tag in [&c.has_tag, &c.has_badge].into_iter().flatten() {
            check_text_len(self, path, tag, 1, 40);
        }
        if let Some(lang) = &c.language {
            if !is_language(lang) {
                self.issue(
                    path,
                    format!("'{lang}' is not a language code like 'sv' or 'en-GB'"),
                );
            }
        }
        if let Some(inner) = &c.not {
            self.condition(&format!("{path}.not"), inner, scope, depth + 1);
        }
        for (key, list) in [("all", &c.all), ("any", &c.any)] {
            if let Some(list) = list {
                if list.is_empty() || list.len() > 16 {
                    self.issue(&format!("{path}.{key}"), "needs 1–16 conditions");
                }
                for (i, inner) in list.iter().enumerate() {
                    self.condition(&format!("{path}.{key}[{i}]"), inner, scope, depth + 1);
                }
            }
        }
    }

    fn image_src(&mut self, path: &str, src: &str, scope: Scope) {
        if is_blob_ref(src) {
            return;
        }
        let binding = src
            .strip_prefix("{{")
            .and_then(|s| s.strip_suffix("}}"))
            .map(str::trim);
        match binding {
            Some("item.image") if scope.item => {}
            Some("category.image") if scope.category => {}
            Some(p @ ("item.image" | "category.image")) => self.issue(
                path,
                format!("'{p}' is not available here; it needs an enclosing repeater"),
            ),
            _ => self.issue(
                path,
                "must be blob:<sha256> or {{item.image}} / {{category.image}}",
            ),
        }
    }

    fn style_ref(&mut self, path: &str, style: &StyleRef) {
        let names = style.names();
        if names.is_empty() || names.len() > 8 {
            self.issue(path, "needs 1–8 style names");
        }
        for name in names {
            if !self.template.styles.contains_key(name) {
                let known = self.template.styles.keys().map(String::as_str);
                self.issue(
                    path,
                    format!("unknown style '{name}'{}", did_you_mean(name, known)),
                );
            }
        }
    }

    /// `minSize` must not exceed the resolved font size it shrinks from.
    fn min_size(&mut self, path: &str, min_size: u32, style: Option<&StyleRef>) {
        self.range(path, min_size, 6, 1000);
        let size = style.and_then(|s| {
            s.names()
                .iter()
                .rev()
                .find_map(|n| self.template.styles.get(*n).and_then(|st| st.size))
        });
        if let Some(size) = size {
            if min_size > size {
                self.issue(
                    path,
                    format!("minSize {min_size} is larger than the font size {size}"),
                );
            }
        }
    }

    fn name(&mut self, path: &str, name: &str) {
        let mut chars = name.chars();
        let ok = name.len() <= 32
            && chars.next().is_some_and(|c| c.is_ascii_alphabetic())
            && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
        if !ok {
            self.issue(path, format!("'{name}' must start with a letter and use only letters, digits, '_' or '-' (max 32)"));
        }
    }

    fn color(&mut self, path: &str, color: &str) {
        let ok = color.strip_prefix('#').is_some_and(|hex| {
            (hex.len() == 6 || hex.len() == 8) && hex.chars().all(|c| c.is_ascii_hexdigit())
        });
        if !ok {
            self.issue(
                path,
                format!("'{color}' is not a colour like #rrggbb or #rrggbbaa"),
            );
        }
    }

    fn length(&mut self, path: &str, v: u32) {
        if v > MAX_LENGTH {
            self.issue(path, format!("{v} is larger than {MAX_LENGTH}"));
        }
    }

    fn range(&mut self, path: &str, v: u32, min: u32, max: u32) {
        if !(min..=max).contains(&v) {
            self.issue(path, format!("{v} is outside {min}–{max}"));
        }
    }

    fn unit_interval(&mut self, path: &str, v: f64) {
        if !(0.0..=1.0).contains(&v) {
            self.issue(path, format!("{v} is outside 0–1"));
        }
    }
}

fn check_text_len(c: &mut Checker, path: &str, text: &str, min: usize, max: usize) {
    let n = text.chars().count();
    if n < min || n > max {
        c.issue(
            path,
            format!("length {n} is outside {min}–{max} characters"),
        );
    }
}

fn is_identifier(s: &str) -> bool {
    let mut chars = s.chars();
    s.len() <= 63
        && chars
            .next()
            .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

fn is_blob_ref(s: &str) -> bool {
    s.strip_prefix("blob:").is_some_and(|h| {
        h.len() == 64
            && h.chars()
                .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
    })
}

fn is_language(s: &str) -> bool {
    let (lang, region) = match s.split_once('-') {
        Some((l, r)) => (l, Some(r)),
        None => (s, None),
    };
    (2..=3).contains(&lang.len())
        && lang.chars().all(|c| c.is_ascii_lowercase())
        && region.is_none_or(|r| r.len() == 2 && r.chars().all(|c| c.is_ascii_uppercase()))
}
