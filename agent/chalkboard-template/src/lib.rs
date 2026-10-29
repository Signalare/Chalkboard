// SPDX-License-Identifier: MIT OR Apache-2.0

//! Chalkboard template format v1 (SPEC §8): parsing and validation.
//!
//! The structure is defined by `schema/template.v1.json`; this crate parses
//! it with exact error locations and adds the semantic checks a schema
//! cannot express. The agent validates every template on save and on
//! receipt from a peer (TPL-010), so an invalid template never reaches a
//! snapshot.

pub mod binding;
pub mod model;
pub mod validate;

use std::fmt;

pub use model::Template;

/// Largest template document accepted, in bytes.
pub const MAX_DOCUMENT_BYTES: usize = 1 << 20;

/// One problem in a template, located by a path such as
/// `root.template.children[2].text`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Issue {
    pub path: String,
    pub message: String,
}

impl fmt::Display for Issue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.path.is_empty() {
            write!(f, "{}", self.message)
        } else {
            write!(f, "{}: {}", self.path, self.message)
        }
    }
}

/// Parses and validates a template. Returns every problem found; structural
/// errors stop parsing at the first one, semantic errors are all reported.
pub fn validate(document: &[u8]) -> Result<Template, Vec<Issue>> {
    if document.len() > MAX_DOCUMENT_BYTES {
        return Err(vec![Issue {
            path: String::new(),
            message: format!(
                "document is {} bytes; at most {MAX_DOCUMENT_BYTES} allowed",
                document.len()
            ),
        }]);
    }
    let mut de = serde_json::Deserializer::from_slice(document);
    let template: Template = serde_path_to_error::deserialize(&mut de).map_err(|e| {
        let issue = Issue {
            path: tidy_path(&e.path().to_string()),
            message: e.inner().to_string(),
        };
        vec![locate_node_error(document).unwrap_or(issue)]
    })?;
    de.end().map_err(|e| {
        vec![Issue {
            path: String::new(),
            message: e.to_string(),
        }]
    })?;
    let issues = validate::check(&template);
    if issues.is_empty() {
        Ok(template)
    } else {
        Err(issues)
    }
}

/// Nodes are an internally tagged enum, which serde buffers, so a parse
/// error inside a node is reported at the outermost node. This re-parses the
/// node tree one node at a time and returns the deepest node that fails.
fn locate_node_error(document: &[u8]) -> Option<Issue> {
    let value: serde_json::Value = serde_json::from_slice(document).ok()?;
    find_failing_node(value.get("root")?, "root")
}

fn find_failing_node(value: &serde_json::Value, path: &str) -> Option<Issue> {
    let children = |key: &str| {
        value
            .get(key)
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default()
    };
    for (i, child) in children("children").iter().enumerate() {
        if let Some(issue) = find_failing_node(child, &format!("{path}.children[{i}]")) {
            return Some(issue);
        }
    }
    for key in ["template", "empty", "then", "else"] {
        if let Some(child) = value.get(key).filter(|v| v.is_object()) {
            if let Some(issue) = find_failing_node(child, &format!("{path}.{key}")) {
                return Some(issue);
            }
        }
    }
    match serde_path_to_error::deserialize::<_, model::Node>(value) {
        Ok(_) => None,
        Err(e) => {
            let inner = tidy_path(&e.path().to_string());
            Some(Issue {
                path: if inner.is_empty() {
                    path.to_string()
                } else {
                    format!("{path}.{inner}")
                },
                message: e.inner().to_string(),
            })
        }
    }
}

/// serde_path_to_error renders the root as "." and unknown positions as "?".
fn tidy_path(path: &str) -> String {
    match path {
        "." => String::new(),
        p => p.to_string(),
    }
}
