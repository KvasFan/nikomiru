// SPDX-License-Identifier: GPL-3.0-only

//! The part of a dictionary's `styles.css` that Nikomiru applies (DIC-035):
//! rules whose selectors are compounds of a tag, `gloss-sc-<tag>` classes and
//! `data-sc-*` attributes (what Yomitan dictionaries use to style their
//! structured content), joined by descendant (` `) or child (`>`)
//! combinators, also when written as nested rules (`&`), with the properties
//! of [`Style`]. Sibling combinators, pseudo-classes, pseudo-elements,
//! at-rules and other properties are ignored. The user's CSS goes through the
//! same subset; [`Sheet::check`] tells its editor what is kept.

use crate::style::Style;

/// Nested rules deeper than this are ignored.
const MAX_NESTING: usize = 16;

/// One compound selector: `span.gloss-sc-span[data-sc-class="tag"]`.
#[derive(Debug, Clone, PartialEq)]
struct Compound {
    tag: Option<String>,
    /// `data-sc-*` attributes (name without the prefix, required value);
    /// `\0tag:<t>` for a `gloss-sc-<t>` class.
    attrs: Vec<(String, Option<String>)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Combinator {
    /// `a b`: any ancestor.
    Descendant,
    /// `a > b`: the parent.
    Child,
}

/// A selector: compounds left to right, `combinators[i]` between `parts[i]`
/// and `parts[i + 1]`.
#[derive(Debug, Clone, PartialEq)]
struct Selector {
    parts: Vec<Compound>,
    combinators: Vec<Combinator>,
}

/// One selector and its declarations.
#[derive(Debug, Clone, PartialEq)]
struct Rule {
    selector: Selector,
    /// (attributes and classes, tags), as CSS specificity.
    specificity: (usize, usize),
    order: usize,
    style: Style,
}

/// The applicable rules of a stylesheet.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Sheet {
    rules: Vec<Rule>,
}

/// What [`Sheet::parse`] keeps of a stylesheet and what it leaves out: the
/// feedback of the editor of the user's CSS (DIC-035).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Report {
    /// Rules that apply: a selector of the subset with at least one property
    /// kept, counted once per selector of a list.
    pub rules: usize,
    /// The properties left out of the rules that apply, in order: outside the
    /// subset, or with a value it refuses.
    pub dropped: Vec<String>,
    /// The rules ignored whole, in order: their selector is outside the
    /// subset (`a:hover`, `span + span`, `#id`), or they are at-rules.
    pub ignored: Vec<String>,
}

/// Report entries longer than this are cut: they are for reading.
const REPORT_TEXT: usize = 48;

fn report_text(text: &str) -> String {
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    match text.char_indices().nth(REPORT_TEXT) {
        Some((cut, _)) => format!("{}…", &text[..cut]),
        None => text,
    }
}

/// An element as a selector sees it: its tag and its `data` entries.
pub type Element<'a> = (&'a str, &'a [(String, String)]);

fn strip_comments(css: &str) -> String {
    let mut out = String::with_capacity(css.len());
    let mut rest = css;
    while let Some(start) = rest.find("/*") {
        out.push_str(&rest[..start]);
        match rest[start + 2..].find("*/") {
            Some(end) => rest = &rest[start + 2 + end + 2..],
            None => return out,
        }
    }
    out.push_str(rest);
    out
}

fn is_ident(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
}

fn compound(text: &str) -> Option<Compound> {
    if text.is_empty() || text.contains([':', '*', '#', '+', '~']) {
        return None;
    }
    let end = text.find(['.', '[']).unwrap_or(text.len());
    let tag = &text[..end];
    if !tag.is_empty() && !is_ident(tag) {
        return None;
    }
    let tag = (!tag.is_empty()).then(|| tag.to_ascii_lowercase());
    let mut attrs = Vec::new();
    let mut rest = &text[end..];
    while !rest.is_empty() {
        if let Some(after) = rest.strip_prefix('.') {
            let len = after.find(['.', '[']).unwrap_or(after.len());
            // structured content nodes only carry `gloss-sc-<their tag>`
            let of = after[..len].strip_prefix("gloss-sc-")?;
            if tag.as_deref().is_some_and(|t| t != of) {
                return None;
            }
            attrs.push((format!("\0tag:{of}"), None));
            rest = &after[len..];
        } else {
            let after = rest.strip_prefix('[')?;
            let close = after.find(']')?;
            let inner = &after[..close];
            let (name, value) = match inner.split_once('=') {
                Some((n, v)) => {
                    let v = v.trim();
                    let unquoted = v
                        .strip_prefix('"')
                        .and_then(|v| v.strip_suffix('"'))
                        .or_else(|| v.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')))
                        .unwrap_or(v);
                    (n.trim(), Some(unquoted.to_string()))
                }
                None => (inner.trim(), None),
            };
            // `~=`, `^=` and friends are not supported
            if name.ends_with(['~', '^', '$', '*', '|']) {
                return None;
            }
            let key = name
                .strip_prefix("data-sc-")
                .or_else(|| name.strip_prefix("data-sc"))?;
            attrs.push((key.to_string(), value));
            rest = &after[close + 1..];
        }
    }
    Some(Compound { tag, attrs })
}

/// A full selector such as `li[data-sc-content="sense"] > ul`.
fn selector(text: &str) -> Option<Selector> {
    // split on combinators outside `[…]`
    let mut tokens: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut in_brackets = false;
    for c in text.trim().chars() {
        match c {
            '[' => {
                in_brackets = true;
                current.push(c);
            }
            ']' => {
                in_brackets = false;
                current.push(c);
            }
            '>' if !in_brackets => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
                tokens.push(">".into());
            }
            c if c.is_whitespace() && !in_brackets => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
            }
            c => current.push(c),
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    let mut parts = Vec::new();
    let mut combinators = Vec::new();
    let mut pending = None;
    for t in tokens {
        if t == ">" {
            if parts.is_empty() || pending.is_some() {
                return None;
            }
            pending = Some(Combinator::Child);
            continue;
        }
        if !parts.is_empty() {
            combinators.push(pending.take().unwrap_or(Combinator::Descendant));
        }
        parts.push(compound(&t)?);
    }
    (!parts.is_empty() && pending.is_none()).then_some(Selector { parts, combinators })
}

fn specificity(s: &Selector) -> (usize, usize) {
    s.parts.iter().fold((0, 0), |(a, t), p| {
        (a + p.attrs.len(), t + usize::from(p.tag.is_some()))
    })
}

/// The style of a block's declarations; the properties it leaves out go to
/// `dropped`.
fn declarations(block: &str, mut dropped: Option<&mut Vec<String>>) -> Style {
    let mut style = Style::default();
    for decl in block.split(';').map(str::trim).filter(|d| !d.is_empty()) {
        let (property, kept) = match decl.split_once(':') {
            Some((property, value)) => (property.trim(), style.set(property.trim(), value.trim())),
            None => (decl, false),
        };
        if !kept && let Some(dropped) = dropped.as_deref_mut() {
            dropped.push(report_text(property));
        }
    }
    style
}

/// The child selectors of a nested rule inside `parents` (`&` is the parent;
/// without it the child is a descendant).
fn nested(prelude: &str, parents: &[String]) -> Vec<String> {
    let children: Vec<&str> = prelude
        .split(',')
        .map(str::trim)
        .filter(|c| !c.is_empty())
        .collect();
    if parents.is_empty() {
        return children.into_iter().map(String::from).collect();
    }
    let mut out = Vec::new();
    for p in parents {
        for c in &children {
            out.push(if c.contains('&') {
                c.replace('&', p)
            } else {
                format!("{p} {c}")
            });
        }
    }
    out
}

/// Reads rules and declarations at one level; `parents` are the selectors of
/// the enclosing rule (none at the top). What it leaves out goes to `report`.
fn parse_level(
    css: &str,
    parents: &[String],
    depth: usize,
    order: &mut usize,
    rules: &mut Vec<Rule>,
    mut report: Option<&mut Report>,
) {
    let mut decls = String::new();
    let mut start = 0;
    let bytes = css.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b';' => {
                decls.push_str(&css[start..=i]);
                start = i + 1;
            }
            b'{' => {
                // the block, braces balanced
                let mut level = 0usize;
                let mut close = None;
                for (j, &b) in bytes.iter().enumerate().skip(i) {
                    match b {
                        b'{' => level += 1,
                        b'}' => {
                            level -= 1;
                            if level == 0 {
                                close = Some(j);
                                break;
                            }
                        }
                        _ => {}
                    }
                }
                let prelude = css[start..i].trim();
                let Some(close) = close else {
                    if let Some(report) = report {
                        report.ignored.push(format!("{} {{", report_text(prelude)));
                    }
                    return;
                };
                let body = &css[i + 1..close];
                if !prelude.starts_with('@') && !prelude.is_empty() && depth < MAX_NESTING {
                    let selectors = nested(prelude, parents);
                    parse_level(
                        body,
                        &selectors,
                        depth + 1,
                        order,
                        rules,
                        report.as_deref_mut(),
                    );
                } else if !prelude.is_empty()
                    && let Some(report) = report.as_deref_mut()
                {
                    report.ignored.push(report_text(prelude));
                }
                i = close + 1;
                start = i;
                continue;
            }
            b'}' => {
                // a stray brace: skip it
                start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    decls.push_str(css.get(start..).unwrap_or_default());
    if parents.is_empty() {
        // statements outside every rule: `@import` and the like, or
        // declarations that belong to nothing
        if let Some(report) = report {
            for statement in decls.split(';').map(str::trim).filter(|s| !s.is_empty()) {
                if statement.starts_with('@') {
                    report.ignored.push(report_text(statement));
                } else {
                    let property = statement.split(':').next().unwrap_or(statement);
                    report.dropped.push(report_text(property));
                }
            }
        }
        return;
    }
    let mut dropped = Vec::new();
    let style = declarations(&decls, report.is_some().then_some(&mut dropped));
    // nothing to apply; and nothing to report when the block only holds
    // nested rules
    if style.is_empty() && (report.is_none() || dropped.is_empty()) {
        return;
    }
    let mut applies = false;
    for text in parents {
        match selector(text) {
            Some(selector) => {
                applies = true;
                if !style.is_empty() {
                    rules.push(Rule {
                        specificity: specificity(&selector),
                        selector,
                        order: *order,
                        style: style.clone(),
                    });
                    *order += 1;
                }
            }
            None => {
                if let Some(report) = report.as_deref_mut() {
                    report.ignored.push(report_text(text));
                }
            }
        }
    }
    // what a rule that applies to nothing leaves out does not matter
    if applies && let Some(report) = report {
        report.dropped.append(&mut dropped);
    }
}

fn matches(c: &Compound, (tag, data): Element<'_>) -> bool {
    c.tag.as_deref().is_none_or(|t| t == tag)
        && c.attrs
            .iter()
            .all(|(name, value)| match name.strip_prefix("\0tag:") {
                Some(of) => of == tag,
                None => data
                    .iter()
                    .any(|(k, v)| kebab(k) == *name && value.as_ref().is_none_or(|want| want == v)),
            })
}

/// Whether `parts[..=i]` matches the path up to `at`, `parts[i]` on `path[at]`.
fn matches_from(s: &Selector, i: usize, path: &[Element<'_>], at: usize) -> bool {
    if !matches(&s.parts[i], path[at]) {
        return false;
    }
    if i == 0 {
        return true;
    }
    match s.combinators[i - 1] {
        Combinator::Child => at > 0 && matches_from(s, i - 1, path, at - 1),
        Combinator::Descendant => (0..at).rev().any(|a| matches_from(s, i - 1, path, a)),
    }
}

impl Sheet {
    /// The applicable part of a stylesheet. Never fails: what cannot be read
    /// is skipped.
    pub fn parse(css: &str) -> Self {
        Self::read(css, None)
    }

    /// What [`Sheet::parse`] keeps of `css` and what it leaves out.
    pub fn check(css: &str) -> Report {
        let mut report = Report::default();
        report.rules = Self::read(css, Some(&mut report)).rules.len();
        report
    }

    fn read(css: &str, report: Option<&mut Report>) -> Self {
        let css = strip_comments(css);
        let mut rules = Vec::new();
        let mut order = 0;
        parse_level(&css, &[], 0, &mut order, &mut rules, report);
        rules.sort_by_key(|r| (r.specificity, r.order));
        Self { rules }
    }

    /// Whether no rule applies to anything.
    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    /// The style of the last element of `path` (the root first).
    pub fn style_for(&self, path: &[Element<'_>]) -> Style {
        let mut out = Style::default();
        let Some(last) = path.len().checked_sub(1) else {
            return out;
        };
        for r in &self.rules {
            if matches_from(&r.selector, r.selector.parts.len() - 1, path, last) {
                out.merge(&r.style);
            }
        }
        out
    }
}

/// `dataKey` → `data-key`, as the browser names `data` attributes.
pub fn kebab(key: &str) -> String {
    let mut out = String::with_capacity(key.len() + 2);
    for c in key.chars() {
        if c.is_ascii_uppercase() {
            out.push('-');
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::{ListMarker, color};

    fn data(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    // T-DIC-035
    #[test]
    fn simple_selectors_apply_by_specificity() {
        let sheet = Sheet::parse(
            r#"
            /* tags */
            span[data-sc-class="tag"] { font-weight: bold; color: #fff; background-color: #565656; border-radius: 0.3em; }
            [data-sc-class] { color: red; font-style: italic }
            .gloss-sc-li { margin-left: 1em }
            a:hover { color: green }
            @media (prefers-color-scheme: dark) { span { color: white } }
            ul[data-sc-content='glossary'] { list-style-type: "◦" }
            span + span { color: pink }
            "#,
        );
        let tag_data = data(&[("class", "tag")]);
        let tag = sheet.style_for(&[("span", &tag_data)]);
        assert_eq!(tag.color, color("#fff"), "the more specific rule wins");
        assert_eq!((tag.bold, tag.italic), (Some(true), Some(true)));
        let x = data(&[("class", "x")]);
        assert_eq!(sheet.style_for(&[("div", &x)]).color, color("red"));
        assert!(
            sheet.style_for(&[("span", &[])]).is_empty(),
            "pseudo-classes, siblings and @media are skipped"
        );
        assert!(sheet.style_for(&[("li", &[])]).margin.left.is_some());
        let g = data(&[("content", "glossary")]);
        assert_eq!(
            sheet.style_for(&[("ul", &g)]).list_marker,
            Some(ListMarker::Text("◦".into()))
        );
    }

    // T-DIC-035
    #[test]
    fn nesting_descendants_and_children() {
        // the shapes Jitendex uses
        let sheet = Sheet::parse(
            r#"
            ul[data-sc-content="glossary"] { list-style-type: disc; }
            li[data-sc-content="sense"] {
                padding-left: 0.25em;
                & ul[data-sc-content="glossary"] { list-style-type: none; }
            }
            div[data-sc-content="xref"] {
                border-color: #1A73E8;
                & span[data-sc-content="reference-label"] { color: #1A73E8; }
            }
            div > span { font-weight: bold }
            td[data-sc-class="form-pri"] > span { color: white; &::before { content: "△"; } }
            "#,
        );
        let sense = data(&[("content", "sense")]);
        let gloss = data(&[("content", "glossary")]);
        let outside = sheet.style_for(&[("ul", &gloss)]);
        assert_eq!(outside.list_marker, Some(ListMarker::Disc));
        let inside = sheet.style_for(&[("ol", &[]), ("li", &sense), ("div", &[]), ("ul", &gloss)]);
        assert_eq!(
            inside.list_marker,
            Some(ListMarker::None),
            "the nested rule is more specific"
        );
        assert!(sheet.style_for(&[("li", &sense)]).padding.left.is_some());

        let xref = data(&[("content", "xref")]);
        let label = data(&[("content", "reference-label")]);
        let in_xref = sheet.style_for(&[("div", &xref), ("div", &[]), ("span", &label)]);
        assert_eq!(in_xref.color, color("#1a73e8"));
        assert!(sheet.style_for(&[("span", &label)]).color.is_none());

        assert_eq!(
            sheet.style_for(&[("div", &[]), ("span", &[])]).bold,
            Some(true)
        );
        assert_eq!(
            sheet
                .style_for(&[("div", &[]), ("li", &[]), ("span", &[])])
                .bold,
            None,
            "> is the parent only"
        );
        let pri = data(&[("class", "form-pri")]);
        assert_eq!(
            sheet.style_for(&[("td", &pri), ("span", &[])]).color,
            color("white")
        );
    }

    // T-DIC-035 (the editor's feedback)
    #[test]
    fn the_check_tells_what_applies_and_what_is_left_out() {
        let report = Sheet::check(
            r#"
            span[data-sc-class="tag"] { color: red; position: absolute; font-family: serif }
            div, a:hover { margin: 1em }
            li { & > ul { list-style-type: none } }
            @media (min-width: 1px) { span { color: blue } }
            @import url(http://evil/x.css);
            color: green;
            span + span { color: pink }
            ul { cursor: pointer }
            "#,
        );
        assert_eq!(report.rules, 3, "{report:?}");
        assert_eq!(
            report.dropped,
            ["position", "font-family", "cursor", "color"]
        );
        assert_eq!(
            report.ignored,
            [
                "a:hover",
                "@media (min-width: 1px)",
                "span + span",
                "@import url(http://evil/x.css)"
            ]
        );
        // what the check counts is what the lookup applies
        let sheet = Sheet::parse("div, a:hover { margin: 1em } ul { cursor: pointer }");
        assert_eq!(sheet.rules.len(), 1);
        let unclosed = Sheet::check("span { color: red");
        assert_eq!(
            (unclosed.rules, &unclosed.ignored[..]),
            (0, &["span {".to_string()][..])
        );
        let long = Sheet::check(&format!("#{} {{ color: red }}", "x".repeat(200)));
        assert_eq!(
            long.ignored[0].chars().count(),
            REPORT_TEXT + 1,
            "cut, with …"
        );
        assert_eq!(Sheet::check(""), Report::default());
    }

    #[test]
    fn hostile_css_is_harmless() {
        let sheet = Sheet::parse(
            "@import url(http://evil/x.css); span { background: url(http://evil/x.png); behavior: url(x) } div { color: expression(alert(1)) } }}} {",
        );
        assert!(sheet.is_empty());
        assert_eq!(kebab("subsenseNumber"), "subsense-number");
        // a deep nest neither overflows nor takes long
        let deep = "a {".repeat(5000) + &"}".repeat(5000);
        let _ = Sheet::parse(&deep);
    }
}
