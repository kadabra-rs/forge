use anyhow::{Result, bail};
use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use regex::{Regex, RegexBuilder};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::bnf::{Production, parse_bnf};
use crate::clause_tree::{ClauseNode, extract_clause_tree};
use crate::symbols::SymbolIndex;

/// How `SpecIndex::search` interprets the query string.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum SearchMode {
    /// Case-insensitive raw substrings; every whitespace-separated token must
    /// match.
    #[default]
    Substring,
    /// Whole-word matches only.
    Word,
    /// The query is one case-insensitive Rust regex.
    Regex,
}

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
    /// None for id/title matches; an excerpt around the first match for body
    /// matches, or the caption line (`Figure N. Title`) for figure matches.
    pub snippet: Option<String>,
    /// Printed figure number when the hit came from the figure-caption tier.
    pub figure: Option<u32>,
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
    /// Grammar cross-reference index (definitions, references, keywords).
    symbols: SymbolIndex,
}

impl SpecIndex {
    pub fn list_clauses(&self) -> &[ClauseNode] {
        &self.clauses
    }

    pub fn clause(&self, id: &str) -> Option<&ClauseNode> {
        self.by_id.get(id).map(|&i| &self.clauses[i])
    }

    /// Ancestor chain root-first (chapter → … → parent), exclusive of the
    /// clause itself. Derived from the dotted clause id: every proper
    /// numeric prefix of the id that exists in the document is an ancestor
    /// (heading levels are NOT reliable in the OMG sources: a child can sit
    /// at the same level as its parent). Missing levels are skipped
    /// (numbering can jump). Empty for top-level clauses / unknown ids.
    pub fn ancestors(&self, clause_id: &str) -> Vec<&ClauseNode> {
        if !clause_id.contains('.') {
            return Vec::new();
        }
        let mut chain = Vec::new();
        for prefix in dotted_prefixes(clause_id) {
            if let Some(&i) = self.by_id.get(&prefix) {
                chain.push(&self.clauses[i]);
            }
        }
        chain
    }

    /// Index of the clause's parent: the deepest existing proper dotted
    /// prefix of the id (heading levels are not reliable — a child can sit
    /// at the same level as its parent). `None` when no proper prefix
    /// exists, i.e. the clause is a root.
    pub fn parent_index(&self, clause_id: &str) -> Option<usize> {
        let mut parent = None;
        for prefix in dotted_prefixes(clause_id) {
            if let Some(&i) = self.by_id.get(&prefix) {
                parent = Some(i);
            }
        }
        parent
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

    pub fn figures(&self) -> &[FigureRef] {
        &self.figures
    }

    pub fn body_for_clause(&self, clause_id: &str) -> Option<&str> {
        let clause = self.clause(clause_id)?;
        Some(&self.src[clause.body_start..clause.body_end])
    }

    /// Symbol cross-reference index over the productions.
    pub fn symbols(&self) -> &SymbolIndex {
        &self.symbols
    }

    /// ASCII-lowercased per-clause own body texts (crate-internal for the
    /// `find_symbol` text tier; match positions map 1:1 to `own_texts`).
    pub(crate) fn own_texts_lower(&self) -> &[String] {
        &self.own_texts_lower
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

    /// Three-tier search; the first tier that matches a clause wins for it:
    /// clauses whose id or title matches (document order, no snippet), then
    /// figure captions (caption snippet plus `figure`), then clauses whose
    /// own body text matches every query unit, ranked by total occurrence
    /// count. Matching is ASCII case-insensitive; see `SearchMode`.
    pub fn search(&self, query: &str, mode: SearchMode) -> Result<Vec<SearchHit<'_>>> {
        let matcher = Matcher::build(query, mode)?;
        let mut seen: HashSet<&str> = HashSet::new();
        let mut hits = Vec::new();

        for c in &self.clauses {
            let id_lower = c.id.to_ascii_lowercase();
            let title_lower = c.title.to_ascii_lowercase();
            if matcher.match_title(&id_lower, &title_lower) {
                seen.insert(c.id.as_str());
                hits.push(SearchHit {
                    clause: c,
                    snippet: None,
                    figure: None,
                });
            }
        }

        let mut figure_hits = 0;
        for f in &self.figures {
            if figure_hits >= BODY_MATCH_LIMIT {
                break;
            }
            if f.title.is_empty() || seen.contains(f.clause_id.as_str()) {
                continue;
            }
            if matcher.match_text(&f.title.to_ascii_lowercase()).is_none() {
                continue;
            }
            let Some(clause) = self.clause(&f.clause_id) else {
                continue;
            };
            seen.insert(clause.id.as_str());
            hits.push(SearchHit {
                clause,
                snippet: Some(format!("Figure {}. {}", f.number.unwrap_or(0), f.title)),
                figure: f.number,
            });
            figure_hits += 1;
        }

        // (score, clause index, first match position, match length)
        let mut body_matches: Vec<(usize, usize, usize, usize)> = Vec::new();
        for (i, c) in self.clauses.iter().enumerate() {
            if seen.contains(c.id.as_str()) {
                continue;
            }
            let Some((counts, pos, len)) = matcher.match_text(&self.own_texts_lower[i]) else {
                continue;
            };
            let score: usize = counts.iter().sum();
            body_matches.push((score, i, pos, len));
        }

        body_matches.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        body_matches.truncate(BODY_MATCH_LIMIT);
        hits.extend(body_matches.into_iter().map(|(_, i, pos, len)| SearchHit {
            clause: &self.clauses[i],
            snippet: self.snippet(i, pos, len),
            figure: None,
        }));
        Ok(hits)
    }

    pub(crate) fn snippet(&self, clause_idx: usize, pos: usize, len: usize) -> Option<String> {
        let text = &self.own_texts[clause_idx];
        if pos > text.len() {
            return None;
        }

        let mut start = pos.saturating_sub(SNIPPET_RADIUS);
        while !text.is_char_boundary(start) {
            start -= 1;
        }
        let mut end = (pos + len + SNIPPET_RADIUS).min(text.len());
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

/// Compiled query, shared by the three search tiers. Title, body and caption
/// texts are ASCII-lowercased before matching, so only `Regex` mode needs a
/// case flag: the user pattern itself is built case-insensitive.
enum Matcher {
    Substring {
        /// Whole trimmed lowercased query; title ids/titles match it as one
        /// substring (legacy behavior).
        query: String,
        /// Whitespace-separated tokens; body text must contain each of them.
        tokens: Vec<String>,
    },
    Word {
        res: Vec<Regex>,
    },
    Regex {
        re: Regex,
    },
}

impl Matcher {
    fn build(query: &str, mode: SearchMode) -> Result<Self> {
        let q = query.trim().to_ascii_lowercase();
        if q.is_empty() {
            bail!("empty query");
        }
        Ok(match mode {
            SearchMode::Substring => Matcher::Substring {
                tokens: q.split_whitespace().map(str::to_string).collect(),
                query: q,
            },
            SearchMode::Word => Matcher::Word {
                res: q
                    .split_whitespace()
                    .map(|t| Regex::new(&format!(r"\b{}\b", regex::escape(t))))
                    .collect::<std::result::Result<Vec<_>, _>>()?,
            },
            SearchMode::Regex => Matcher::Regex {
                re: RegexBuilder::new(query.trim())
                    .case_insensitive(true)
                    .build()
                    .map_err(|err| anyhow::anyhow!("invalid regex: {err}"))?,
            },
        })
    }

    fn match_title(&self, id_lower: &str, title_lower: &str) -> bool {
        match self {
            Matcher::Substring { query, .. } => {
                id_lower.contains(query.as_str()) || title_lower.contains(query.as_str())
            }
            Matcher::Word { res } => res
                .iter()
                .all(|re| re.is_match(id_lower) || re.is_match(title_lower)),
            Matcher::Regex { re } => re.is_match(id_lower) || re.is_match(title_lower),
        }
    }

    /// `(per-unit occurrence counts, first match position, first match
    /// length)`, `None` unless every unit matches (AND semantics).
    fn match_text(&self, lower: &str) -> Option<(Vec<usize>, usize, usize)> {
        let units = self.units(lower)?;
        let counts = units.iter().map(|u| u.2).collect();
        let (pos, len, _) = *units.iter().min_by_key(|u| u.0)?;
        Some((counts, pos, len))
    }

    /// `(first match position, match length, match count)` per query unit.
    fn units(&self, lower: &str) -> Option<Vec<(usize, usize, usize)>> {
        let mut out = Vec::new();
        match self {
            Matcher::Substring { tokens, .. } => {
                for tok in tokens {
                    let pos = lower.find(tok.as_str())?;
                    out.push((pos, tok.len(), lower.matches(tok.as_str()).count()));
                }
            }
            Matcher::Word { res } => {
                for re in res {
                    out.push(scan_re(re, lower)?);
                }
            }
            Matcher::Regex { re } => out.push(scan_re(re, lower)?),
        }
        Some(out)
    }
}

/// `(first match position, match length, match count)` for one regex.
fn scan_re(re: &Regex, lower: &str) -> Option<(usize, usize, usize)> {
    let mut count = 0;
    let mut first = None;
    for m in re.find_iter(lower) {
        if first.is_none() {
            first = Some((m.start(), m.len()));
        }
        count += 1;
    }
    let (pos, len) = first?;
    Some((pos, len, count))
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
    let symbols = SymbolIndex::build(&productions);

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
        symbols,
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

/// All proper numeric prefixes of a dotted clause id, shortest first
/// (`"8.2.4.1.1"` -> `"8"`, `"8.2"`, `"8.2.4"`, `"8.2.4.1"`).
fn dotted_prefixes(id: &str) -> Vec<String> {
    let parts: Vec<&str> = id.split('.').collect();
    let mut out = Vec::new();
    for i in 1..parts.len() {
        if parts[i].chars().all(|c| c.is_ascii_digit()) {
            out.push(parts[..=i - 1].join("."));
        } else {
            break;
        }
    }
    out
}
