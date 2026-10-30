use anyhow::Result;
use std::collections::HashMap;
use std::sync::LazyLock;

use regex::Regex;

use crate::bnf::{Production, parse_bnf};
use crate::clause_tree::{ClauseNode, extract_clause_tree};

#[derive(Clone, Debug)]
pub struct FigureRef {
    /// Printed figure number ("Figure N.") when the image has a caption;
    /// `None` for diagrams the Markdown embeds without a caption line.
    pub number: Option<u32>,
    /// Caption text; empty when the image is uncaptioned.
    pub title: String,
    pub image_path: String,
    pub clause_id: String,
}

#[derive(Clone, Debug)]
pub struct SearchHit<'a> {
    pub clause: &'a ClauseNode,
    /// None for id/title matches; a short excerpt around the first match for
    /// body matches.
    pub snippet: Option<String>,
}

/// Body-only matches are capped so a common word can't flood the result;
/// id/title matches are never dropped.
const BODY_MATCH_LIMIT: usize = 20;

/// Characters of context kept on each side of the first match in a snippet.
const SNIPPET_RADIUS: usize = 100;

static IMAGE_LINK_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"!\[\]\((_page_[^)]+\.jpeg)\)").unwrap());

/// A caption immediately following an image link, e.g. `**Figure 3. Title**`
/// or the heading form `# **Figure 63. Title**` the SysML conversion emits.
/// Anchored so it only matches the text right after the link.
static FIGURE_CAPTION_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\A\s*\n\s*(?:#+\s*)?\*\*Figure (\d+)\.\s*(.+?)\*\*").unwrap());

static SPAN_ANCHOR_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"<span id="([^"]+)"></span>"#).unwrap());

pub struct SpecIndex {
    src: String,
    clauses: Vec<ClauseNode>,
    by_id: HashMap<String, usize>,
    productions: Vec<Production>,
    productions_by_clause: HashMap<String, Vec<usize>>,
    figures: Vec<FigureRef>,
    figures_by_clause: HashMap<String, Vec<usize>>,
    anchor_to_clause: HashMap<String, String>,
    /// Per-clause body text excluding subclauses, aligned with `clauses`.
    own_texts: Vec<String>,
    /// ASCII-lowercased `own_texts` (same byte offsets, so match positions
    /// map 1:1 back to the original text).
    own_texts_lower: Vec<String>,
}

impl SpecIndex {
    pub fn list_clauses(&self) -> &[ClauseNode] {
        &self.clauses
    }

    pub fn clause(&self, id: &str) -> Option<&ClauseNode> {
        self.by_id.get(id).map(|&i| &self.clauses[i])
    }

    pub fn productions(&self) -> &[Production] {
        &self.productions
    }

    pub fn productions_for_clause(&self, clause_id: &str) -> Vec<&Production> {
        self.productions_by_clause
            .get(clause_id)
            .map(|idxs| idxs.iter().map(|&i| &self.productions[i]).collect())
            .unwrap_or_default()
    }

    pub fn body_for_clause(&self, clause_id: &str) -> Option<&str> {
        let clause = self.clause(clause_id)?;
        Some(&self.src[clause.body_start..clause.body_end])
    }

    pub fn figures(&self) -> &[FigureRef] {
        &self.figures
    }

    pub fn figures_for_clause(&self, clause_id: &str) -> Vec<&FigureRef> {
        self.figures_by_clause
            .get(clause_id)
            .map(|idxs| idxs.iter().map(|&i| &self.figures[i]).collect())
            .unwrap_or_default()
    }

    pub fn resolve_anchor(&self, anchor: &str) -> Option<&ClauseNode> {
        if let Some(clause_id) = self.anchor_to_clause.get(anchor) {
            return self.clause(clause_id);
        }
        self.clauses.iter().find(|c| c.anchor == anchor)
    }

    /// Two-tier search: clauses whose id or title contains the query come
    /// first (document order, no snippet), then clauses whose own body text
    /// contains every whitespace-separated query token, ranked by total
    /// occurrence count. Matching is ASCII case-insensitive.
    pub fn search(&self, query: &str) -> Vec<SearchHit<'_>> {
        let q = query.to_ascii_lowercase();
        let tokens: Vec<&str> = q.split_whitespace().collect();
        if tokens.is_empty() {
            return vec![];
        }

        let mut hits = Vec::new();
        let mut body_matches: Vec<(usize, usize)> = Vec::new(); // (score, clause index)
        for (i, c) in self.clauses.iter().enumerate() {
            if c.id.contains(&q) || c.title.to_ascii_lowercase().contains(&q) {
                hits.push(SearchHit {
                    clause: c,
                    snippet: None,
                });
                continue;
            }
            let text = &self.own_texts_lower[i];
            if tokens.iter().all(|t| text.contains(t)) {
                let score = tokens.iter().map(|t| text.matches(t).count()).sum();
                body_matches.push((score, i));
            }
        }

        body_matches.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        body_matches.truncate(BODY_MATCH_LIMIT);
        hits.extend(body_matches.into_iter().map(|(_, i)| SearchHit {
            clause: &self.clauses[i],
            snippet: self.snippet(i, tokens[0]),
        }));
        hits
    }

    fn snippet(&self, clause_idx: usize, token: &str) -> Option<String> {
        let text = &self.own_texts[clause_idx];
        let pos = self.own_texts_lower[clause_idx].find(token)?;

        let mut start = pos.saturating_sub(SNIPPET_RADIUS);
        while !text.is_char_boundary(start) {
            start -= 1;
        }
        let mut end = (pos + token.len() + SNIPPET_RADIUS).min(text.len());
        while !text.is_char_boundary(end) {
            end += 1;
        }

        let mut s = text[start..end]
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        if start > 0 {
            s.insert(0, '…');
        }
        if end < text.len() {
            s.push('…');
        }
        Some(s)
    }
}

pub fn build_index(md: &str, bnf_src: &str) -> Result<SpecIndex> {
    let clauses = extract_clause_tree(md)?;
    let by_id: HashMap<String, usize> = clauses
        .iter()
        .enumerate()
        .map(|(i, c)| (c.id.clone(), i))
        .collect();

    let productions = parse_bnf(bnf_src)?;
    let mut productions_by_clause: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, p) in productions.iter().enumerate() {
        productions_by_clause
            .entry(p.clause_id.clone())
            .or_default()
            .push(i);
    }

    let (figures, figures_by_clause) = extract_figures(md, &clauses);
    let anchor_to_clause = build_anchor_map(md, &clauses);
    let own_texts = extract_own_texts(md, &clauses);
    let own_texts_lower = own_texts.iter().map(|t| t.to_ascii_lowercase()).collect();

    Ok(SpecIndex {
        src: md.to_string(),
        clauses,
        by_id,
        productions,
        productions_by_clause,
        figures,
        figures_by_clause,
        anchor_to_clause,
        own_texts,
        own_texts_lower,
    })
}

/// Body text of each clause minus its subclauses, so a term that appears only
/// in a subclause doesn't also match every ancestor.
fn extract_own_texts(md: &str, clauses: &[ClauseNode]) -> Vec<String> {
    clauses
        .iter()
        .map(|c| md[c.body_start..c.own_end].to_string())
        .collect()
}

fn extract_figures(
    md: &str,
    clauses: &[ClauseNode],
) -> (Vec<FigureRef>, HashMap<String, Vec<usize>>) {
    let mut figures = Vec::new();
    for caps in IMAGE_LINK_RE.captures_iter(md) {
        let whole = caps.get(0).unwrap();
        let image_path = caps.get(1).unwrap().as_str().to_string();
        let (number, title) = match FIGURE_CAPTION_RE.captures(&md[whole.end()..]) {
            Some(caption) => (
                Some(caption[1].parse().unwrap_or(0)),
                caption[2].trim().to_string(),
            ),
            None => (None, String::new()),
        };
        let clause_id = find_clause_for_byte(clauses, whole.start());
        figures.push(FigureRef {
            number,
            title,
            image_path,
            clause_id,
        });
    }

    let mut by_clause: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, f) in figures.iter().enumerate() {
        by_clause.entry(f.clause_id.clone()).or_default().push(i);
    }

    (figures, by_clause)
}

fn find_clause_for_byte(clauses: &[ClauseNode], byte: usize) -> String {
    let mut best = String::new();
    for c in clauses {
        if c.body_start <= byte && byte < c.body_end && (best.is_empty() || c.id.len() > best.len())
        {
            best = c.id.clone();
        }
    }
    best
}

fn build_anchor_map(md: &str, clauses: &[ClauseNode]) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for caps in SPAN_ANCHOR_RE.captures_iter(md) {
        let anchor = caps.get(1).unwrap().as_str().to_string();
        let byte = caps.get(0).unwrap().start();
        let clause_id = find_clause_for_byte(clauses, byte);
        if !clause_id.is_empty() {
            map.insert(anchor, clause_id);
        }
    }
    map
}
