use spec_mcp::assets::SpecId;
use spec_mcp::tools::SpecServer;

fn build_test_server() -> SpecServer {
    let md = "## <span id=\"page-51-0\"></span>**7.3.2 Types**\n\n## <span id=\"page-52-0\"></span>**7.3.2.3 Specialization**\n";
    let bnf = "// Clause 8.2.4.1.2 Specialization\n\nSpecialization =\n    'specializes'\n\n// Clause 8.2.4.3.1 Features\n\nFeatureDeclaration : Feature =\n    'feature'\n";
    let crates_dir = std::path::PathBuf::from("../../crates");
    SpecServer::with_spec_source(SpecId::Kerml, md, bnf, crates_dir).unwrap()
}

#[test]
fn constructs_server_from_synthetic_spec() {
    let _ = build_test_server();
}

#[test]
fn get_section_returns_title_and_anchor() {
    let server = build_test_server();
    let result = server.get_section_impl(SpecId::Kerml, "7.3.2").unwrap();
    assert_eq!(result.title, "Types");
    assert_eq!(result.anchor, "page-51-0");
    assert_eq!(result.spec, "kerml");
    assert!(result.productions.is_empty());
}

#[test]
fn get_section_returns_none_for_missing_clause() {
    let server = build_test_server();
    assert!(server.get_section_impl(SpecId::Kerml, "99.99").is_none());
}

#[test]
fn unloaded_spec_returns_nothing() {
    let server = build_test_server();
    assert!(server.get_section_impl(SpecId::Sysml, "7.3.2").is_none());
    assert!(server.list_sections_impl(SpecId::Sysml).is_empty());
    assert!(
        server
            .search_sections_impl(SpecId::Sysml, "types")
            .is_empty()
    );
}

#[test]
fn list_sections_returns_all_clauses() {
    let server = build_test_server();
    let clauses = server.list_sections_impl(SpecId::Kerml);
    assert_eq!(clauses.len(), 2);
    assert!(clauses.iter().any(|c| c.id == "7.3.2"));
    assert!(clauses.iter().any(|c| c.id == "7.3.2.3"));
}

#[test]
fn search_sections_reports_match_tier_and_snippet() {
    let md = "## <span id=\"page-51-0\"></span>**7.3.2 Types**\n\nA specialization is asserted between types.\n";
    let crates_dir = std::path::PathBuf::from("../../crates");
    let server = SpecServer::with_spec_source(SpecId::Kerml, md, "", crates_dir).unwrap();

    let title_hits = server.search_sections_impl(SpecId::Kerml, "types");
    assert_eq!(title_hits.len(), 1);
    assert_eq!(title_hits[0].matched, "title");
    assert!(title_hits[0].snippet.is_none());

    let body_hits = server.search_sections_impl(SpecId::Kerml, "asserted");
    assert_eq!(body_hits.len(), 1);
    assert_eq!(body_hits[0].id, "7.3.2");
    assert_eq!(body_hits[0].matched, "body");
    assert!(
        body_hits[0]
            .snippet
            .as_deref()
            .unwrap()
            .contains("asserted")
    );
}

#[test]
fn find_implementation_returns_status() {
    let server = build_test_server();
    let result = server.find_implementation_impl(SpecId::Kerml, "8.2.4.3.1");
    assert_eq!(result.status, "Implemented");
    assert!(
        result
            .entry_points
            .iter()
            .any(|e| e.contains("parse_feature_decl"))
    );
}

#[test]
fn get_section_includes_productions() {
    let md = "## <span id=\"page-51-0\"></span>**8.2.4.1.2 Specialization**\n";
    let bnf = "// Clause 8.2.4.1.2 Specialization\n\nSpecialization =\n    'specializes'\n";
    let crates_dir = std::path::PathBuf::from("../../crates");
    let server = SpecServer::with_spec_source(SpecId::Kerml, md, bnf, crates_dir).unwrap();
    let result = server.get_section_impl(SpecId::Kerml, "8.2.4.1.2").unwrap();
    assert!(result.productions.contains(&"Specialization".to_string()));
}

#[test]
fn get_syntax_returns_production_bodies() {
    let server = build_test_server();
    let hits = server.get_syntax_impl(SpecId::Kerml, "FeatureDeclaration");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].name, "FeatureDeclaration");
    assert_eq!(hits[0].clause_id, "8.2.4.3.1");
    assert!(hits[0].body.contains("'feature'"));
}

#[test]
fn get_syntax_substring_match_ranks_exact_first() {
    let server = build_test_server();
    let hits = server.get_syntax_impl(SpecId::Kerml, "specialization");
    assert_eq!(hits[0].name, "Specialization");
}

#[test]
fn follow_anchor_link_resolves_to_clause() {
    let server = build_test_server();
    let result = server.follow_link_impl(SpecId::Kerml, "#page-51-0");
    assert_eq!(result.kind, "section");
    assert_eq!(result.clause_id.as_deref(), Some("7.3.2"));
    assert_eq!(result.title.as_deref(), Some("Types"));
}

#[test]
fn follow_anchor_link_on_figure_resolves_to_enclosing_clause() {
    let md = std::fs::read_to_string(
        "../../docs/spec/1-Kernel_Modeling_Language/1-Kernel_Modeling_Language.md",
    )
    .unwrap();
    let crates_dir = std::path::PathBuf::from("../../crates");
    let server = SpecServer::with_spec_source(SpecId::Kerml, &md, "", crates_dir).unwrap();
    let result = server.follow_link_impl(SpecId::Kerml, "#page-131-2");
    assert_eq!(result.kind, "section");
    assert_eq!(result.clause_id.as_deref(), Some("8.3.1"));
    assert_eq!(result.title.as_deref(), Some("Abstract Syntax Overview"));
}

#[test]
fn follow_anchor_link_not_found() {
    let server = build_test_server();
    let result = server.follow_link_impl(SpecId::Kerml, "#nonexistent");
    assert_eq!(result.kind, "anchor_not_found");
    assert!(result.error.is_some());
}

#[test]
fn get_figure_from_real_spec() {
    let md = std::fs::read_to_string(
        "../../docs/spec/1-Kernel_Modeling_Language/1-Kernel_Modeling_Language.md",
    )
    .unwrap();
    let crates_dir = std::path::PathBuf::from("../../crates");
    let server = SpecServer::with_spec_source(SpecId::Kerml, &md, "", crates_dir).unwrap();
    let fig = server.get_figure_impl(SpecId::Kerml, 1).unwrap();
    assert_eq!(fig.number, 1);
    assert_eq!(fig.title, "KerML Syntax Layers");
    assert_eq!(fig.clause_id, "8.3.1");
    assert!(fig.image_file.ends_with(".jpeg"));
}

#[test]
fn embedded_server_indexes_both_specs() {
    let server = SpecServer::embedded(std::path::PathBuf::from("../../crates")).unwrap();

    let kerml = server.list_sections_impl(SpecId::Kerml);
    let sysml = server.list_sections_impl(SpecId::Sysml);
    assert!(kerml.iter().any(|c| c.id == "7.3.2"), "KerML clause tree");
    assert!(
        sysml.iter().any(|c| c.id.starts_with("7.")),
        "SysML clause tree"
    );

    // Each spec answers from its own grammar.
    let kerml_syntax = server.get_syntax_impl(SpecId::Kerml, "FeatureDeclaration");
    assert!(!kerml_syntax.is_empty(), "KerML BNF has FeatureDeclaration");
    let sysml_syntax = server.get_syntax_impl(SpecId::Sysml, "PartDefinition");
    assert!(!sysml_syntax.is_empty(), "SysML BNF has PartDefinition");
}

#[test]
fn embedded_figures_resolve_to_image_bytes() {
    let server = SpecServer::embedded(std::path::PathBuf::from("../../crates")).unwrap();
    for spec in SpecId::ALL {
        let fig = server.get_figure_impl(spec, 1).unwrap();
        let bytes = spec_mcp::assets::figure_bytes(spec, &fig.image_file)
            .unwrap_or_else(|| panic!("figure 1 image missing for {}", spec.label()));
        // JPEG magic bytes.
        assert_eq!(
            &bytes[..2],
            &[0xFF, 0xD8],
            "not a JPEG for {}",
            spec.label()
        );
    }
}

#[test]
fn get_section_includes_unnumbered_sibling_subsections() {
    let md = std::fs::read_to_string(
        "../../docs/spec/1-Kernel_Modeling_Language/1-Kernel_Modeling_Language.md",
    )
    .unwrap();
    let crates_dir = std::path::PathBuf::from("../../crates");
    let server = SpecServer::with_spec_source(SpecId::Kerml, &md, "", crates_dir).unwrap();
    // Clause 8.3.3.3.4 Feature's content lives under same-level unnumbered
    // headings (Description, Attributes, ...) that follow its own heading.
    let section = server.get_section_impl(SpecId::Kerml, "8.3.3.3.4").unwrap();
    assert_eq!(section.title, "Feature");
    assert!(
        section.body_md.contains("isComposite"),
        "body should include the Attributes subsection"
    );
}

#[test]
fn get_section_lists_figures_for_clause() {
    let md = std::fs::read_to_string(
        "../../docs/spec/1-Kernel_Modeling_Language/1-Kernel_Modeling_Language.md",
    )
    .unwrap();
    let crates_dir = std::path::PathBuf::from("../../crates");
    let server = SpecServer::with_spec_source(SpecId::Kerml, &md, "", crates_dir).unwrap();
    let section = server.get_section_impl(SpecId::Kerml, "8.3.1").unwrap();
    assert!(
        section.figures.iter().any(|f| f.number == 1),
        "clause 8.3.1 should list Figure 1"
    );
}
