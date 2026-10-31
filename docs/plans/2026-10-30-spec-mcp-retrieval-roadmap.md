# spec-mcp retrieval roadmap: search modes, TOC, find_symbol, tf-idf, figure context, semantic search

- [x] P0 — Add spec-mcp retrieval roadmap tracker
- [x] P1 — Add word/regex modes and figure-caption tier to search
- [x] P2 — Replace list_sections with nested spec_toc tool
- [x] P3 — Add ancestor path to get_section result
- [x] P4 — Add find_symbol grammar and prose cross-reference tool
- [x] P5 — Rank body hits by tf-idf and widen snippets to sentence windows
- [x] P6 — Return surrounding prose context from get_figure
- [x] P7 — Add embedding engine with first-run model download and disk cache
- [x] P8 — Add semantic_search tool with eval gate against tf-idf

Base: main @ 6f7a691. Resume protocol: first unchecked box.

## Context

Implement the user's 7-item retrieval roadmap for `tools/spec-mcp` (self-contained MCP oracle over the OMG KerML 1.0 + SysML v2 specs), phased, **one commit per item, verification between commits, sequential subagent delegation (one at a time, never parallel)**, with a durable in-repo tracker because sessions get cleared between phases. Item 3 (`near`) is re-dispositioned per user delegation ("razmisli… nema potrebe da ga radimo ako tebi to ne treba") — see Phase 3. Item 7 (semantic search) IS in scope, gated by an executable keep/drop eval at the end.

Verified current state that reshapes the roadmap (all read this session):

- `SpecIndex::search` (`src/index.rs:111`) is **already ASCII case-insensitive**, two-tier (id/title, then body AND-tokens ranked by occurrence count, ±100-char snippet). Item 1 = word-boundary/regex modes + figure-caption tier only.
- `src/bnf.rs::parse_bnf` **already structures** both `.kebnf` grammars into `Production { clause_id, name, body, line }` (284 KerML / 343 SysML productions, clause from `// Clause X` markers). Item 4's premise ("danas je samo embedded string") is outdated — Phase 4 is a symbol cross-reference index + one tool, plus recovering the currently-discarded production supertype.
- `FigureRef { number, title, image_path, clause_id }` already indexes every caption. **Corpus fact: every "Figure N" string in both specs is a caption (KerML 42/42, SysML 76/76 — verified by grep); there are zero prose figure citations.** So item 6's "koje figure cita klauza X" is already served by `get_section.figures` (embedded links); the only new direction is figure → surrounding text.
- `list_sections` returns a flat dump of all ~1600–2400 clauses; user chose: **`spec_toc` replaces `list_sections`** (clean cutover).
- Test corpus facts for real-spec assertions: `conjugate` 42× in KerML md, `PortUsage` 58× in SysML md, `isComposite` 9× in KerML md, `portType` **0×** (user's example symbol does not exist in the corpus — tests use verified symbols).
- Baseline: clean `main` @ `6f7a691` (spec-mcp v0.2.0). spec-mcp is its **own Cargo workspace** (`cd tools/spec-mcp` for every cargo command). Assets: KerML md 1.1 MB, SysML md 1.3 MB, figures 4.8 MB + 13 MB embedded.

## Execution model

- **Branch**: `spec-mcp-retrieval-roadmap` from `main` @ `6f7a691`. One branch for the whole roadmap; one commit per phase; push branch + open PR only after the Phase 8 gate decision (repo rule: no direct pushes to main).
- **Tracker** (survives session clearing): Phase 0 copies this plan to `docs/plans/2026-10-30-spec-mcp-retrieval-roadmap.md`, converting each phase heading into a `- [ ] P<N> — <commit subject>` checkbox list at the top. **Resume protocol for any fresh session/subagent: read the tracker, find the first unchecked phase, execute only that phase.** Each phase's implementer ticks its own checkbox in its own commit.
- **Delegation loop** (orchestrator = the session holding this plan): for each phase P, spawn exactly ONE general-purpose `task` subagent whose prompt is: work in `/Users/mjaric/prj/kadabra-rs/forge` on branch `spec-mcp-retrieval-roadmap`; read `docs/plans/2026-10-30-spec-mcp-retrieval-roadmap.md`; execute ONLY phase P top-to-bottom (edits, tests, gate, tick checkbox, `git add` the phase's files + tracker, commit with the phase's exact commit message); never proceed to phase P+1; never push; never run `git rebase`/`reset` on existing commits. Orchestrator then independently runs the gate + phase smoke below; only on green spawns P+1. On red: send the failure back to the same subagent (or a fresh one with the error output) — still one at a time.
- **Per-commit gate** (run in every phase, by subagent AND re-run by orchestrator):
  ```bash
  cd tools/spec-mcp && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
  ```
  (`tools/spec-mcp/Cargo.toml` has no `[lints]` section — default clippy + `-D warnings` is the project's existing bar; zero-warnings policy applies.)
- **E2E smoke harness** (rmcp stdio = newline-delimited JSON-RPC; each phase lists its own call):
  ```bash
  cd tools/spec-mcp && printf '%s\n' \
    '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"smoke","version":"0"}}}' \
    '{"jsonrpc":"2.0","method":"notifications/initialized"}' \
    '<TOOLS_CALL_LINE>' \
    | timeout 120 cargo run -q 2>/dev/null | sed -n '2p' | head -c 800
  ```
  Expected: one JSON response for `id:2` containing the phase's expected fragment. Phase 8 needs `timeout 600` (first call computes embeddings).
- **Code constraints** (global AGENTS.md): ≤100 lines/function, ≤5 positional params, 100-char lines, absolute imports (`crate::…`), no new dependency without the Phase 7 vetting step.

## Phase 0 — branch + tracker (commit C0)

1. `git switch -c spec-mcp-retrieval-roadmap` (tree is clean at `6f7a691`; if dirty, stop and report — do not stash or discard).
2. Create `docs/plans/2026-10-30-spec-mcp-retrieval-roadmap.md`: this plan's content verbatim, with a checkbox list inserted after the title:
   ```
   - [ ] P0 — Add spec-mcp retrieval roadmap tracker
   - [ ] P1 — Add word/regex modes and figure-caption tier to search
   - [ ] P2 — Replace list_sections with nested spec_toc tool
   - [ ] P3 — Add ancestor path to get_section result
   - [ ] P4 — Add find_symbol grammar and prose cross-reference tool
   - [ ] P5 — Rank body hits by tf-idf and widen snippets to sentence windows
   - [ ] P6 — Return surrounding prose context from get_figure
   - [x] P7 — Add embedding engine with first-run model download and disk cache
   - [x] P8 — Add semantic_search tool with eval gate against tf-idf
   ```
   plus a line `Base: main @ 6f7a691. Resume protocol: first unchecked box.`
3. Tick P0. Commit: `Add spec-mcp retrieval roadmap tracker`. No code changes → gate is `cargo test` unchanged (skip clippy/fmt, nothing touched).

## Phase 1 — search modes + figure-caption tier (item 1, commit C1)

Only `src/index.rs`, `src/tools.rs`, tests. No new deps (`regex` already present).

### index.rs

1. Add (needs `serde`, `schemars::JsonSchema` imports in index.rs):
   ```rust
   #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
   #[serde(rename_all = "lowercase")]
   pub enum SearchMode {
       #[default]
       Substring,
       Word,
       Regex,
   }
   ```
2. Add private `enum Matcher { Substring { tokens: Vec<String> }, Word { res: Vec<Regex> }, Regex { re: Regex } }` with:
   - `fn build(query: &str, mode: SearchMode) -> Result<Self>` (anyhow): trim query; empty → `bail!("empty query")`. Substring: tokens = `query.to_ascii_lowercase().split_whitespace()`. Word: one regex per lowercased token `format!(r"\b{}\b", regex::escape(tok))` (text is matched lowercased, so no case flag needed). Regex: `RegexBuilder::new(query).case_insensitive(true).build()`, map compile error with context.
   - `fn match_title(&self, id_lower: &str, title_lower: &str) -> bool`: Substring → whole lowercased query `contains` in either (current behavior: `c.id.contains(&q) || title_lower.contains(&q)`); Word → every token regex matches id or title; Regex → re matches id or title.
   - `fn match_text(&self, lower: &str) -> Option<(Vec<usize>, usize, usize)>` → `(per_unit_counts, first_pos, first_len)`, `None` unless every unit matches (AND semantics preserved). Units: substring tokens / word regexes / the single regex. `first_pos` = min first-match start across units (Substring unit = token: `lower.find(tok)`). Count per unit: Substring `lower.matches(tok).count()`; Word/Regex `re.find_iter(lower).count()`.
3. `SearchHit<'a>` gains `pub figure: Option<u32>`.
4. Rewrite `pub fn search(&self, query: &str, mode: SearchMode) -> Result<Vec<SearchHit<'_>>>` (signature change — `Result` now): build Matcher (propagate Err). Three tiers with a `seen: HashSet<&str>` of clause ids, first tier wins per clause:
   - **title tier** (unchanged semantics, now via `match_title(&c.id.to_ascii_lowercase(), &title_lower)`), `snippet: None, figure: None`, document order; insert ids into `seen`.
   - **figure tier** (new): iterate `self.figures` in doc order; skip `title.is_empty()`; if `matcher.match_text(&f.title.to_ascii_lowercase()).is_some()` and `f.clause_id` not in `seen` → `SearchHit { clause: owning clause (via self.clause(&f.clause_id); skip if None), snippet: Some(format!("Figure {}. {}", f.number.unwrap_or(0), f.title)), figure: f.number }`; insert into `seen`; cap at `BODY_MATCH_LIMIT`.
   - **body tier**: per clause not in `seen`, `match_text(&self.own_texts_lower[i])` → `(counts, pos, len)`; score = `counts.iter().sum()` (P5 replaces this sum with tf-idf — keep the `counts` vec shape so P5 only changes scoring); sort score desc then clause idx asc; truncate `BODY_MATCH_LIMIT`; snippet via generalized `self.snippet(i, pos, len)`; insert into `seen`.
5. Generalize `fn snippet(&self, clause_idx: usize, pos: usize, len: usize) -> Option<String>`: same ±`SNIPPET_RADIUS` char window around `[pos, pos+len)` as today (P5 replaces the window logic; keep the fn name/signature).

### tools.rs

6. `SearchQuery` gains:
   ```rust
   /// Match mode. "substring" (default): case-insensitive raw substrings, all
   /// tokens must match. "word": whole-word matches only. "regex": the query
   /// is one case-insensitive Rust regex.
   #[serde(default)]
   pub mode: Option<SearchMode>,
   ```
   (`Option` + `#[serde(default)]` keeps the param optional in the generated JSON schema — the rmcp `Parameters<>` path deserializes with serde; verified by the P1 smoke `tools/list` check.)
7. `SearchHitSummary` gains `#[serde(skip_serializing_if = "Option::is_none")] pub figure_number: Option<u32>`; `matched` doc comment becomes: `"title" | "figure" | "body"` (set `"figure"` when `hit.figure.is_some() && hit.snippet` starts with "Figure "… simpler: change `SearchHit` mapping — `matched = if figure.is_some() { "figure" } else if snippet.is_some() { "body" } else { "title" }`).
8. `search_sections_impl(&self, spec, query: &str, mode: SearchMode) -> Result<Vec<SearchHitSummary>, ErrorData>`: map `index.search` Err → `ErrorData::invalid_params(err.to_string(), None)`. Tool fn passes `mode.unwrap_or_default()`.
9. Replace the `search_sections` `#[tool(description = …)]` literal with:
   `"Full-text search over the KerML or SysML v2 spec (e.g. \"specialization\", \"isComposite\", \"8.3.4\"). Optional mode: \"substring\" (default) matches case-insensitive substrings, all tokens required; \"word\" matches whole words; \"regex\" treats the query as one case-insensitive Rust regex. Clauses whose id or title matches come first, then figure-caption matches (matched=\"figure\", with figure_number), then body matches ranked by occurrence count with a snippet. Returns clause ids for get_section. Set spec to \"kerml\" or \"sysml\"."`

### Tests (P1)

- `tests/index_tests.rs`: update existing `idx.search(q)` call sites → `idx.search(q, SearchMode::default()).unwrap()`.
- New index tests (synthetic md, follow existing `build_index` fixture style):
  - word mode: body "classified types" — `search("type", Word)` → no body hit; `search("type", Substring)` → hit.
  - regex mode: `search("specializ(ation|ing)", Regex)` hits body containing "specialization"; `search("[", Regex)` → `is_err()`.
  - figure tier: md with `![](_page_1_Figure_1.jpeg)\n\n**Figure 1. Widget diagram**\n` → `search("widget", Substring)` returns one hit with `figure == Some(1)`, snippet containing "Figure 1. Widget diagram", matched clause = owning clause; same query where the clause TITLE contains "Widget" → single title-tier hit (dedup).
- `tests/tools_tests.rs`: `search_sections_impl` with `SearchMode::Word`; `mode: None` in `SearchQuery` deserializes (serde_json round-trip test optional — the tools/list smoke covers schema).
- Real-spec test: `search(kerml, "conjugates", Word)` non-empty (42 occurrences verified).

**Commit C1**: `Add word/regex modes and figure-caption tier to search`
**Smoke P1**: `<TOOLS_CALL_LINE>` = `'{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"search_sections","arguments":{"spec":"kerml","query":"conjugates","mode":"word"}}}'` → expect `"matched"` values and clause ids like `7.5`/`8.2.x`; then repeat with `"method":"tools/list"` as id:2 and confirm `search_sections.inputSchema.properties.mode` exists and `required` lacks `mode`.

## Phase 2 — spec_toc replaces list_sections (item 2, commit C2)

### tools.rs

1. Delete: `list_sections` tool fn, `list_sections_impl`, `ClauseListResult`, `SpecOnlyQuery` (grep `SpecOnlyQuery\|ClauseListResult\|list_sections` — exactly these + tests reference them). Keep `ClauseSummary` (P3 reuses it; `pub` item → no dead-code warning).
2. Add wire types:
   ```rust
   #[derive(Clone, Serialize, Deserialize, JsonSchema)]
   pub struct TocQuery {
       /// Which spec to outline: "kerml" or "sysml".
       pub spec: SpecId,
       /// Maximum outline depth (1 = top-level chapters, e.g. "7 Semantics").
       /// Omit for the complete tree.
       #[serde(default)]
       pub max_depth: Option<u8>,
   }
   #[derive(Clone, Serialize, Deserialize, JsonSchema)]
   pub struct TocNode {
       pub id: String,
       pub title: String,
       pub anchor: String,
       /// 1 = top-level chapter; deeper = nested section/clause.
       pub depth: u8,
       pub children: Vec<TocNode>,
   }
   #[derive(Clone, Serialize, Deserialize, JsonSchema)]
   pub struct TocResult { pub spec: String, pub toc: Vec<TocNode> }
   ```
3. `pub fn spec_toc_impl(&self, spec: SpecId, max_depth: Option<u8>) -> Vec<TocNode>`: empty vec if spec unloaded. Build from `index.list_clauses()` (doc order = pre-order): `min_level = clauses.iter().map(|c| c.level).min().unwrap_or(1)`; `depth(c) = c.level - min_level + 1`. Recursive nest (safe Rust, no raw pointers):
   ```rust
   fn nest(clauses: &[ClauseNode], i: &mut usize, min_level: u8,
           parent_depth: u8, max_depth: Option<u8>) -> Vec<TocNode> {
       let mut out = Vec::new();
       while *i < clauses.len() {
           let d = clauses[*i].level - min_level + 1;
           if d <= parent_depth { break; }
           let c = clauses[*i].clone(); *i += 1;
           let children = nest(clauses, i, min_level, d, max_depth);
           if max_depth.is_none_or(|m| d <= m) {
               out.push(TocNode { id: c.id, title: c.title, anchor: c.anchor, depth: d, children });
           }
       }
       out
   }
   // entry: let mut i = 0; nest(clauses, &mut i, min_level, 0, max_depth)
   ```
   (Level jumps, e.g. depth 1 → 3, attach to the current stack top — same rule `extract_clause_tree` uses for body_end. Nodes deeper than max_depth are dropped with their subtrees; recursion still walks them, which is fine at these sizes.)
4. Tool:
   ```rust
   #[tool(name = "spec_toc", description = "Nested outline of an OMG spec's clause tree: chapters, sections, and clauses with id, title, anchor, and depth (1 = top-level chapter). Optional max_depth trims deeper levels — use 1 or 2 to orient before drilling in with get_section; omit for the complete tree. Set spec to \"kerml\" or \"sysml\".")]
   fn spec_toc(&self, Parameters(TocQuery { spec, max_depth }): Parameters<TocQuery>)
       -> Result<Json<TocResult>, ErrorData>   // index_or_err(spec)? then Json(TocResult { spec: spec.label().to_string(), toc: self.spec_toc_impl(spec, max_depth) })
   ```
5. `get_info` instructions literal: replace `Typical flow: search_sections to locate a clause` with `Typical flow: spec_toc to see the outline, search_sections to locate a clause`.

### Repo consumers of the removed tool name (clean cutover, part of this commit)

- `.omp/skills/sysml-modeling/SKILL.md` line ~35: replace `| \`list_sections {spec}\` | Full clause tree (id, title, anchor) — orient before drilling in. |` with `| \`spec_toc {spec, max_depth?}\` | Nested clause outline (id, title, anchor, depth); max_depth 1–2 to orient, omit for the full tree. |`
- `tools/spec-mcp/README.md` line ~26: replace the `list_sections` row with `| \`spec_toc\` | nested clause outline (chapters → sections → clauses), optional max_depth |`.
- Grep the whole repo for `list_sections` after edits — must return zero hits.

### Tests (P2)

- `tests/tools_tests.rs`: `list_sections_returns_all_clauses` → `spec_toc_nests_and_filters`: synthetic md `## 7.3 A` / `### 7.3.2 B` / `#### 7.3.2.1 C` (keep the `<span id>` + `**…**` heading format of existing fixtures) → unlimited: one root depth 1 with nested child depth 2 with child depth 3; `max_depth=1` → root only, `children.is_empty()`. `unloaded_spec_returns_nothing`: `list_sections_impl` → `spec_toc_impl(SpecId::Sysml, None)` empty.
- Real-spec test: `spec_toc_impl(Kerml, Some(1))` → every node `depth == 1`, `children` empty, ids include `"7"`, count between 5 and 25 (17 level-2 headings measured, includes front chapters).

**Commit C2**: `Replace list_sections with nested spec_toc tool`
**Smoke P2**: tools/call `spec_toc` `{"spec":"kerml","max_depth":1}` → expect `"depth":1` nodes incl. id `"7"`; tools/list → `spec_toc` present, `list_sections` absent.

## Phase 3 — ancestor path on get_section; `near` tool dropped (item 3 disposition, commit C3)

**Decision (user delegated):** no standalone `near` tool. Rationale recorded in tracker: with `spec_toc` giving the full ordered outline (prev/next/siblings are read off it) and `get_section` already returning the clause's entire subtree body, the only genuine gap is ancestor context in-place — so `get_section` gains `path`.

1. `src/index.rs`:
   ```rust
   /// Ancestor chain root-first (chapter → … → parent), exclusive of the
   /// clause itself. Parent = nearest preceding clause with a smaller level
   /// (same rule body_end uses); empty for top-level clauses / unknown ids.
   pub fn ancestors(&self, clause_id: &str) -> Vec<&ClauseNode> {
       // by_id → index i; walk self.clauses[..i].rev(), keep c.level < current,
       // each hit lowers current; reverse at the end.
   }
   ```
2. `src/tools.rs`: `SectionResult` gains `pub path: Vec<ClauseSummary>` (doc: "Ancestor clauses root-first (chapter → parent); the clause itself is `id`/`title`."). `get_section_impl` fills it from `index.ancestors(clause_id)` mapped to `ClauseSummary { id, title, anchor }`. `get_section` tool description: append `" The \`path\` field gives the ancestor chain (chapter → parent) for orientation."` before the closing quote.

### Tests (P3)

- Synthetic: `7.3.2.3` under `7.3.2` under `7.3` → `path` ids `["7.3", "7.3.2"]` root-first.
- Real-spec: kerml `8.2.4.1.2` → `path` non-empty, `path[0].id == "8"`, every ancestor id is a dotted prefix of `"8.2.4.1.2"`.

**Commit C3**: `Add ancestor path to get_section result`
**Smoke P3**: get_section `{"spec":"kerml","clause_id":"8.2.4.1.2"}` → `"path"` array non-empty, first element `"id":"8"`.

## Phase 4 — find_symbol (item 4, commit C4)

### bnf.rs — recover the supertype

`production_header` currently parses then discards `: Supertype`. Change return to `Option<(String, Option<String>, String)>` (name, supertype, rhs); `Production` gains `pub supertype: Option<String>`. Update the two `Production { … }` literals in `parse_bnf` and every literal in `tests/bnf_tests.rs` (grep `Production {`).

### New `src/symbols.rs` (+ `pub mod symbols;` in lib.rs)

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SymbolKind { Definition, Reference, Keyword }

#[derive(Clone, Debug)]
pub struct SymbolOccurrence {
    /// Index into SpecIndex::productions().
    pub production: usize,
    pub clause_id: String,
    pub line: usize,
    pub kind: SymbolKind,
}

pub struct SymbolIndex {
    occurrences: Vec<SymbolOccurrence>,
    /// Lowercased symbol name → occurrence indices.
    by_key: HashMap<String, Vec<usize>>,
}

impl SymbolIndex {
    pub fn build(productions: &[Production]) -> Self;
    /// Exact-key hits first, then keys containing `sym` (only when
    /// sym.len() >= 3), each sorted by occurrence order. Returns
    /// (occurrences, production_name) pairs via the caller's productions slice.
    pub fn lookup<'p>(&self, sym: &str, productions: &'p [Production])
        -> (Vec<(&'p Production, &SymbolOccurrence)>, Vec<(&'p Production, &SymbolOccurrence)>);
}
```

`build` per production `p` (two statics: `QUOTED_RE = Regex::new(r"'([^']*)'")`, `IDENT_RE = Regex::new(r"[A-Za-z_][A-Za-z0-9_]*")`):
- `p.name` → Definition, key = `name.to_ascii_lowercase()`.
- `QUOTED_RE.captures_iter(&p.body)` → Keyword for each literal whose trimmed lowercase key starts with an ASCII alphabetic char (filters regex literals like `'[^\r\n]*'`, `'#x27'`).
- Mask quoted spans (`QUOTED_RE.replace_all(&p.body, |c: &Captures| " ".repeat(c[0].len()))` — preserves byte offsets), then `IDENT_RE.find_iter` → Reference (includes ownership targets like `conjugatedType`; recursion self-references are legitimate, keep).
- `p.supertype` → Reference.
- Dedup: `HashSet<(key, production_idx, kind)>`, first occurrence wins.

`SpecIndex` gains field `symbols: SymbolIndex` (built in `build_index` after productions) + `pub fn symbols(&self) -> &SymbolIndex`.

### tools.rs

```rust
#[derive(Clone, Serialize, Deserialize, JsonSchema)]
pub struct SymbolQuery {
    /// Which spec to search: "kerml" or "sysml".
    pub spec: SpecId,
    /// Identifier: keyword ("conjugates"), production ("Conjugation"), or
    /// model-level name ("isComposite", "PortUsage").
    pub symbol: String,
}
#[derive(Clone, Serialize, Deserialize, JsonSchema)]
pub struct SymbolGrammarHit {
    pub production: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supertype: Option<String>,
    /// Full EBNF right-hand side.
    pub body: String,
    pub clause_id: String,
    pub line: usize,
    /// "definition" | "reference" | "keyword".
    pub kind: String,
}
#[derive(Clone, Serialize, Deserialize, JsonSchema)]
pub struct SymbolTextHit { pub clause_id: String, pub title: String, pub snippet: String }
#[derive(Clone, Serialize, Deserialize, JsonSchema)]
pub struct SymbolResult { pub symbol: String, pub grammar: Vec<SymbolGrammarHit>, pub text: Vec<SymbolTextHit> }
```

`find_symbol_impl(&self, spec, symbol) -> Result<SymbolResult, ErrorData>` (None-spec → `index_or_err`):
- `symbol.trim()` empty → `ErrorData::invalid_params("symbol must not be empty", None)`.
- Grammar tier: `symbols.lookup(sym, index.productions())`; map to `SymbolGrammarHit`; exact list then partial list; dedup by `(production.name, kind)`; cap 30.
- Text tier: `RegexBuilder::new(&format!(r"\b{}", regex::escape(&sym.to_ascii_lowercase()))).case_insensitive(true).build()` — word-start **prefix** match so "conjugate" also finds "conjugates"/"conjugation". Scan `own_texts_lower`, collect `(count, first_pos, len)`, sort count desc then doc order, cap 20, snippet via `index.snippet(i, pos, len)` (make `snippet` `pub(crate)`).
- Both tiers empty → success with empty vecs (not an error).

Tool:
```rust
#[tool(name = "find_symbol", description = "Map any identifier — keyword ('conjugates'), grammar production (Conjugation), or model-level name (isComposite, PortUsage) — to where the spec defines and uses it: grammar hits (productions declaring, referencing, or quoting the name, with full EBNF body and defining clause) and text hits (clauses whose prose mentions it, with snippets). Exact matches first, then substring. Complements get_syntax, which searches production names only.")]
```
`get_info` instructions: replace `get_syntax for grammar productions, follow_link` with `get_syntax for grammar productions, find_symbol to map any identifier to its defining clause and grammar production, follow_link`.

### Tests (P4)

- `tests/bnf_tests.rs`: supertype — `FeatureDeclaration : Feature =` → `supertype == Some("Feature")`; plain `Specialization =` → `None`.
- New `tests/symbols_tests.rs` (or extend index_tests): synthetic bnf `Specialization =\n    'specializes'\n` + `FeatureDeclaration : Feature =\n    'feature'\n` → `find_symbol("specializes")` grammar contains kind "keyword" production "Specialization"; `find_symbol("Feature")` grammar contains kind "definition" (FeatureDeclaration) and kind "reference" (supertype).
- tools_tests synthetic: text tier — md body containing "isComposite" → `find_symbol("isComposite")` text hit with snippet.
- Real-spec: kerml `find_symbol("conjugate")` → grammar contains production `"CONJUGATES"` (verified at kebnf line 146) and text hits non-empty (42 prose occurrences verified); sysml `find_symbol("PortUsage")` → text non-empty (58 verified).

**Commit C4**: `Add find_symbol grammar and prose cross-reference tool`
**Smoke P4**: find_symbol `{"spec":"kerml","symbol":"conjugate"}` → `"production":"CONJUGATES"` in grammar, non-empty text array.

## Phase 5 — tf-idf ranking + sentence-window snippets (item 5, commit C5)

### index.rs

1. Body-tier scoring in `search`: one pass collecting `(clause_idx, counts: Vec<usize>, first_pos, first_len)` for all matching clauses (P1's `match_text` already returns per-unit counts). `df[u]` = number of matching clauses with `counts[u] > 0`; `N = self.clauses.len()`; `idf[u] = ((1.0 + N as f64) / (1.0 + df[u] as f64)).ln() + 1.0`; `score = Σ_u counts[u] as f64 * idf[u]`. Sort score desc, then clause idx asc. Truncate `BODY_MATCH_LIMIT`. (Single-unit modes — Regex, one-token Word/Substring — reduce to df-scaled counts; ordering across clauses is unchanged for a single token, which is correct.)
2. Replace the ±100-char snippet window with sentence windows:
   - `fn sentence_spans(text: &str) -> Vec<(usize, usize)>`: a sentence ends at `.`/`!`/`?` followed by ASCII whitespace or end-of-text; spans cover the whole text (trailing remainder is its own span). ("7.3.2" doesn't split — `.` followed by a digit; "e.g. " may split — acceptable for a snippet window.)
   - Rewrite `snippet(&self, clause_idx, pos, len)`: find span containing `pos`; window = spans `[i-2 ..= i+2]` clamped; join with `' '`; collapse internal whitespace (`split_whitespace().join(" ")`); while joined > 600 chars and window has >1 span, drop alternately the last then the first span; if still > 600, hard-truncate at a char boundary nearest the match and append `'…'`. Prepend `'…'` if window start byte > 0, append `'…'` if window end < text len.
   - Delete `SNIPPET_RADIUS`.
3. `search_sections` tool description: replace `ranked by occurrence count with a snippet` with `ranked by tf-idf, each with a ±2-sentence snippet around the first match`.

### Tests (P5)

- tf-idf flip (assert order only, never scores): clauses in doc order B then A; B body `alpha alpha alpha alpha beta`, A body `alpha beta beta beta beta`, plus clauses C, D, E, F each `alpha gamma` (df(alpha)=6… any N where A's rare-token weight wins: with N=6, df_alpha=6? B,A,C,D,E,F all contain alpha → df=6, idf=ln(7/7)+1=1.0; df_beta=2, idf=ln(7/3)+1≈1.85; A=1+3·1.85=6.55 > B=4+1.85=5.85; old count ranking: B=5 > A=4). Assert A precedes B for query `"alpha beta"` and note in a comment that raw-count ranking would put B first.
- Sentence window: body of 7 single-sentence paragraphs (`S1 one. S2 two. …`), match in S4 → snippet contains S2..S6, not S1/S7; starts and ends with `…`.
- Existing snippet tests still pass (they assert `contains` only).

**Commit C5**: `Rank body hits by tf-idf and widen snippets to sentence windows`
**Smoke P5**: search_sections `{"spec":"kerml","query":"conjugation direction"}` → body hit snippets noticeably longer than 200 chars, multi-sentence.

## Phase 6 — figure context (item 6 scope-adjusted, commit C6)

**Scope note (corpus fact, record in tracker):** "koje figure cita klauza X" needs no new work — every "Figure N" string in both specs is a caption (42/42, 76/76 verified), i.e. zero prose citations exist; embedded figures per clause are already `get_section.figures`. This phase adds the missing direction only: figure → surrounding text.

1. `src/index.rs`: `FigureRef` gains `pub link_start: usize, pub link_end: usize` (byte offsets of the `![](...)` match in md — `caps.get(0)` start/end already available in `extract_figures`). Add `pub fn own_text(&self, clause_id: &str) -> Option<&str>` (`by_id` → `&self.own_texts[i]`).
2. `src/tools.rs`: `FigureResult` gains:
   ```rust
   /// Prose surrounding the embedded image in its clause (±2 sentences),
   /// including the caption line.
   #[serde(skip_serializing_if = "Option::is_none")]
   pub context_md: Option<String>,
   ```
   `get_figure_impl`: after finding `fig`, `context_md = index.clause(&fig.clause_id).and_then(|c| index.own_text(&fig.clause_id).zip(...))` — concretely: clause index via `by_id` (add `pub fn clause_index(&self, id: &str) -> Option<usize>` to SpecIndex), local `pos = fig.link_start - clause.body_start`, `len = link_end - link_start`, then `index.snippet(clause_index, pos, len)` (the P5 sentence window; make `snippet` reachable from tools.rs — `pub(crate)` already in P4). The caption line directly follows the link in md, so it lands inside the window. `None` when the clause or window is unavailable.
   The owning clause is always the deepest clause containing the link byte (`find_clause_for_byte`), so the link lies within its own text — no bounds hazard.
3. `get_figure` tool description: append `" The result includes \`context_md\`: the prose around the figure in its clause."`.

### Tests (P6)

- Synthetic: prose sentences + image link + `**Figure 1. Widget diagram**` + trailing prose → `get_figure_impl(_, 1).context_md` contains "Widget diagram" and the adjacent sentence.
- Real-spec: kerml figure 1 → `context_md` is `Some` and contains "Figure 1.".

**Commit C6**: `Return surrounding prose context from get_figure`
**Smoke P6**: get_figure `{"spec":"kerml","figure_number":1}` → non-empty `"context_md"`.

## Phase 7 — vendored model + fastembed + embedding engine (item 7a, commit C7)

### Dependency vetting (AGENTS.md workflow — do first, record results in tracker)

- `fastembed = "=7.1.1"` — released 2026-10-07 (23 days old at planning; **re-check ≥14 days at execution**), MIT license, repo `qdrant/fastembed`. 7.1.2–7.5.0 all published 2026-10-19..23 → ineligible under the 2-week rule at planning time. Exact pin `=7.1.1`.
- Features: `default-features = false, features = ["ort-download-binaries-native-tls"]` — drops `hf-hub` (no runtime download code path at all) and `image-models`. Pulls `ort =2.0.0-rc.13` (fastembed's own exact pin), `tokenizers ^0.23`.
- Run `cd tools/spec-mcp && cargo audit` after adding; any fastembed/ort advisory → stop, report, do not work around.
- API verified against the 7.1.1 source this session: `UserDefinedEmbeddingModel::new(onnx_file: Vec<u8>, tokenizer_files: TokenizerFiles)`; `TokenizerFiles { tokenizer_file, config_file, special_tokens_map_file, tokenizer_config_file }` (all `Vec<u8>`); `TextEmbedding::try_new_from_user_defined(model, InitOptionsUserDefined::new())` — fully in-memory, no disk staging of the model.

### Vendor the model (hermetic; ~23 MB into git — repo already vendors the multi-hundred-MB SysML release)

`tools/spec-mcp/models/all-MiniLM-L6-v2-q8/` from HF `Xenova/all-MiniLM-L6-v2` @ revision `751bff37182d3f1213fa05d7196b954e230abad9` (this is exactly the file fastembed's `AllMiniLML6V2Q` uses; repo license tag `apache-2.0`, base model MIT — redistributable):

|local file|HF path|expected size|
|---|---|---|
|`model.onnx`|`onnx/model_quantized.onnx`|22,972,370 B|
|`tokenizer.json`|`tokenizer.json`|711,661 B|
|`config.json`|`config.json`|650 B|
|`special_tokens_map.json`|`special_tokens_map.json`|125 B|
|`tokenizer_config.json`|`tokenizer_config.json`|366 B|

Download via `curl -L "https://huggingface.co/Xenova/all-MiniLM-L6-v2/resolve/751bff37182d3f1213fa05d7196b954e230abad9/<path>"`, verify each byte size matches the table. `models/README.md` records: source repo, revision sha, per-file `shasum -a 256` output, license, retrieval date (provenance record — part of vendoring, not doc cleanup).

### New `src/embed.rs` (+ `pub mod embed;`)

```rust
pub const MODEL_DIM: usize = 384;
const CACHE_KEY: &str = "minilm6v2q8-v1";

pub struct SemanticEngine { model: fastembed::TextEmbedding }
impl SemanticEngine {
    /// From include_bytes! of the five vendored files.
    pub fn init() -> Result<Self>;
    /// L2-normalized rows; embeds in batches of 64.
    pub fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>>;
}

pub struct SpecEmbeddings {
    /// Flat row-major, L2-normalized, MODEL_DIM per row.
    pub vectors: Vec<f32>,
    /// Row r ↔ clauses[clause_idxs[r]]; clauses with empty trimmed own_text are excluded.
    pub clause_idxs: Vec<usize>,
}
```
- `init`: `UserDefinedEmbeddingModel::new(include_bytes!("../models/all-MiniLM-L6-v2-q8/model.onnx").to_vec(), TokenizerFiles { … four include_bytes! … .to_vec() })` → `TextEmbedding::try_new_from_user_defined(model, InitOptionsUserDefined::new())`.
- `embed`: call `model.embed(chunk, None)`, then L2-normalize every row defensively (do not rely on fastembed's normalization).
- Cache file: `<cache_dir>/embeddings-<spec_label>-<CACHE_KEY>-<md_len>-<clause_count>.bin`, little-endian `[u32 dim][u32 rows][f32 …]`. `cache_dir()`: `$SPEC_MCP_CACHE` → `$XDG_CACHE_HOME/spec-mcp` → `$HOME/.cache/spec-mcp` → `std::env::temp_dir().join("spec-mcp")`; `create_dir_all`; unwritable → proceed uncached (compute each first-call), no error. Load validates dim/rows/file-length exactly, else recompute; write via temp file + rename.
- `SpecServer` gains: `engine: OnceLock<Result<Arc<SemanticEngine>, String>>` and `embeddings: HashMap<SpecId, OnceLock<Result<Arc<SpecEmbeddings>, String>>>` (built in both constructors for `SpecId::ALL`/the single test spec). Lazy: nothing initializes at startup — first `semantic_search` (P8) triggers `get_or_init`. Computation blocks that one call (~30–60 s for ~2000 clauses on CPU); the disk cache makes it once-per-machine. `with_spec_source` (tests) gets the same fields so synthetic specs can embed.
- Startup cost of merely adding the fields: zero (OnceLock empty). `cargo build` binary grows ~23 MB (model) + ort static lib — expected, note in tracker.

### Tests (P7)

- `SemanticEngine::init()` succeeds and `embed(&["specialization".into()])` → 1 row, `row.len() == MODEL_DIM`, L2 norm ≈ 1.0 (±1e-3). Mark `#[ignore]`-free but keep it the only engine test here (P8 adds behavior tests). If CI duration matters: this test runs the ort session once — seconds, acceptable.
- Cache round-trip: write `SpecEmbeddings` to a temp `SPEC_MCP_CACHE` dir, reload, vectors equal; corrupt dims → recompute path returns fresh vectors (test via env-var-pointed temp dir).

**Commit C7**: `Vendor MiniLM q8 model and add embedding engine with disk cache`
**Smoke P7**: gate only (no new tool surface) + `ls -la target/debug/spec-mcp` shows the expected multi-MB growth.

## Phase 8 — semantic_search + eval gate (item 7b, commit C8)

### Tool surface (tools.rs)

```rust
#[derive(Clone, Serialize, Deserialize, JsonSchema)]
pub struct SemanticQuery {
    /// Which spec to search: "kerml" or "sysml".
    pub spec: SpecId,
    /// Natural-language question or concept, e.g. "rules for flipping in/out direction".
    pub query: String,
    /// Max hits (default 10, clamped to 1..=25).
    #[serde(default)]
    pub limit: Option<u16>,
}
#[derive(Clone, Serialize, Deserialize, JsonSchema)]
pub struct SemanticHit { pub id: String, pub title: String, pub anchor: String, pub score: f32, pub snippet: Option<String> }
#[derive(Clone, Serialize, Deserialize, JsonSchema)]
pub struct SemanticResult { pub spec: String, pub query: String, pub hits: Vec<SemanticHit> }
```
`semantic_search_impl(&self, spec, query, limit) -> Result<SemanticResult, ErrorData>`: engine/embeddings `get_or_init` errors → `ErrorData::internal_error(reason)`; embed query (normalize), score = dot over rows, top-`limit.unwrap_or(10).clamp(1,25)` by score desc, ties → clause doc order; snippet = `index.snippet(clause_idx, 0, 0)` (leading sentence window). Empty query → invalid_params.
Tool description literal:
`"Semantic (embedding) search over clause meaning — use when lexical search_sections misses a concept phrased differently (e.g. \"rules for flipping in/out direction\" finds the conjugation clauses). Returns clauses ranked by cosine similarity (score) with snippets. The first call per spec computes and caches clause embeddings (~1 minute); later calls are fast. For identifiers and exact terms prefer search_sections or find_symbol."`
`get_info` instructions: append `semantic_search finds clauses by meaning when lexical search misses.` before the closing quote. SKILL.md + README tables gain the `semantic_search` row (same style as P2/P4 rows).

### Eval gate (executable keep/drop)

New `tests/semantic_eval.rs`, single `#[test] #[ignore = "embeds the full corpus; run manually"] fn semantic_vs_lexical_recall()`:
1. Query set (fixed literals; expected clause ids are **established by the implementer during this phase** by reading candidates with `get_section` and recording 1–3 verified clause ids per query as a `const` table in this file — bounded data-collection, not a design decision):
   - kerml: "how does conjugation flip feature direction"; "rules for redefining a feature"; "difference between a type and a feature"; "what makes an association a link"; "multiplicity constraints on features"; "specialization cycle detection semantics".
   - sysml: "requirement satisfaction semantics"; "how ports connect items across a boundary"; "allocating behavior to structural elements"; "entry and exit actions of a state"; "attribute usage typing and default values"; "view and viewpoint rendering of models".
2. For each query: `semantic_search` top-5 and lexical `search` (Substring mode) top-5 (title+body tiers as returned); recall@5 = fraction of queries whose expected set intersects top-5.
3. Print a per-query table (query, lexical hit?, semantic hit?, ids) — this output is the phase's evidence.
4. Keep-rule assertion: `sem_recall5 > lex_recall5 && sem_exclusive_wins >= 3 && sem_exclusive_losses <= 1` (exclusive = hit expected when the other missed).
5. **Drop procedure if the assertion fails**: `git revert --no-edit <C8-sha> <C7-sha>` (C8 first), tick P8 in the tracker with `DROPPED (eval: <printed recalls>)`, keep going — nothing after P8 depends on it. Record either outcome (KEPT/DROPPED + numbers) in the tracker.

### Tests (P8, non-ignored)

- Synthetic-spec `semantic_search_impl` returns hits ordered by score desc, `limit` respected (limit=1 → 1 hit), scores ≤ 1.0 + ε, snippet present. Uses the lazy engine on a 2-clause synthetic spec (a few seconds).
- Unloaded spec → Err. Empty query → Err.

**Commit C8**: `Add semantic_search tool with eval gate against tf-idf`
**Smoke P8** (`timeout 600`; first call embeds ~1600 KerML clauses): semantic_search `{"spec":"kerml","query":"how does conjugation flip feature direction","limit":5}` → non-empty hits with scores; second identical call returns in <5 s (cache warm). Then run the gate: `cargo test --release -- --ignored semantic_vs_lexical_recall --nocapture` and record KEPT/DROPPED + the printed recalls in the tracker.

## After the gate (orchestrator, not a subagent phase)

Push branch, open PR against `main` describing the shipped tool surface changes (spec_toc replaces list_sections; new find_symbol, semantic_search-if-kept; extended search_sections/get_section/get_figure). Version bump + `spec-mcp-vX.Y.Z` tag + npm publish are the existing release ritual — out of this plan's scope unless the user asks.

## Critical files & anchors

- `tools/spec-mcp/src/index.rs` — `SpecIndex` (fields at :48-62), `search` (:111), `snippet` (:144), `build_index` (:171), `extract_figures` (:216); every phase touches this file.
- `tools/spec-mcp/src/tools.rs` — `SpecServer` impl (:29-277), wire structs (:291-480), `#[rmcp::tool_router]` block (:482-640), `get_info` instructions literal (:653-667).
- `tools/spec-mcp/src/bnf.rs` — `Production` (:3-9), `production_header` (:76-96, supertype discarded at :85-93 → P4).
- `tools/spec-mcp/src/clause_tree.rs` — `ClauseNode` fields incl. `level`, `body_start`, `own_end` (:5-23); read-only reference for P2/P3/P6 geometry.
- `tools/spec-mcp/tests/tools_tests.rs` — fixture style (`build_test_server` :4-9, synthetic md/bnf string literals); copy this pattern in every phase.

## Verification (aggregate)

Every phase: gate command (fmt/clippy/test) + the phase's stdio smoke with its expected fragment (listed per phase above). Additionally:

- P2 grep check: `grep -rn list_sections .` (repo root, excluding target/ and the tracker's history notes) → zero hits.
- New-behavior end-to-end proofs are the smokes: word/regex mode call (P1), spec_toc max_depth=1 outline (P2), path in get_section (P3), find_symbol("conjugate") → CONJUGATES (P4), multi-sentence tf-idf snippets (P5), get_figure context_md (P6), semantic_search hit + warm-cache latency (P8).
- Final: full `cargo test` (incl. real-spec tests) on the branch; `tools/list` smoke shows exactly: spec_toc, get_section, search_sections, get_syntax, find_symbol, get_figure, follow_link, list_examples, get_example, find_implementation (+ semantic_search iff KEPT); P8 gate output recorded in tracker.
- Prerequisites: none beyond the repo (assets embedded at compile time). P7/P8 need network once at `cargo build` time (ort prebuilt download via `download-binaries`) — CI runners and dev machine have it; note in tracker.

## Assumptions & contingencies

- **near() dropped** (user delegated the call): spec_toc + get_section.path cover orientation; prev/next is read off spec_toc order. If the user later wants explicit prev/next ids, that's a small follow-up on `SpecIndex::ancestors`' doc-order neighbor — not in this plan.
- **fastembed pin age**: `=7.1.1` verified 23 days old on 2026-10-30. If execution slips or `cargo audit` flags 7.1.1, bump to the newest release that is ≥14 days old at that moment and re-verify `InitOptionsUserDefined`/`UserDefinedEmbeddingModel`/`TokenizerFiles` field names against the vendored crate source before proceeding (same discovery steps as this plan used).
- **ort prebuilt availability**: release targets are x86_64/aarch64-linux-gnu, aarch64/x86_64-apple-darwin (verified in `.github/workflows/spec-mcp-release.yml`); `ort/download-binaries` (pyke) is expected to cover all four — *unverified until P7 builds*. If a target lacks a prebuilt: do NOT switch to `ort-load-dynamic` (breaks self-containment); treat as gate failure → drop procedure (revert C7/C8) and record.
- **Semantic first-call latency vs MCP client timeouts**: cache write happens after full computation; if a client kills the server mid-compute the cache never lands. Orchestrator warms the cache manually via the P8 smoke (timeout 600) immediately after C8 — before any agent traffic.
- **HF revision drift**: model files pinned to revision `751bff3…` with byte-size checks; if the download 404s, stop and report (do not substitute `main`).
- **Eval ground truth**: expected clause ids for the P8 query set are collected by the P8 implementer via get_section reading and frozen in `tests/semantic_eval.rs`; if fewer than 3 exclusive semantic wins materialize, the drop procedure is mandatory, not negotiable.

## Execution notes

- P3: `SpecIndex::ancestors` is derived from the dotted clause id (every existing proper numeric prefix, root-first), **not** from heading levels — in the OMG sources a child heading can sit at the same level as its parent (e.g. `8.2.1` is `###` like its parent `8.2`; 494/534 KerML clauses resolve to a different parent under the level-walk). Level-walk nesting would also have mis-nested the P2 outline.
- P2: `spec_toc` nesting corrected to the same id-derived parent rule (deepest existing proper dotted prefix; clauses with no such prefix are roots) in the `Nest spec_toc by clause id hierarchy` commit, keeping the `TocNode`/`TocResult`/`TocQuery` wire shapes and the tool description unchanged.
- P5: `df` is corpus-wide document frequency (clauses whose own text contains the unit, over all `N = clauses.len()`), matching the phase's test arithmetic (`df(alpha)=6`, `N=6`) and standard tf-idf; per-matching-clause df degenerates to a constant idf under the AND-token semantics. The flip test's A body is `alpha beta beta beta` (3 beta, 4 occurrences total) so raw occurrence-count ranking genuinely puts B (5) first while tf-idf puts A first.
- P5 follow-up: the match length threaded from `scan_re`/`units`/`match_text` into `snippet` was never read once the ±`SNIPPET_RADIUS` char window became a sentence window, so `snippet(clause_idx, pos)` now takes two arguments and the length is gone from the whole pipeline (`Remove unused match length from snippet pipeline`). Later phases call `snippet(i, pos)`; the plan text still shows the three-argument form.
- P6: clause→figure direction needed no work (corpus fact: every "Figure N" string in both specs is a caption — 42/42 KerML, 76/76 SysML — so `get_section.figures` already answers "koje figure cita klauza X"); P6 adds the reverse direction only, `get_figure.context_md` (±2-sentence window around the image link, caption included).
- P6 fix (enabling): `find_clause_for_byte` picked the longest *id* among enclosing clauses, which mis-attributed 24/44 KerML figures (an ancestor whose `body_end` overshoots into following chapters has a longer id than the deepest clause); it now picks the enclosing clause with the largest `body_start` (the innermost), leaving 2 KerML + 1 SysML front-matter images outside every clause (no context, `clause_id` empty). Offsets are `FigureRef::link_start`/`link_end`; `figure_context` still guards the own-text bounds, so a pathological corpus yields `None` instead of a panic.

- P7 (revised per user decision): the MiniLM model is **downloaded on first use**, not vendored into git or embedded in the binary — there is no `models/` directory and no `include_bytes!` in this phase (the plan's "Vendor MiniLM q8 model" design was rejected; the model bytes stay out of git and out of the binary — only the static ONNX Runtime library links in, growing the release binary ~24 → ~49 MB). The model is `all-MiniLM-L6-v2` (q8, `Xenova/all-MiniLM-L6-v2`), ~23 MB, pulled via `fastembed`'s `hf-hub` path into `cache_dir()` (`$SPEC_MCP_CACHE` → `$XDG_CACHE_HOME/spec-mcp` → `$HOME/.cache/spec-mcp` → temp dir); `show_download_progress` is off so progress bars never reach an MCP stdio stream.
- P7: `fastembed =7.1.1` is **Apache-2.0**, not MIT as the plan claimed. Features: `default-features = false` + `hf-hub-native-tls` (kept: the model downloads on first run) + `ort-download-binaries-native-tls`; this drops `image-models` from the defaults.
- P7: `hf-hub` **pins no revision** — the model resolves from the repo `main` ref at download time (the first download on this machine happened to land on revision `751bff3…`, matching the plan's byte-size table). Recorded as a known reproducibility limit: a future upstream change to that HF repo could alter the model on machines with cold caches. The clause-embedding cache is keyed on `CACHE_KEY` + spec + source length + clause count, so a corpus change invalidates it; the cache file is written via temp-file + rename and validated on load (exact dim/rows/byte-length).
- P7: `TextEmbedding::embed` takes `&mut self`, so `SemanticEngine` holds the session in a `Mutex` (inference is serialized; one corpus pass plus one query per call).
- P7: embedding input is `"{id} {title}\n{own text}"` with fenced code blocks removed and whitespace collapsed, split into ≤1600-char chunks (max 8 per clause), mean-pooled then L2-normalized — because 18% of clauses exceed the model's 512-token window and naive truncation would discard most of the substantive clauses. Clauses with no text left after code-block removal are excluded.
- P7: `cargo audit` passes (exit 0) after the dependency: no advisories on fastembed/ort/tokenizers/hf-hub. The lockfile needed `cookie_store 0.22.0 → 0.22.1` (its `time 0.3.16` lower bound dragged in vulnerable `time 0.3.45`, RUSTSEC-2026-0009); `time` is pinned to `0.3.47` (fixed, Rust 1.85-compatible). The three remaining `cargo audit` warnings (paste unmaintained, anyhow/tokio unsoundness, both pre-existing in the tree) are not named by the phase's stop-condition.
- P7: `ort/download-binaries` (pyke) prebuilt download verified on this machine (aarch64-apple-darwin): first `cargo build` fetched it without issue; the remaining release targets are still unverified until CI builds them.

- P8 eval gate — **KEPT**: `cargo test --release -- --ignored semantic_vs_lexical_recall --nocapture` (12 queries, 6 KerML + 6 SysML, all with ground truth frozen in `tests/semantic_eval.rs` after reading the candidate clauses): recall@5 lexical 4/12 = 0.333, semantic 11/12 = 0.917; exclusive semantic-only wins = 8, semantic-only losses = 1 (the "attribute usage typing and default values" query — lexical hit on `7.6.3` alone, semantic top-5 missed both `7.7.2` and `7.6.3`; semantic top-1 was the near-adjacent `7.7`). Keep-rule `sem_recall5 > lex_recall5 && exclusive_wins >= 3 && exclusive_losses <= 1` satisfied → the tool stays; no revert of C7/C8.
- P8 timings (user-requested evidence, run on this machine, aarch64-apple-darwin, release): (a) cold engine init in a throwaway `SPEC_MCP_CACHE` (23 MB model download via hf-hub + session init + first embed): **3.5 s** of test-process time (9.8 s wall incl. release compile); (b) cold clause-embedding build, corpus absent from cache — KerML (471 embedded clauses, ~1103 KB md): **9.4 s**, SysML (705, ~1326 KB): **13.3 s**; (c) warm cache load — KerML: **49 ms**, SysML: **12 ms**; (d) one semantic query end-to-end after warm-up: **2 ms** (eval) / **3.8 s wall** for the full stdio server round trip in the smoke (initialize + call, cache warm, < 5 s requirement met).
- P8 note: the `snippet(clause_idx, pos)` two-argument form from the P5 follow-up note is what `semantic_search_impl` calls (pos = 0, leading sentence window) and what the timing hooks use; the 3-argument form in the plan body is historical.
- P8 independent timing check (orchestrator, real stdio server, release binary, aarch64-apple-darwin, cache wiped with `mv`, request→response measured inside the client so process lifetime and pipe setup are excluded): **cold first `semantic_search` on KerML = 12.65 s** (hf-hub downloads the ~23 MB model, the session initializes, 471 clause embeddings are computed, the query is embedded and scored); **warm KerML = 0.05 s**; **SysML first call with the model already cached = 13.46 s** (705 clauses embedded, model not re-downloaded); **warm SysML = 0.05 s**. So per-machine one-off cost is ~13 s per spec (~26 s total for both), later calls are ~50 ms end-to-end. Cold run burns ~73 s of CPU across cores (user 73.4 s vs real 13.4 s), i.e. the embedding pass is parallel; the process is not idle-blocked on the model.
- P8 note: a stdio harness that closes stdin immediately after writing the request can lose the response to a cold-cache `semantic_search` — the server finishes the shutdown before flushing the compute-heavy result. Real MCP clients hold the connection open, so this is a harness artifact; measure with the stream kept open (a client that waits for the response) or the number is meaningless.
