use anyhow::{Context, Result};
use tree_sitter::Node;
use tree_sitter_md::MarkdownParser;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClauseNode {
    pub id: String,
    pub title: String,
    pub anchor: String,
    pub level: u8,
    pub line: usize,
    pub body_start: usize,
    /// End of the clause's full text including subclauses: extends to the
    /// next clause heading at the same or shallower level, or a shallower
    /// non-clause heading. Non-clause headings at the same or deeper level
    /// (e.g. "#### Attributes" following "#### 8.3.3.3.4 Feature") belong to
    /// the clause, so Markdown section boundaries alone are not enough.
    pub body_end: usize,
    /// End of the clause's own text: extends to the next clause heading at
    /// any level (or a shallower non-clause heading), so subclause text is
    /// excluded but trailing non-clause subsections are kept.
    pub own_end: usize,
}

/// Any ATX heading in the document, clause or not, in document order.
struct Heading {
    level: u8,
    line: usize,
    start: usize,
    end: usize,
    /// Clause number if the heading starts one (e.g. "8.3.2"), else None.
    clause_id: Option<String>,
    title: String,
    anchor: String,
}

pub fn extract_clause_tree(src: &str) -> Result<Vec<ClauseNode>> {
    let bytes = src.as_bytes();
    let mut parser = MarkdownParser::default();
    let tree = parser
        .parse(bytes, None)
        .context("tree-sitter-md failed to parse spec")?;
    let root = tree.block_tree().root_node();
    let mut headings = Vec::new();
    collect_headings(&root, bytes, &mut headings);

    let mut out = Vec::new();
    for (i, h) in headings.iter().enumerate() {
        let Some(id) = &h.clause_id else { continue };
        let mut body_end = src.len();
        let mut own_end = None;
        for h2 in &headings[i + 1..] {
            let ends_own = h2.clause_id.is_some() || h2.level < h.level;
            if ends_own && own_end.is_none() {
                own_end = Some(h2.start);
            }
            let ends_subtree = if h2.clause_id.is_some() {
                h2.level <= h.level
            } else {
                h2.level < h.level
            };
            if ends_subtree {
                body_end = h2.start;
                break;
            }
        }
        out.push(ClauseNode {
            id: id.clone(),
            title: h.title.clone(),
            anchor: h.anchor.clone(),
            level: h.level,
            line: h.line,
            body_start: h.end,
            body_end,
            own_end: own_end.unwrap_or(body_end).min(body_end),
        });
    }
    Ok(out)
}

fn collect_headings(node: &Node, src: &[u8], out: &mut Vec<Heading>) {
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i as u32) {
            if child.kind() == "atx_heading" {
                if let Some(h) = parse_heading(&child, src) {
                    out.push(h);
                }
            } else if child.kind() == "section" {
                collect_headings(&child, src, out);
            }
        }
    }
}

fn parse_heading(heading: &Node, src: &[u8]) -> Option<Heading> {
    let heading_text = heading.utf8_text(src).unwrap_or_default();
    let level = heading_level(heading)?;
    let anchor = extract_anchor(heading_text);
    let raw = raw_title(heading_text);
    let clause_id = clause_id_from_title(&raw);
    let title = if clause_id.is_some() {
        strip_clause_number(&raw)
    } else {
        raw
    };
    Some(Heading {
        level,
        line: heading.start_position().row + 1,
        start: heading.start_byte(),
        end: heading.end_byte(),
        clause_id,
        title,
        anchor,
    })
}

fn heading_level(heading: &Node) -> Option<u8> {
    for i in 0..heading.child_count() as u32 {
        if let Some(child) = heading.child(i) {
            let kind = child.kind();
            if let Some(rest) = kind.strip_prefix("atx_h") {
                let digit = rest.split('_').next()?;
                if let Ok(level) = digit.parse::<u8>() {
                    return Some(level);
                }
            }
        }
    }
    None
}

fn extract_anchor(text: &str) -> String {
    text.find("id=\"")
        .and_then(|start| {
            let rest = &text[start + 4..];
            rest.find('"').map(|end| rest[..end].to_string())
        })
        .unwrap_or_default()
}

fn raw_title(text: &str) -> String {
    let no_hash = text.trim_start_matches('#').trim();
    let no_span = strip_span_tag(no_hash);
    no_span.replace("**", "").trim().to_string()
}

fn strip_clause_number(raw: &str) -> String {
    raw.split_whitespace().skip(1).collect::<Vec<_>>().join(" ")
}

fn strip_span_tag(s: &str) -> String {
    let start = s.find("<span ");
    let end = s.find("</span>");
    match (start, end) {
        (Some(st), Some(en)) if st < en => {
            let after = &s[en + "</span>".len()..];
            format!("{}{}", &s[..st], after)
        }
        _ => s.to_string(),
    }
}

fn clause_id_from_title(title: &str) -> Option<String> {
    let first_token = title.split_whitespace().next()?;
    let trimmed = first_token.trim_end_matches('.');
    if !trimmed.chars().all(|c| c.is_ascii_digit() || c == '.') {
        return None;
    }
    if trimmed.contains('.') || trimmed.chars().all(|c| c.is_ascii_digit()) {
        Some(trimmed.to_string())
    } else {
        None
    }
}
