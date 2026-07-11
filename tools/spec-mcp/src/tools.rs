use anyhow::Result;
use std::collections::HashMap;
use std::path::PathBuf;

use base64::Engine as _;
use rmcp::handler::server::wrapper::{Json, Parameters};
use rmcp::model::{CallToolResult, Content, ErrorData};
use rmcp::tool;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::assets::{self, SpecId};
use crate::implementation::find_implementation;
use crate::index::{SpecIndex, build_index};

/// Sections can reference many figures; inline at most this many images per
/// get_section response. All figures are still listed in the JSON metadata.
const INLINE_FIGURE_LIMIT: usize = 6;

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

    pub fn list_sections_impl(&self, spec: SpecId) -> Vec<ClauseSummary> {
        let Some(index) = self.index(spec) else {
            return vec![];
        };
        index
            .list_clauses()
            .iter()
            .map(|c| ClauseSummary {
                id: c.id.clone(),
                title: c.title.clone(),
                anchor: c.anchor.clone(),
            })
            .collect()
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
            body_md: body,
            productions: productions.iter().map(|p| p.name.clone()).collect(),
            figures,
        })
    }

    pub fn search_sections_impl(&self, spec: SpecId, query: &str) -> Vec<ClauseSummary> {
        let Some(index) = self.index(spec) else {
            return vec![];
        };
        let q = query.to_lowercase();
        index
            .list_clauses()
            .iter()
            .filter(|c| c.id.contains(&q) || c.title.to_lowercase().contains(&q))
            .map(|c| ClauseSummary {
                id: c.id.clone(),
                title: c.title.clone(),
                anchor: c.anchor.clone(),
            })
            .collect()
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

    pub fn get_figure_impl(&self, spec: SpecId, figure_number: u32) -> Option<FigureResult> {
        let index = self.index(spec)?;
        let fig = index.figures().iter().find(|f| f.number == figure_number)?;
        Some(FigureResult {
            spec: spec.label().to_string(),
            number: fig.number,
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

#[derive(Clone, Serialize, Deserialize, JsonSchema)]
pub struct ClauseSummary {
    pub id: String,
    pub title: String,
    pub anchor: String,
}

#[derive(Clone, Serialize, Deserialize, JsonSchema)]
pub struct SpecOnlyQuery {
    /// Which spec to query: "kerml" or "sysml".
    pub spec: SpecId,
}

#[derive(Clone, Serialize, Deserialize, JsonSchema)]
pub struct SectionQuery {
    /// Which spec to query: "kerml" or "sysml".
    pub spec: SpecId,
    /// Clause number as written in the spec, e.g. "7.3.2" or "8.2.4.1.2".
    pub clause_id: String,
}

#[derive(Clone, Serialize, Deserialize, JsonSchema)]
pub struct ClauseListResult {
    pub clauses: Vec<ClauseSummary>,
}

#[derive(Clone, Serialize, Deserialize, JsonSchema)]
pub struct FigureSummary {
    pub number: u32,
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
    /// Case-insensitive keyword matched against clause ids and titles.
    pub query: String,
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
        name = "list_sections",
        description = "List the complete clause tree (clause id, title, anchor) of an OMG spec. Set spec to \"kerml\" (KerML 1.0, kernel language) or \"sysml\" (OMG SysML v2, systems layer). Use this to orient yourself before drilling into individual clauses with get_section."
    )]
    fn list_sections(
        &self,
        Parameters(SpecOnlyQuery { spec }): Parameters<SpecOnlyQuery>,
    ) -> Result<Json<ClauseListResult>, ErrorData> {
        self.index_or_err(spec)?;
        Ok(Json(ClauseListResult {
            clauses: self.list_sections_impl(spec),
        }))
    }

    #[tool(
        name = "get_section",
        description = "Read one clause of the KerML or SysML v2 spec by clause id (e.g. \"7.3.2\", \"8.2.4.1.2\"). Returns the clause body as Markdown, the BNF production names it defines, and its figures — the figure images are attached to the result inline so diagrams can be viewed directly. Relative image links in the body (e.g. _page_131_Figure_3.jpeg) map to the entries in the `figures` list."
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
        name = "search_sections",
        description = "Search clause ids and titles of the KerML or SysML v2 spec by case-insensitive keyword (e.g. \"specialization\", \"redefinition\", \"8.3.4\"). Returns matching clause ids to feed into get_section. Set spec to \"kerml\" or \"sysml\" to choose which spec to search."
    )]
    fn search_sections(
        &self,
        Parameters(SearchQuery { spec, query }): Parameters<SearchQuery>,
    ) -> Result<Json<ClauseListResult>, ErrorData> {
        self.index_or_err(spec)?;
        Ok(Json(ClauseListResult {
            clauses: self.search_sections_impl(spec, &query),
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
        description = "Fetch a diagram from the KerML or SysML v2 spec by its printed figure number (e.g. 1 for \"Figure 1.\"). Returns the figure's title and owning clause, with the image attached inline so it can be viewed directly. Use get_section's `figures` list to discover figure numbers."
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
             systems-modeling layer built on KerML). Typical flow: search_sections to \
             locate a clause, get_section to read it (figures arrive inline as images), \
             get_syntax for grammar productions, follow_link to chase cross-references, \
             find_implementation to map a clause to kermlc compiler code.",
        )
    }
}
