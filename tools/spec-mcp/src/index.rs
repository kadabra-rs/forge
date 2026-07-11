use anyhow::Result;
use std::collections::HashMap;
use std::sync::LazyLock;

use regex::Regex;

use crate::bnf::{Production, parse_bnf};
use crate::clause_tree::{ClauseNode, extract_clause_tree};

#[derive(Clone, Debug)]
pub struct FigureRef {
    pub number: u32,
    pub title: String,
    pub image_path: String,
    pub clause_id: String,
}

static FIGURE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"!\[\]\((_page_[^)]+\.jpeg)\)\s*\n\s*\*\*Figure (\d+)\.\s*(.+?)\*\*").unwrap()
});

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

    Ok(SpecIndex {
        src: md.to_string(),
        clauses,
        by_id,
        productions,
        productions_by_clause,
        figures,
        figures_by_clause,
        anchor_to_clause,
    })
}

fn extract_figures(
    md: &str,
    clauses: &[ClauseNode],
) -> (Vec<FigureRef>, HashMap<String, Vec<usize>>) {
    let mut figures = Vec::new();
    for caps in FIGURE_RE.captures_iter(md) {
        let image_path = caps.get(1).unwrap().as_str().to_string();
        let number: u32 = caps.get(2).unwrap().as_str().parse().unwrap_or(0);
        let title = caps.get(3).unwrap().as_str().trim().to_string();
        let match_start = caps.get(0).unwrap().start();
        let clause_id = find_clause_for_byte(clauses, match_start);
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
