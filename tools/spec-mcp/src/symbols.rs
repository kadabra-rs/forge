//! Cross-reference index over the grammar productions: which production
//! *defines* each identifier (its name), which *quote* it (quoted keyword
//! literals), and which *reference* it (unquoted identifier occurrences in a
//! body, plus declared supertypes). Consumed by the `find_symbol` tool.

use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use regex::{Captures, Regex};
use serde::Serialize;

use crate::bnf::Production;

/// Quoted keyword literal, e.g. `'specializes'`.
static QUOTED_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"'([^']*)'").expect("static regex"));

/// Unquoted identifier (also matches RHS operands of ownership/optionality
/// operators such as `ownedRelationship += X`, which is intended).
static IDENT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[A-Za-z_][A-Za-z0-9_]*").expect("static regex"));

/// How a production is related to a symbol.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SymbolKind {
    /// The production's name is the symbol (it defines the grammar term).
    Definition,
    /// The symbol occurs in the production body as an unquoted identifier,
    /// or is the production's declared supertype.
    Reference,
    /// The symbol is a quoted keyword literal in the production body.
    Keyword,
}

/// One symbol occurrence within one production.
#[derive(Clone, Debug)]
pub struct SymbolOccurrence {
    /// Index into `SpecIndex::productions()`.
    pub production: usize,
    pub clause_id: String,
    pub line: usize,
    pub kind: SymbolKind,
}

/// Lowercased symbol name → indices into `occurrences`, so lookups are
/// exact-key first, then (for queries of length >= 3) substring keys.
pub struct SymbolIndex {
    occurrences: Vec<SymbolOccurrence>,
    by_key: HashMap<String, Vec<usize>>,
}

impl SymbolIndex {
    /// Index every production: its name is a `Definition`, each quoted
    /// literal starting with an ASCII alphabetic char is a `Keyword`
    /// (regex-style literals such as `'[^\r\n]*'` and `'#x27'` are
    /// filtered), each unquoted identifier in the body is a `Reference`,
    /// and the declared supertype is a `Reference`. Deduped per
    /// `(key, production, kind)`; the first occurrence wins.
    pub fn build(productions: &[Production]) -> Self {
        let mut occurrences = Vec::new();
        let mut by_key: HashMap<String, Vec<usize>> = HashMap::new();
        let mut seen: HashSet<(String, usize, SymbolKind)> = HashSet::new();

        for (idx, p) in productions.iter().enumerate() {
            let mut push = |key: String, kind: SymbolKind| {
                if key.is_empty() || !seen.insert((key.clone(), idx, kind)) {
                    return;
                }
                by_key
                    .entry(key.clone())
                    .or_default()
                    .push(occurrences.len());
                occurrences.push(SymbolOccurrence {
                    production: idx,
                    clause_id: p.clause_id.clone(),
                    line: p.line,
                    kind,
                });
            };

            // 1. The production defines its own name.
            push(p.name.to_ascii_lowercase(), SymbolKind::Definition);

            // 2. Quoted keyword literals.
            for caps in QUOTED_RE.captures_iter(&p.body) {
                let literal = caps[1].trim();
                let key = literal.to_ascii_lowercase();
                if key.chars().next().is_some_and(|c| c.is_ascii_alphabetic()) {
                    push(key, SymbolKind::Keyword);
                }
            }

            // 3. Unquoted identifiers in the body: mask the quoted spans
            // first (preserving byte offsets) so keyword literals are not
            // also reported as references.
            let masked = QUOTED_RE.replace_all(&p.body, |c: &Captures| " ".repeat(c[0].len()));
            for m in IDENT_RE.find_iter(&masked) {
                push(m.as_str().to_ascii_lowercase(), SymbolKind::Reference);
            }

            // 4. Declared supertype (the `:` part of `Name : Sup =`).
            if let Some(sup) = &p.supertype {
                push(sup.to_ascii_lowercase(), SymbolKind::Reference);
            }
        }

        Self {
            occurrences,
            by_key,
        }
    }

    /// All occurrences whose key equals `sym` (lowercased), in occurrence
    /// order; then all occurrences whose key *contains* the normalized
    /// symbol — only when it is at least 3 chars long, to keep 1-2 char
    /// queries from flooding the result — excluding the exact key. Each
    /// pair is `(&Production, &SymbolOccurrence)`, production resolved via
    /// the caller's `productions` slice.
    // The pair-tuple return shape is load-bearing for the two-tier grammar
    // listing in `find_symbol`; a named alias would hide the
    // exact/partial distinction.
    #[allow(clippy::type_complexity)]
    pub fn lookup<'p>(
        &self,
        sym: &str,
        productions: &'p [Production],
    ) -> (
        Vec<(&'p Production, &SymbolOccurrence)>,
        Vec<(&'p Production, &SymbolOccurrence)>,
    ) {
        let key = sym.to_ascii_lowercase();
        let pair = |i: usize| {
            (
                &productions[self.occurrences[i].production],
                &self.occurrences[i],
            )
        };

        let exact = self
            .by_key
            .get(&key)
            .map(|idxs| idxs.iter().copied().map(pair).collect())
            .unwrap_or_default();

        let mut partial_idx: Vec<usize> = Vec::new();
        if key.len() >= 3 {
            for k in self.by_key.keys() {
                if k.as_str() != key.as_str() && k.contains(key.as_str()) {
                    partial_idx.extend_from_slice(&self.by_key[k]);
                }
            }
        }
        partial_idx.sort_unstable();
        let partial = partial_idx.iter().copied().map(pair).collect();

        (exact, partial)
    }
}
