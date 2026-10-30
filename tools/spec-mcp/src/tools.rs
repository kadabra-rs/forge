use anyhow::Result;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use base64::Engine as _;
use regex::RegexBuilder;
use rmcp::handler::server::wrapper::{Json, Parameters};
use rmcp::model::{CallToolResult, Content, ErrorData};
use rmcp::tool;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::assets::{self, SpecId};
use crate::clause_tree::ClauseNode;
use crate::implementation::find_implementation;
use crate::index::{SearchMode, SpecIndex, build_index};
use crate::symbols::SymbolKind;

/// Sections can reference many figures; inline at most this many images per
/// get_section response. All figures are still listed in the JSON metadata.
const INLINE_FIGURE_LIMIT: usize = 6;

/// `list_examples` caps its output so the result stays readable; the corpora
/// are far smaller than this today.
const MAX_EXAMPLE_LISTING: usize = 600;

pub struct SpecServer {
    specs: HashMap<SpecId, SpecIndex>,
    crates_dir: PathBuf,
}

impl SpecServer {
    /// Production constructor: both specs are built from assets embedded in
    /// the binary; `crates_dir` is only used by `find_implementation`.
    pub fn embedded(crates_dir: PathBuf) -> Result<Self> {
        let mut specs = HashMap::new();
        for spec in SpecId::ALL {
            specs.insert(
                spec,
                build_index(assets::spec_md(spec), assets::spec_bnf(spec))?,
            );
        }
        Ok(Self { specs, crates_dir })
    }

    /// Test constructor: a single spec built from the given sources.
    pub fn with_spec_source(
        spec: SpecId,
        md: &str,
        bnf: &str,
        crates_dir: PathBuf,
    ) -> Result<Self> {
        let mut specs = HashMap::new();
        specs.insert(spec, build_index(md, bnf)?);
        Ok(Self { specs, crates_dir })
    }

    fn index(&self, spec: SpecId) -> Option<&SpecIndex> {
        self.specs.get(&spec)
    }

    fn index_or_err(&self, spec: SpecId) -> Result<&SpecIndex, ErrorData> {
        self.index(spec).ok_or_else(|| {
            ErrorData::invalid_params(
                format!("spec '{}' is not loaded in this server", spec.label()),
                None,
            )
        })
    }

    pub fn spec_toc_impl(&self, spec: SpecId, max_depth: Option<u8>) -> Vec<TocNode> {
        let Some(index) = self.index(spec) else {
            return vec![];
        };
        let clauses = index.list_clauses();
        // Each clause attaches to its id-derived parent (deepest existing
        // proper dotted prefix); clauses with no such prefix are roots.
        let mut child_links: Vec<Vec<usize>> = vec![Vec::new(); clauses.len()];
        let mut roots: Vec<usize> = Vec::new();
        for (i, c) in clauses.iter().enumerate() {
            match index.parent_index(&c.id) {
                Some(p) => child_links[p].push(i),
                None => roots.push(i),
            }
        }
        roots
            .iter()
            .flat_map(|&i| toc_subtree(clauses, &child_links, i, 1, max_depth))
            .collect()
    }

    /// All clause ids of a spec in document order, or empty when the spec is
    /// not loaded.
    pub fn clause_ids_impl(&self, spec: SpecId) -> Vec<String> {
        let Some(index) = self.index(spec) else {
            return vec![];
        };
        index.list_clauses().iter().map(|c| c.id.clone()).collect()
    }

    pub fn get_section_impl(&self, spec: SpecId, clause_id: &str) -> Option<SectionResult> {
        let index = self.index(spec)?;
        let clause = index.clause(clause_id)?;
        let productions = index.productions_for_clause(clause_id);
        let figures = index
            .figures_for_clause(clause_id)
            .iter()
            .map(|f| FigureSummary {
                number: f.number,
                title: f.title.clone(),
                image_file: f.image_path.clone(),
            })
            .collect();
        let body = index
            .body_for_clause(clause_id)
            .unwrap_or_default()
            .trim()
            .to_string();
        Some(SectionResult {
            spec: spec.label().to_string(),
            id: clause.id.clone(),
            title: clause.title.clone(),
            anchor: clause.anchor.clone(),
            line: clause.line,
            path: index
                .ancestors(clause_id)
                .into_iter()
                .map(|c| ClauseSummary {
                    id: c.id.clone(),
                    title: c.title.clone(),
                    anchor: c.anchor.clone(),
                })
                .collect(),
            body_md: body,
            productions: productions.iter().map(|p| p.name.clone()).collect(),
            figures,
        })
    }

    pub fn search_sections_impl(
        &self,
        spec: SpecId,
        query: &str,
        mode: SearchMode,
    ) -> Result<Vec<SearchHitSummary>, ErrorData> {
        let Some(index) = self.index(spec) else {
            return Ok(vec![]);
        };
        let hits = index
            .search(query, mode)
            .map_err(|err| ErrorData::invalid_params(err.to_string(), None))?;
        Ok(hits
            .into_iter()
            .map(|hit| SearchHitSummary {
                id: hit.clause.id.clone(),
                title: hit.clause.title.clone(),
                anchor: hit.clause.anchor.clone(),
                matched: if hit.figure.is_some() {
                    "figure"
                } else if hit.snippet.is_some() {
                    "body"
                } else {
                    "title"
                }
                .to_string(),
                figure_number: hit.figure,
                snippet: hit.snippet,
            })
            .collect())
    }

    pub fn get_syntax_impl(&self, spec: SpecId, production: &str) -> Vec<ProductionResult> {
        let Some(index) = self.index(spec) else {
            return vec![];
        };
        let q = production.to_lowercase();
        let mut hits: Vec<&crate::bnf::Production> = index
            .productions()
            .iter()
            .filter(|p| p.name.to_lowercase().contains(&q))
            .collect();
        // Exact matches first, then shortest names (closest to the query).
        hits.sort_by_key(|p| (p.name.to_lowercase() != q, p.name.len()));
        hits.truncate(20);
        hits.into_iter()
            .map(|p| ProductionResult {
                name: p.name.clone(),
                body: p.body.clone(),
                clause_id: p.clause_id.clone(),
                line: p.line,
            })
            .collect()
    }

    /// Map an identifier to the grammar productions that define, reference,
    /// or quote it, plus the clauses whose prose mentions it.
    pub fn find_symbol_impl(&self, spec: SpecId, symbol: &str) -> Result<SymbolResult, ErrorData> {
        let index = self.index_or_err(spec)?;
        let sym = symbol.trim();
        if sym.is_empty() {
            return Err(ErrorData::invalid_params("symbol must not be empty", None));
        }

        // Grammar tier: exact keys first, then substring keys; dedup by
        // (production name, kind), capped.
        let (exact, partial) = index.symbols().lookup(sym, index.productions());
        let mut seen: HashSet<(String, String)> = HashSet::new();
        let mut grammar = Vec::new();
        for (p, occ) in exact.iter().chain(partial.iter()) {
            let kind = kind_name(occ.kind).to_string();
            if !seen.insert((p.name.clone(), kind.clone())) {
                continue;
            }
            grammar.push(SymbolGrammarHit {
                production: p.name.clone(),
                supertype: p.supertype.clone(),
                body: p.body.clone(),
                clause_id: p.clause_id.clone(),
                line: p.line,
                kind,
            });
        }
        grammar.truncate(30);

        // Text tier: word-start prefix match in each clause's own body,
        // ranked by occurrence count then document order.
        let escaped = regex::escape(&sym.to_ascii_lowercase());
        let re = RegexBuilder::new(&format!(r"\b{}", escaped))
            .case_insensitive(true)
            .build()
            .map_err(|err| ErrorData::internal_error(err.to_string(), None))?;
        let clauses = index.list_clauses();
        let mut matches = Vec::new();
        for (i, lower) in index.own_texts_lower().iter().enumerate() {
            let Some(first) = re.find(lower) else {
                continue;
            };
            let count = re.find_iter(lower).count();
            matches.push((count, i, first.start(), first.len()));
        }
        matches.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        matches.truncate(20);
        let text = matches
            .into_iter()
            .filter_map(|(_count, i, pos, len)| {
                let clause = &clauses[i];
                let snippet = index.snippet(i, pos, len)?;
                Some(SymbolTextHit {
                    clause_id: clause.id.clone(),
                    title: clause.title.clone(),
                    snippet,
                })
            })
            .collect();

        Ok(SymbolResult {
            symbol: sym.to_string(),
            grammar,
            text,
        })
    }

    pub fn find_implementation_impl(&self, spec: SpecId, clause_id: &str) -> ImplResult {
        if !self.crates_dir.exists() {
            return ImplResult {
                status: format!(
                    "unavailable: crates directory '{}' not found (run from the forge repo \
                     root or pass the crates dir as the first CLI argument)",
                    self.crates_dir.display()
                ),
                entry_points: vec![],
            };
        }
        let productions: Vec<&str> = self
            .index(spec)
            .map(|index| {
                index
                    .productions_for_clause(clause_id)
                    .iter()
                    .map(|p| p.name.as_str())
                    .collect()
            })
            .unwrap_or_default();
        match find_implementation(clause_id, &productions, &self.crates_dir) {
            Ok(r) => ImplResult {
                status: format!("{:?}", r.status),
                entry_points: r.entry_points,
            },
            Err(e) => ImplResult {
                status: format!("error: {e}"),
                entry_points: vec![],
            },
        }
    }

    /// Embedded worked-example model sources, sorted by path (so entries group by
    /// top-level directory) and capped at [`MAX_EXAMPLE_LISTING`].
    pub fn list_examples_impl(&self, spec: SpecId) -> Vec<ExampleSummary> {
        assets::example_paths(spec)
            .into_iter()
            .take(MAX_EXAMPLE_LISTING)
            .map(|path| {
                let bytes = assets::example_source(spec, &path).map_or(0, |s| s.len());
                ExampleSummary { path, bytes }
            })
            .collect()
    }

    /// Full source of one embedded example model, or `None` when the relative
    /// path is unknown or unsafe.
    pub fn get_example_impl(&self, spec: SpecId, path: &str) -> Option<ExampleResult> {
        let bytes = assets::example_source(spec, path)?;
        Some(ExampleResult {
            path: path.to_string(),
            source: String::from_utf8_lossy(bytes.as_ref()).into_owned(),
        })
    }

    pub fn get_figure_impl(&self, spec: SpecId, figure_number: u32) -> Option<FigureResult> {
        let index = self.index(spec)?;
        // Uncaptioned diagrams have no printed number; the first figure in
        // document order carrying this number wins (spec numbering restarts
        // per part, so duplicates would still resolve deterministically).
        let fig = index
            .figures()
            .iter()
            .find(|f| f.number == Some(figure_number))?;
        Some(FigureResult {
            spec: spec.label().to_string(),
            number: figure_number,
            title: fig.title.clone(),
            image_file: fig.image_path.clone(),
            clause_id: fig.clause_id.clone(),
        })
    }

    pub fn follow_link_impl(&self, spec: SpecId, link: &str) -> LinkResult {
        let Some(index) = self.index(spec) else {
            return LinkResult {
                kind: "error".to_string(),
                clause_id: None,
                title: None,
                url: None,
                error: Some(format!("spec '{}' is not loaded", spec.label())),
            };
        };
        if let Some(anchor) = link.strip_prefix('#') {
            if let Some(clause) = index.resolve_anchor(anchor) {
                return LinkResult {
                    kind: "section".to_string(),
                    clause_id: Some(clause.id.clone()),
                    title: Some(clause.title.clone()),
                    url: None,
                    error: None,
                };
            }
            return LinkResult {
                kind: "anchor_not_found".to_string(),
                clause_id: None,
                title: None,
                url: None,
                error: Some(format!("No clause with anchor '{anchor}'")),
            };
        }
        if link.starts_with("http://") || link.starts_with("https://") {
            return LinkResult {
                kind: "url".to_string(),
                clause_id: None,
                title: None,
                url: Some(link.to_string()),
                error: None,
            };
        }
        LinkResult {
            kind: "unknown".to_string(),
            clause_id: None,
            title: None,
            url: None,
            error: Some(format!("Unrecognized link format: {link}")),
        }
    }
}

/// Build the TOC subtree rooted at the clause at index `i` (which is at
/// `depth`): one node whose `children` are its children (document order),
/// each recursed at `depth + 1`. Returns an empty vec when `depth` exceeds
/// `max_depth`, dropping the subtree.
fn toc_subtree(
    clauses: &[ClauseNode],
    child_links: &[Vec<usize>],
    i: usize,
    depth: u8,
    max_depth: Option<u8>,
) -> Vec<TocNode> {
    if !max_depth.is_none_or(|m| depth <= m) {
        return Vec::new();
    }
    let mut node = TocNode {
        id: clauses[i].id.clone(),
        title: clauses[i].title.clone(),
        anchor: clauses[i].anchor.clone(),
        depth,
        children: Vec::new(),
    };
    for &c in &child_links[i] {
        node.children
            .extend(toc_subtree(clauses, child_links, c, depth + 1, max_depth));
    }
    vec![node]
}

fn json_text_content<T: Serialize>(value: &T) -> Result<Content, ErrorData> {
    let text = serde_json::to_string_pretty(value)
        .map_err(|e| ErrorData::internal_error(format!("serialization failed: {e}"), None))?;
    Ok(Content::text(text))
}

fn image_content(spec: SpecId, image_file: &str) -> Option<Content> {
    let bytes = assets::figure_bytes(spec, image_file)?;
    let data = base64::engine::general_purpose::STANDARD.encode(bytes.as_ref());
    Some(Content::image(data, "image/jpeg"))
}

/// Wire-level name of a [`SymbolKind`] (`"definition" | "reference" |
/// "keyword"`).
fn kind_name(kind: SymbolKind) -> &'static str {
    match kind {
        SymbolKind::Definition => "definition",
        SymbolKind::Reference => "reference",
        SymbolKind::Keyword => "keyword",
    }
}

#[derive(Clone, Serialize, Deserialize, JsonSchema)]
pub struct ClauseSummary {
    pub id: String,
    pub title: String,
    pub anchor: String,
}

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
pub struct SectionQuery {
    /// Which spec to query: "kerml" or "sysml".
    pub spec: SpecId,
    /// Clause number as written in the spec, e.g. "7.3.2" or "8.2.4.1.2".
    pub clause_id: String,
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
pub struct TocResult {
    pub spec: String,
    pub toc: Vec<TocNode>,
}

#[derive(Clone, Serialize, Deserialize, JsonSchema)]
pub struct FigureSummary {
    /// Printed figure number ("Figure N."), or `null` for diagrams the spec
    /// embeds without a caption. Uncaptioned figures are still listed (and
    /// their images inlined) but cannot be fetched with `get_figure`.
    pub number: Option<u32>,
    /// Caption text; empty for uncaptioned diagrams.
    pub title: String,
    /// File name the spec Markdown links relatively, e.g. "_page_131_Figure_3.jpeg".
    pub image_file: String,
}

#[derive(Clone, Serialize, Deserialize, JsonSchema)]
pub struct SectionResult {
    pub spec: String,
    pub id: String,
    pub title: String,
    pub anchor: String,
    pub line: usize,
    /// Ancestor clauses root-first (chapter → parent); the clause itself is
    /// `id`/`title`.
    pub path: Vec<ClauseSummary>,
    pub body_md: String,
    pub productions: Vec<String>,
    /// Figures whose images this clause's body links to. Their images are
    /// attached to the tool result in this order (up to a limit).
    pub figures: Vec<FigureSummary>,
}

#[derive(Clone, Serialize, Deserialize, JsonSchema)]
pub struct SearchQuery {
    /// Which spec to search: "kerml" or "sysml".
    pub spec: SpecId,
    /// Case-insensitive query. Matched as a whole against clause ids and
    /// titles; body matches require every whitespace-separated token.
    pub query: String,
    /// Match mode. "substring" (default): case-insensitive raw substrings, all
    /// tokens must match. "word": whole-word matches only. "regex": the query
    /// is one case-insensitive Rust regex.
    #[serde(default)]
    pub mode: Option<SearchMode>,
}

#[derive(Clone, Serialize, Deserialize, JsonSchema)]
pub struct SearchHitSummary {
    pub id: String,
    pub title: String,
    pub anchor: String,
    /// Where the query matched: "title" (clause id or title), "figure"
    /// (figure caption) or "body" (clause text). Title matches are listed
    /// first, then figure caption matches, then body matches.
    pub matched: String,
    /// Printed figure number for "figure" matches.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub figure_number: Option<u32>,
    /// For body matches, an excerpt around the first occurrence; for figure
    /// matches, the caption line.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snippet: Option<String>,
}

#[derive(Clone, Serialize, Deserialize, JsonSchema)]
pub struct SearchResult {
    pub clauses: Vec<SearchHitSummary>,
}

#[derive(Clone, Serialize, Deserialize, JsonSchema)]
pub struct SyntaxQuery {
    /// Which grammar to search: "kerml" or "sysml".
    pub spec: SpecId,
    /// Production name, exact or substring, e.g. "FeatureDeclaration".
    pub production: String,
}

#[derive(Clone, Serialize, Deserialize, JsonSchema)]
pub struct ProductionResult {
    pub name: String,
    /// EBNF right-hand side as written in the official .kebnf grammar.
    pub body: String,
    /// Spec clause that defines this production.
    pub clause_id: String,
    pub line: usize,
}

#[derive(Clone, Serialize, Deserialize, JsonSchema)]
pub struct SyntaxResult {
    pub productions: Vec<ProductionResult>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct SymbolQuery {
    /// Which spec to search: "kerml" or "sysml".
    pub spec: SpecId,
    /// Identifier: keyword ("conjugates"), production ("Conjugation"), or
    /// model-level name ("isComposite", "PortUsage").
    pub symbol: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
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

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct SymbolTextHit {
    pub clause_id: String,
    pub title: String,
    pub snippet: String,
}

#[derive(Clone, Serialize, Deserialize, JsonSchema)]
pub struct SymbolResult {
    pub symbol: String,
    pub grammar: Vec<SymbolGrammarHit>,
    pub text: Vec<SymbolTextHit>,
}

#[derive(Clone, Serialize, Deserialize, JsonSchema)]
pub struct ImplQuery {
    /// Which spec the clause belongs to: "kerml" or "sysml".
    pub spec: SpecId,
    /// Clause number, e.g. "8.2.4.3.1".
    pub clause_id: String,
}

#[derive(Clone, Serialize, Deserialize, JsonSchema)]
pub struct ImplResult {
    pub status: String,
    pub entry_points: Vec<String>,
}

/// One embedded worked-example model file.
#[derive(Clone, Serialize, Deserialize, JsonSchema)]
pub struct ExampleSummary {
    /// Path relative to the corpus root, e.g.
    /// "training/32. Requirements/Requirement Definitions.sysml".
    pub path: String,
    /// Source size in bytes.
    pub bytes: usize,
}

#[derive(Clone, Serialize, Deserialize, JsonSchema)]
pub struct ExampleListQuery {
    /// Which corpus to list: "kerml" (58 `.kerml` examples) or "sysml"
    /// (examples/, training/, validation/ `.sysml` models).
    pub spec: SpecId,
}

#[derive(Clone, Serialize, Deserialize, JsonSchema)]
pub struct ExampleListResult {
    pub spec: String,
    pub count: usize,
    /// Sorted by path, so entries group by top-level directory.
    pub examples: Vec<ExampleSummary>,
}

#[derive(Clone, Serialize, Deserialize, JsonSchema)]
pub struct ExampleQuery {
    /// Which corpus to read: "kerml" or "sysml".
    pub spec: SpecId,
    /// Path exactly as returned by list_examples, e.g.
    /// "training/32. Requirements/Requirement Definitions.sysml".
    pub path: String,
}

#[derive(Clone, Serialize, Deserialize, JsonSchema)]
pub struct ExampleResult {
    pub path: String,
    /// Full model source text.
    pub source: String,
}

#[derive(Clone, Serialize, Deserialize, JsonSchema)]
pub struct FigureQuery {
    /// Which spec the figure belongs to: "kerml" or "sysml".
    pub spec: SpecId,
    /// Figure number as printed in the spec, e.g. 1 for "Figure 1."
    pub figure_number: u32,
}

#[derive(Clone, Serialize, Deserialize, JsonSchema)]
pub struct FigureResult {
    pub spec: String,
    pub number: u32,
    pub title: String,
    pub image_file: String,
    pub clause_id: String,
}

#[derive(Clone, Serialize, Deserialize, JsonSchema)]
pub struct FollowLinkQuery {
    /// Which spec the link came from: "kerml" or "sysml".
    pub spec: SpecId,
    /// The link as it appears in the spec Markdown, e.g. "#page-51-0".
    pub link: String,
}

#[derive(Clone, Serialize, Deserialize, JsonSchema)]
pub struct LinkResult {
    pub kind: String,
    pub clause_id: Option<String>,
    pub title: Option<String>,
    pub url: Option<String>,
    pub error: Option<String>,
}

#[rmcp::tool_router]
impl SpecServer {
    #[tool(
        name = "spec_toc",
        description = "Nested outline of an OMG spec's clause tree: chapters, sections, and clauses with id, title, anchor, and depth (1 = top-level chapter). Optional max_depth trims deeper levels — use 1 or 2 to orient before drilling in with get_section; omit for the complete tree. Set spec to \"kerml\" or \"sysml\"."
    )]
    fn spec_toc(
        &self,
        Parameters(TocQuery { spec, max_depth }): Parameters<TocQuery>,
    ) -> Result<Json<TocResult>, ErrorData> {
        self.index_or_err(spec)?;
        Ok(Json(TocResult {
            spec: spec.label().to_string(),
            toc: self.spec_toc_impl(spec, max_depth),
        }))
    }

    #[tool(
        name = "get_section",
        description = "Read one clause of the KerML or SysML v2 spec by clause id (e.g. \"7.3.2\", \"8.2.4.1.2\"). Returns the clause body as Markdown, the BNF production names it defines, and its figures — the figure images are attached to the result inline so diagrams can be viewed directly. Relative image links in the body (e.g. _page_131_Figure_3.jpeg) map to the entries in the `figures` list. The `path` field gives the ancestor chain (chapter → parent) for orientation."
    )]
    fn get_section(
        &self,
        Parameters(SectionQuery { spec, clause_id }): Parameters<SectionQuery>,
    ) -> Result<CallToolResult, ErrorData> {
        self.index_or_err(spec)?;
        let section = self.get_section_impl(spec, &clause_id).ok_or_else(|| {
            ErrorData::invalid_params(
                format!(
                    "no clause '{clause_id}' in the {} spec (try search_sections)",
                    spec.label()
                ),
                None,
            )
        })?;
        let mut content = vec![json_text_content(&section)?];
        content.extend(
            section
                .figures
                .iter()
                .take(INLINE_FIGURE_LIMIT)
                .filter_map(|f| image_content(spec, &f.image_file)),
        );
        Ok(CallToolResult::success(content))
    }

    #[tool(
        name = "list_examples",
        description = "List the OMG worked-example model corpus embedded in this server: complete `.kerml`/`.sysml` model sources shipped with the KerML 1.0 / SysML v2 release (examples/, training/, validation/). Returns each file's path and byte size, sorted by path. Read one with get_example. Use these to copy real modeling idioms instead of inventing syntax."
    )]
    fn list_examples(
        &self,
        Parameters(ExampleListQuery { spec }): Parameters<ExampleListQuery>,
    ) -> Json<ExampleListResult> {
        let examples = self.list_examples_impl(spec);
        Json(ExampleListResult {
            spec: spec.label().to_string(),
            count: examples.len(),
            examples,
        })
    }

    #[tool(
        name = "get_example",
        description = "Read one worked-example model source from the OMG corpus by its exact path as listed by list_examples (paths may contain spaces, e.g. \"training/32. Requirements/Requirement Definitions.sysml\"). Returns the full `.kerml`/`.sysml` text. Use list_examples first to discover paths."
    )]
    fn get_example(
        &self,
        Parameters(ExampleQuery { spec, path }): Parameters<ExampleQuery>,
    ) -> Result<Json<ExampleResult>, ErrorData> {
        self.get_example_impl(spec, &path).map(Json).ok_or_else(|| {
            ErrorData::invalid_params(
                format!(
                    "example '{path}' not found in the {} corpus; try list_examples",
                    spec.label()
                ),
                None,
            )
        })
    }

    #[tool(
        name = "search_sections",
        description = "Full-text search over the KerML or SysML v2 spec (e.g. \"specialization\", \"isComposite\", \"8.3.4\"). Optional mode: \"substring\" (default) matches case-insensitive substrings, all tokens required; \"word\" matches whole words; \"regex\" treats the query as one case-insensitive Rust regex. Clauses whose id or title matches come first, then figure-caption matches (matched=\"figure\", with figure_number), then body matches ranked by occurrence count with a snippet. Returns clause ids for get_section. Set spec to \"kerml\" or \"sysml\"."
    )]
    fn search_sections(
        &self,
        Parameters(SearchQuery { spec, query, mode }): Parameters<SearchQuery>,
    ) -> Result<Json<SearchResult>, ErrorData> {
        self.index_or_err(spec)?;
        Ok(Json(SearchResult {
            clauses: self.search_sections_impl(spec, &query, mode.unwrap_or_default())?,
        }))
    }

    #[tool(
        name = "get_syntax",
        description = "Look up textual-notation grammar productions in the official OMG .kebnf grammar of KerML or SysML v2, by production name (exact or substring, e.g. \"FeatureDeclaration\" or \"import\"). Returns each production's EBNF body and the spec clause that defines it. Use this for concrete syntax questions; use get_section for semantics."
    )]
    fn get_syntax(
        &self,
        Parameters(SyntaxQuery { spec, production }): Parameters<SyntaxQuery>,
    ) -> Result<Json<SyntaxResult>, ErrorData> {
        self.index_or_err(spec)?;
        Ok(Json(SyntaxResult {
            productions: self.get_syntax_impl(spec, &production),
        }))
    }

    #[tool(
        name = "find_symbol",
        description = "Map any identifier — keyword ('conjugates'), grammar production (Conjugation), or model-level name (isComposite, PortUsage) — to where the spec defines and uses it: grammar hits (productions declaring, referencing, or quoting the name, with full EBNF body and defining clause) and text hits (clauses whose prose mentions it, with snippets). Exact matches first, then substring. Complements get_syntax, which searches production names only."
    )]
    fn find_symbol(
        &self,
        Parameters(SymbolQuery { spec, symbol }): Parameters<SymbolQuery>,
    ) -> Result<Json<SymbolResult>, ErrorData> {
        let result = self.find_symbol_impl(spec, &symbol)?;
        Ok(Json(result))
    }

    #[tool(
        name = "find_implementation",
        description = "Map a KerML/SysML spec clause to kermlc compiler code: derives expected lexer/parser symbols from the clause's BNF productions and greps the forge crates/ directory for them. Reports Implemented / NotStarted / OutOfScope with file:line entry points. Only works when the server runs inside the forge repo."
    )]
    fn find_implementation(
        &self,
        Parameters(ImplQuery { spec, clause_id }): Parameters<ImplQuery>,
    ) -> Json<ImplResult> {
        Json(self.find_implementation_impl(spec, &clause_id))
    }

    #[tool(
        name = "get_figure",
        description = "Fetch a diagram from the KerML or SysML v2 spec by its printed figure number (e.g. 1 for \"Figure 1.\"). Returns the figure's title and owning clause, with the image attached inline so it can be viewed directly. Use get_section's `figures` list to discover figure numbers; entries whose number is null are uncaptioned diagrams, viewable inline via get_section."
    )]
    fn get_figure(
        &self,
        Parameters(FigureQuery {
            spec,
            figure_number,
        }): Parameters<FigureQuery>,
    ) -> Result<CallToolResult, ErrorData> {
        self.index_or_err(spec)?;
        let figure = self.get_figure_impl(spec, figure_number).ok_or_else(|| {
            ErrorData::invalid_params(
                format!("no figure {figure_number} in the {} spec", spec.label()),
                None,
            )
        })?;
        let mut content = vec![json_text_content(&figure)?];
        match image_content(spec, &figure.image_file) {
            Some(img) => content.push(img),
            None => content.push(Content::text(format!(
                "(image file '{}' is not embedded in this build)",
                figure.image_file
            ))),
        }
        Ok(CallToolResult::success(content))
    }

    #[tool(
        name = "follow_link",
        description = "Resolve a cross-reference link found in KerML or SysML v2 spec text: \"#anchor\" links resolve to the clause containing that anchor (then read it with get_section); http(s) URLs are classified and returned as-is."
    )]
    fn follow_link(
        &self,
        Parameters(FollowLinkQuery { spec, link }): Parameters<FollowLinkQuery>,
    ) -> Json<LinkResult> {
        Json(self.follow_link_impl(spec, &link))
    }
}

#[rmcp::tool_handler]
impl rmcp::ServerHandler for SpecServer {
    fn get_info(&self) -> rmcp::model::ServerInfo {
        // Must advertise the tools capability so the client calls tools/list —
        // ServerInfo::default() leaves it unset. Mirror what the tool_handler
        // macro generates, since providing get_info suppresses that codegen.
        rmcp::model::ServerInfo::new(
            rmcp::model::ServerCapabilities::builder()
                .enable_tools()
                .build(),
        )
        .with_instructions(
            "Authoritative oracle for the OMG KerML 1.0 and SysML v2 specifications. \
             Serves the full spec text clause-by-clause, the official textual-notation \
             BNF grammars (.kebnf), and every spec figure as a viewable image — all \
             embedded in the server binary. Every tool takes a `spec` parameter: \
             \"kerml\" for the Kernel Modeling Language (kernel semantics, clauses 7.x; \
             concrete/abstract syntax, clauses 8.x) or \"sysml\" for OMG SysML v2 (the \
             systems-modeling layer built on KerML). Typical flow: spec_toc to \
             see the outline, search_sections to locate a clause, get_section to \
             read it (figures arrive inline as images), get_syntax for grammar \
             productions, find_symbol to map any identifier to its defining clause \
             and grammar production, follow_link to chase cross-references, \
             find_implementation to map a clause to kermlc compiler code. The complete \
             OMG worked-example model corpus (`.kerml` and `.sysml` sources) is embedded \
             too: list_examples lists it, get_example returns any model's full text, so \
             real modeling idioms can be copied instead of inventing syntax.",
        )
    }
}
