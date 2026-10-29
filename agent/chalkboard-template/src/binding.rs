// SPDX-License-Identifier: MIT OR Apache-2.0

//! The `{{path | filter:arg}}` binding syntax used in text (TPL-004).
//!
//! Deliberately logic-free: a binding is a dotted path into the data model
//! followed by zero or more filters from a fixed list. Conditions live in
//! `conditional` nodes, never in strings, so the renderer (C) and the
//! preview (TypeScript) can implement it identically.

/// Data available to a node, determined by the repeaters around it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Scope {
    pub category: bool,
    pub item: bool,
}

/// One `{{ … }}` expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Binding {
    pub path: String,
    pub filters: Vec<Filter>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Filter {
    Price,
    Secondary,
    Upper,
    Lower,
    Truncate(u32),
    Default(String),
}

/// Paths that can be printed, per scope.
const GLOBAL_PATHS: &[&str] = &["page.title", "page.language", "screen.name"];
const CATEGORY_PATHS: &[&str] = &["category.title", "category.description"];
const ITEM_PATHS: &[&str] = &["item.name", "item.description", "item.price"];
const FILTERS: &[&str] = &[
    "price",
    "secondary",
    "upper",
    "lower",
    "truncate",
    "default",
];

/// Extracts every binding from `text`. Text outside `{{ }}` is literal.
pub fn parse(text: &str) -> Result<Vec<Binding>, String> {
    let mut bindings = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("{{") {
        let after = &rest[start + 2..];
        let end = after
            .find("}}")
            .ok_or_else(|| "unterminated '{{' (missing '}}')".to_string())?;
        bindings.push(parse_expression(&after[..end])?);
        rest = &after[end + 2..];
    }
    if rest.contains("}}") {
        return Err("'}}' without a matching '{{'".to_string());
    }
    Ok(bindings)
}

fn parse_expression(expr: &str) -> Result<Binding, String> {
    let mut parts = expr.split('|');
    let path = parts.next().unwrap_or_default().trim();
    if path.is_empty() {
        return Err("empty binding '{{}}'".to_string());
    }
    if !path.split('.').all(|seg| {
        !seg.is_empty()
            && seg
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    }) {
        return Err(format!("invalid binding path '{path}'"));
    }
    let filters = parts.map(parse_filter).collect::<Result<_, _>>()?;
    Ok(Binding {
        path: path.to_string(),
        filters,
    })
}

fn parse_filter(raw: &str) -> Result<Filter, String> {
    let raw = raw.trim();
    let (name, arg) = match raw.split_once(':') {
        Some((name, arg)) => (name.trim(), Some(arg.trim())),
        None => (raw, None),
    };
    let no_arg = |filter: Filter| match arg {
        None => Ok(filter),
        Some(_) => Err(format!("filter '{name}' takes no argument")),
    };
    match name {
        "price" => no_arg(Filter::Price),
        "secondary" => no_arg(Filter::Secondary),
        "upper" => no_arg(Filter::Upper),
        "lower" => no_arg(Filter::Lower),
        "truncate" => {
            let n: u32 = arg
                .and_then(|a| a.parse().ok())
                .filter(|n| (1..=500).contains(n))
                .ok_or_else(|| {
                    "filter 'truncate' needs a length from 1 to 500, e.g. truncate:40".to_string()
                })?;
            Ok(Filter::Truncate(n))
        }
        "default" => {
            let text = arg
                .and_then(|a| a.strip_prefix('"').and_then(|a| a.strip_suffix('"')))
                .ok_or_else(|| {
                    "filter 'default' needs quoted text, e.g. default:\"-\"".to_string()
                })?;
            Ok(Filter::Default(text.to_string()))
        }
        "" => Err("empty filter after '|'".to_string()),
        other => Err(format!(
            "unknown filter '{other}'{}",
            did_you_mean(other, FILTERS.iter().copied())
        )),
    }
}

/// Whether `path` is a price (`item.price` or `item.prices.<variant>`).
pub fn is_price_path(path: &str) -> bool {
    path == "item.price"
        || path
            .strip_prefix("item.prices.")
            .is_some_and(|v| !v.is_empty() && !v.contains('.'))
}

/// Checks that `binding` can be resolved in `scope` and its filters fit it.
pub fn check(binding: &Binding, scope: Scope) -> Result<(), String> {
    let path = binding.path.as_str();
    let known = GLOBAL_PATHS.contains(&path)
        || (scope.category && CATEGORY_PATHS.contains(&path))
        || (scope.item && (ITEM_PATHS.contains(&path) || is_price_path(path)));
    if !known {
        let root = path.split('.').next().unwrap_or_default();
        if root == "item" && !scope.item {
            return Err(format!(
                "'{path}' is only available inside a repeater over items"
            ));
        }
        if root == "category" && !scope.category {
            return Err(format!(
                "'{path}' is only available inside a repeater over categories"
            ));
        }
        let candidates = GLOBAL_PATHS
            .iter()
            .chain(if scope.category { CATEGORY_PATHS } else { &[] })
            .chain(if scope.item { ITEM_PATHS } else { &[] })
            .copied();
        return Err(format!(
            "unknown binding '{path}'{}",
            did_you_mean(path, candidates)
        ));
    }
    for filter in &binding.filters {
        if matches!(filter, Filter::Price | Filter::Secondary) && !is_price_path(path) {
            return Err(format!(
                "filter 'price'/'secondary' only applies to prices, not '{path}'"
            ));
        }
    }
    Ok(())
}

/// Suggests the closest candidate if it is plausibly a typo.
pub fn did_you_mean<'a>(word: &str, candidates: impl Iterator<Item = &'a str>) -> String {
    candidates
        .map(|c| (levenshtein(word, c), c))
        .filter(|(d, c)| *d <= 2.max(c.len() / 4))
        .min()
        .map(|(_, c)| format!(" (did you mean '{c}'?)"))
        .unwrap_or_default()
}

fn levenshtein(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut cur = vec![i + 1; b.len() + 1];
        for (j, cb) in b.iter().enumerate() {
            cur[j + 1] = (prev[j] + usize::from(ca != *cb))
                .min(prev[j + 1] + 1)
                .min(cur[j] + 1);
        }
        prev = cur;
    }
    prev[b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    const ITEM: Scope = Scope {
        category: false,
        item: true,
    };

    #[test]
    fn parses_paths_and_filters() {
        let b =
            parse("from {{ item.prices.L | price }} – {{item.name|upper|truncate:20}}").unwrap();
        assert_eq!(b.len(), 2);
        assert_eq!(b[0].path, "item.prices.L");
        assert_eq!(b[0].filters, vec![Filter::Price]);
        assert_eq!(b[1].filters, vec![Filter::Upper, Filter::Truncate(20)]);
    }

    #[test]
    fn literal_text_without_bindings_is_fine() {
        assert!(parse("Vegan").unwrap().is_empty());
    }

    #[test]
    fn default_filter_takes_quoted_text() {
        let b = parse("{{item.description | default:\"Ask staff\"}}").unwrap();
        assert_eq!(b[0].filters, vec![Filter::Default("Ask staff".into())]);
        assert!(parse("{{item.description | default:Ask}}").is_err());
    }

    #[test]
    fn rejects_malformed_bindings() {
        for bad in [
            "{{item.name",
            "item.name}}",
            "{{}}",
            "{{ item..name }}",
            "{{item.name | }}",
            "{{item.name | trunc}}",
        ] {
            assert!(parse(bad).is_err(), "{bad} should fail");
        }
    }

    #[test]
    fn suggests_close_paths_and_filters() {
        let err = check(&parse("{{item.nmae}}").unwrap()[0], ITEM).unwrap_err();
        assert!(err.contains("did you mean 'item.name'"), "{err}");
        let err = parse("{{item.name | uper}}").unwrap_err();
        assert!(err.contains("did you mean 'upper'"), "{err}");
    }

    #[test]
    fn scope_rules() {
        let item = &parse("{{item.name}}").unwrap()[0];
        assert!(
            check(item, Scope::default())
                .unwrap_err()
                .contains("repeater over items")
        );
        assert!(check(item, ITEM).is_ok());
        let price = &parse("{{item.name | price}}").unwrap()[0];
        assert!(
            check(price, ITEM)
                .unwrap_err()
                .contains("only applies to prices")
        );
        assert!(check(&parse("{{item.prices.S | secondary}}").unwrap()[0], ITEM).is_ok());
    }
}
