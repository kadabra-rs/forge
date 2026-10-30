use spec_mcp::assets::SpecId;
use spec_mcp::index::SearchMode;
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
fn get_section_returns_ancestor_path() {
    let md = "## <span id=\"p1\"></span>**7.3 A**\n\n\
              ### <span id=\"p2\"></span>**7.3.2 B**\n\n\
              #### <span id=\"p3\"></span>**7.3.2.3 C**\n";
    let crates_dir = std::path::PathBuf::from("../../crates");
    let server = SpecServer::with_spec_source(SpecId::Kerml, md, "", crates_dir).unwrap();

    let section = server.get_section_impl(SpecId::Kerml, "7.3.2.3").unwrap();
    assert_eq!(section.id, "7.3.2.3");
    assert_eq!(
        section
            .path
            .iter()
            .map(|c| c.id.as_str())
            .collect::<Vec<_>>(),
        vec!["7.3", "7.3.2"]
    );
}

#[test]
fn get_section_returns_full_chain_on_real_spec() {
    let server = SpecServer::embedded(std::path::PathBuf::from("../../crates")).unwrap();

    let section = server.get_section_impl(SpecId::Kerml, "8.2.4.1.1").unwrap();
    assert_eq!(
        section
            .path
            .iter()
            .map(|c| c.id.as_str())
            .collect::<Vec<_>>(),
        vec!["8", "8.2", "8.2.4", "8.2.4.1"]
    );

    let section = server.get_section_impl(SpecId::Kerml, "8.2.4.2.2").unwrap();
    assert_eq!(
        section
            .path
            .iter()
            .map(|c| c.id.as_str())
            .collect::<Vec<_>>(),
        vec!["8", "8.2", "8.2.4", "8.2.4.2"]
    );
}

#[test]
fn unloaded_spec_returns_nothing() {
    let server = build_test_server();
    assert!(server.get_section_impl(SpecId::Sysml, "7.3.2").is_none());
    assert!(server.spec_toc_impl(SpecId::Sysml, None).is_empty());
    assert!(
        server
            .search_sections_impl(SpecId::Sysml, "types", SearchMode::default())
            .unwrap()
            .is_empty()
    );
}

#[test]
fn spec_toc_nests_and_filters() {
    let md = "## <span id=\"p1\"></span>**7.3 A**\n\n\
              ### <span id=\"p2\"></span>**7.3.2 B**\n\n\
              #### <span id=\"p3\"></span>**7.3.2.1 C**\n";
    let crates_dir = std::path::PathBuf::from("../../crates");
    let server = SpecServer::with_spec_source(SpecId::Kerml, md, "", crates_dir).unwrap();

    let toc = server.spec_toc_impl(SpecId::Kerml, None);
    assert_eq!(toc.len(), 1);
    assert_eq!(toc[0].id, "7.3");
    assert_eq!(toc[0].depth, 1);
    let mid = &toc[0].children;
    assert_eq!(mid.len(), 1);
    assert_eq!(mid[0].id, "7.3.2");
    assert_eq!(mid[0].depth, 2);
    let leaf = &mid[0].children;
    assert_eq!(leaf.len(), 1);
    assert_eq!(leaf[0].id, "7.3.2.1");
    assert_eq!(leaf[0].depth, 3);
    assert!(leaf[0].children.is_empty());

    let shallow = server.spec_toc_impl(SpecId::Kerml, Some(1));
    assert_eq!(shallow.len(), 1);
    assert_eq!(shallow[0].id, "7.3");
    assert!(shallow[0].children.is_empty());
}

#[test]
fn search_sections_reports_match_tier_and_snippet() {
    let md = "## <span id=\"page-51-0\"></span>**7.3.2 Types**\n\nA specialization is asserted between types.\n";
    let crates_dir = std::path::PathBuf::from("../../crates");
    let server = SpecServer::with_spec_source(SpecId::Kerml, md, "", crates_dir).unwrap();

    let title_hits = server
        .search_sections_impl(SpecId::Kerml, "types", SearchMode::default())
        .unwrap();
    assert_eq!(title_hits.len(), 1);
    assert_eq!(title_hits[0].matched, "title");
    assert!(title_hits[0].snippet.is_none());

    let body_hits = server
        .search_sections_impl(SpecId::Kerml, "asserted", SearchMode::default())
        .unwrap();
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
fn search_sections_honours_word_mode() {
    let md = "## <span id=\"page-51-0\"></span>**7.3.2 Types**\n\nA specialized classifier classifies things.\n";
    let crates_dir = std::path::PathBuf::from("../../crates");
    let server = SpecServer::with_spec_source(SpecId::Kerml, md, "", crates_dir).unwrap();

    // "type" only occurs inside "Types"/"types" — no whole word.
    let word_hits = server
        .search_sections_impl(SpecId::Kerml, "type", SearchMode::Word)
        .unwrap();
    assert!(word_hits.is_empty());

    let body_hits = server
        .search_sections_impl(SpecId::Kerml, "classifier", SearchMode::Word)
        .unwrap();
    assert_eq!(body_hits.len(), 1);
    assert_eq!(body_hits[0].matched, "body");
}

#[test]
fn search_sections_reports_figure_matches() {
    let md = "## <span id=\"page-51-0\"></span>**7.3.2 Diagrams**\n\n\
              ![](_page_1_Figure_1.jpeg)\n\n**Figure 1. Widget diagram**\n";
    let crates_dir = std::path::PathBuf::from("../../crates");
    let server = SpecServer::with_spec_source(SpecId::Kerml, md, "", crates_dir).unwrap();

    let hits = server
        .search_sections_impl(SpecId::Kerml, "widget", SearchMode::default())
        .unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].id, "7.3.2");
    assert_eq!(hits[0].matched, "figure");
    assert_eq!(hits[0].figure_number, Some(1));
    assert!(
        hits[0]
            .snippet
            .as_deref()
            .unwrap()
            .contains("Widget diagram")
    );
}

#[test]
fn search_sections_word_mode_on_real_spec() {
    let md = std::fs::read_to_string(
        "../../docs/spec/1-Kernel_Modeling_Language/1-Kernel_Modeling_Language.md",
    )
    .unwrap();
    let crates_dir = std::path::PathBuf::from("../../crates");
    let server = SpecServer::with_spec_source(SpecId::Kerml, &md, "", crates_dir).unwrap();
    let hits = server
        .search_sections_impl(SpecId::Kerml, "conjugates", SearchMode::Word)
        .unwrap();
    assert!(!hits.is_empty(), "KerML prose uses 'conjugates' 42 times");
    assert!(hits.iter().all(|h| h.matched == "body"));
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

    let kerml_ids = server.clause_ids_impl(SpecId::Kerml);
    let sysml_ids = server.clause_ids_impl(SpecId::Sysml);
    assert!(
        kerml_ids.contains(&"7.3.2".to_string()),
        "KerML clause tree"
    );
    assert!(
        sysml_ids.iter().any(|id| id.starts_with("7.")),
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
        section.figures.iter().any(|f| f.number == Some(1)),
        "clause 8.3.1 should list Figure 1"
    );
}

#[test]
fn get_section_lists_uncaptioned_figures() {
    let server = SpecServer::embedded(std::path::PathBuf::from("../../crates")).unwrap();
    let mut hit = None;
    for id in server.clause_ids_impl(SpecId::Sysml) {
        let section = server.get_section_impl(SpecId::Sysml, &id).unwrap();
        if let Some(fig) = section.figures.iter().find(|f| f.number.is_none()) {
            let fig = fig.clone();
            hit = Some((section, fig));
            break;
        }
    }
    let (section, fig) = hit.expect("some SysML clause should list an uncaptioned diagram");
    assert_eq!(fig.title, "", "uncaptioned figures have no title");
    assert!(
        section.body_md.contains(&fig.image_file),
        "the clause body links the uncaptioned image"
    );
    // Images inlined by get_section (first 6) must all be embedded.
    let inline: Vec<_> = section.figures.iter().take(6).collect();
    assert!(!inline.is_empty());
    for f in inline {
        assert!(
            spec_mcp::assets::figure_bytes(SpecId::Sysml, &f.image_file).is_some(),
            "'{}' must be embedded so it can be inlined",
            f.image_file
        );
    }
}

#[test]
fn spec_toc_real_spec_chapters() {
    let server = SpecServer::embedded(std::path::PathBuf::from("../../crates")).unwrap();
    let toc = server.spec_toc_impl(SpecId::Kerml, Some(1));
    assert!(!toc.is_empty());
    for node in &toc {
        assert_eq!(node.depth, 1);
        assert!(node.children.is_empty());
    }
    assert!(
        toc.iter().any(|n| n.id == "7"),
        "KerML top-level outline should include chapter 7, got: {:?}",
        toc.iter().map(|n| n.id.as_str()).collect::<Vec<_>>()
    );

    // The full outline nests by dotted clause id: `8.2.4.1.1` sits under
    // 8 -> 8.2 -> 8.2.4 -> 8.2.4.1 even though its heading is at the same
    // level as its parent's (heading levels are not reliable).
    let full = server.spec_toc_impl(SpecId::Kerml, None);
    let mut cur = full
        .iter()
        .find(|n| n.id == "8")
        .expect("chapter 8 in the KerML outline");
    for (expected, depth) in [("8.2", 2), ("8.2.4", 3), ("8.2.4.1", 4), ("8.2.4.1.1", 5)] {
        let Some(pos) = cur.children.iter().position(|c| c.id == expected) else {
            panic!("{expected} not a child of {}", cur.id);
        };
        assert_eq!(cur.children[pos].depth, depth);
        cur = &cur.children[pos];
    }
}

#[test]
fn find_symbol_maps_identifier_to_grammar_and_prose() {
    let md = "## <span id=\"page-51-0\"></span>**7.3.2 Types**\n\n\
              A feature usage may be isComposite in its owning type.\n";
    let bnf = "// Clause 8.2.4.1.2 Specialization\n\nSpecialization =\n    'specializes'\n\n\
              // Clause 8.2.4.3.1 Features\n\nFeatureDeclaration : Feature =\n    'feature'\n";
    let crates_dir = std::path::PathBuf::from("../../crates");
    let server = SpecServer::with_spec_source(SpecId::Kerml, md, bnf, crates_dir).unwrap();

    // Text tier: the prose mentions isComposite; the grammar tier is empty
    // for it (no production names, keywords, or body ids match).
    let is_composite = server
        .find_symbol_impl(SpecId::Kerml, "isComposite")
        .unwrap();
    assert!(
        !is_composite.text.is_empty(),
        "prose mentioning isComposite should produce a text hit"
    );
    assert!(
        is_composite.text[0].snippet.contains("isComposite"),
        "snippet should contain the matched text: {:?}",
        is_composite.text[0].snippet
    );
    assert!(
        is_composite.grammar.is_empty(),
        "no grammar term matches isComposite: {:?}",
        is_composite.grammar
    );

    // Grammar tier: 'specializes' is a quoted keyword in Specialization;
    // 'feature' is a quoted keyword in FeatureDeclaration.
    let specializes = server
        .find_symbol_impl(SpecId::Kerml, "specializes")
        .unwrap();
    assert!(
        specializes
            .grammar
            .iter()
            .any(|h| h.production == "Specialization" && h.kind == "keyword"),
        "Specialization should quote 'specializes': {:?}",
        specializes.grammar
    );
    assert!(specializes.text.is_empty());

    // Unknown identifier: success with empty tiers, not an error.
    let unknown = server
        .find_symbol_impl(SpecId::Kerml, "no_such_symbol_xyz")
        .unwrap();
    assert!(unknown.grammar.is_empty());
    assert!(unknown.text.is_empty());

    // Blank symbol is a parameter error.
    assert!(server.find_symbol_impl(SpecId::Kerml, "  ").is_err());
}

#[test]
fn find_symbol_on_real_kerml_finds_conjugate() {
    let server = SpecServer::embedded(std::path::PathBuf::from("../../crates")).unwrap();
    let result = server.find_symbol_impl(SpecId::Kerml, "conjugate").unwrap();

    assert!(
        result.grammar.iter().any(|h| h.production == "CONJUGATES"),
        "the CONJUGATES production quotes 'conjugates' (a partial key match); got: {:?}",
        result
            .grammar
            .iter()
            .map(|h| &h.production)
            .collect::<Vec<_>>()
    );
    // 'conjugate' is an exact quoted literal in the Conjugation production
    // (and in RESERVED_KEYWORD); it must appear as a keyword hit.
    assert!(
        result
            .grammar
            .iter()
            .any(|h| h.production == "Conjugation" && h.kind == "keyword"),
        "Conjugation should quote 'conjugate': {:?}",
        result.grammar
    );

    // The prose mentions conjugation concepts (42 occurrences verified in
    // the KerML corpus).
    assert!(
        !result.text.is_empty(),
        "KerML prose should mention conjugate"
    );
}

#[test]
fn find_symbol_on_real_sysml_finds_port_usage() {
    let server = SpecServer::embedded(std::path::PathBuf::from("../../crates")).unwrap();
    let result = server.find_symbol_impl(SpecId::Sysml, "PortUsage").unwrap();

    // Exact production-name hit, plus supertype references in the port
    // productions that inherit from PortUsage.
    assert!(
        result
            .grammar
            .iter()
            .any(|h| h.production == "PortUsage" && h.kind == "definition"),
        "PortUsage should define itself: {:?}",
        result.grammar
    );
    assert!(
        result
            .grammar
            .iter()
            .any(|h| h.kind == "reference" && h.supertype.as_deref() == Some("PortUsage")),
        "productions declared `: PortUsage` reference it: {:?}",
        result.grammar
    );
    assert!(
        !result.text.is_empty(),
        "SysML prose mentions PortUsage (58 occurrences)"
    );
}
