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
    pub body_end: usize,
}

pub fn extract_clause_tree(src: &str) -> Result<Vec<ClauseNode>> {
    let bytes = src.as_bytes();
    let mut parser = MarkdownParser::default();
    let tree = parser
        .parse(bytes, None)
        .context("tree-sitter-md failed to parse spec")?;
    let root = tree.block_tree().root_node();
    let mut out = Vec::new();
    collect_sections(&root, bytes, &mut out);
    Ok(out)
}

fn collect_sections(node: &Node, src: &[u8], out: &mut Vec<ClauseNode>) {
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i as u32) {
            if child.kind() == "section" {
                if let Some(clause) = parse_section(&child, src) {
                    out.push(clause);
                }
                collect_sections(&child, src, out);
            }
        }
    }
}

fn parse_section(section: &Node, src: &[u8]) -> Option<ClauseNode> {
    let mut i = 0u32;
    while i < section.child_count() as u32 {
        if let Some(child) = section.child(i) {
            if child.kind() == "atx_heading" {
                let heading_text = child.utf8_text(src).unwrap_or_default();
                let level = heading_level(&child)?;
                let anchor = extract_anchor(heading_text);
                let raw = raw_title(heading_text);
                let id = clause_id_from_title(&raw)?;
                let title = strip_clause_number(&raw);
                let line = child.start_position().row + 1;
                let body_start = child.end_byte();
                let body_end = section.end_byte();
                return Some(ClauseNode {
                    id,
                    title,
                    anchor,
                    level,
                    line,
                    body_start,
                    body_end,
                });
            }
        }
        i += 1;
    }
    None
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
