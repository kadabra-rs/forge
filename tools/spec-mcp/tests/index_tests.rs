use spec_mcp::index::build_index;

/// Counts image links directly followed (within a small window, optionally
/// through a Markdown heading marker) by a `**Figure N.` caption — the same
/// notion the index implements, derived independently from the Markdown.
fn expected_captioned(md: &str) -> usize {
    let mut count = 0;
    let mut from = 0;
    while let Some(off) = md[from..].find("![](_page_") {
        let i = from + off;
        let link_end = md[i..].find(')').map(|j| i + j + 1).unwrap_or(md.len());
        let window: String = md[link_end..].chars().take(80).collect();
        if window.contains("**Figure ") {
            count += 1;
        }
        from = link_end;
    }
    count
}
#[test]
fn index_joins_clauses_and_productions() {
    let md = "## <span id=\"page-51-0\"></span>**7.3.2 Types**\n\n## <span id=\"page-52-0\"></span>**7.3.2.3 Specialization**\n";
    let bnf = "// Clause 8.2.4.1.2 Specialization\n\nSpecialization =\n    'specializes'\n";
    let idx = build_index(md, bnf).unwrap();

    let clauses = idx.list_clauses();
    assert!(clauses.iter().any(|c| c.id == "7.3.2"));
    assert!(clauses.iter().any(|c| c.id == "7.3.2.3"));

    let prods = idx.productions_for_clause("8.2.4.1.2");
    assert_eq!(prods.len(), 1);
    assert_eq!(prods[0].name, "Specialization");
}

#[test]
fn index_lookups_clause_by_id() {
    let md = "## <span id=\"page-51-0\"></span>**7.3.2 Types**\n";
    let idx = build_index(md, "").unwrap();
    let clause = idx.clause("7.3.2").unwrap();
    assert_eq!(clause.title, "Types");
    assert_eq!(clause.anchor, "page-51-0");
}

#[test]
fn index_returns_body_text_for_clause() {
    let md = "## <span id=\"page-51-0\"></span>**7.3.2 Types**\n\nBody content here.\n\n## <span id=\"page-52-0\"></span>**7.3.3 Next**\n";
    let idx = build_index(md, "").unwrap();
    let body = idx.body_for_clause("7.3.2").unwrap();
    assert!(body.contains("Body content here."));
    assert!(!body.contains("7.3.3"));
}

#[test]
fn search_matches_titles_before_bodies() {
    let md = "## <span id=\"a\"></span>**7.1 Types**\n\nA type classifies things.\n\n## <span id=\"b\"></span>**7.2 Features**\n\nFeatures relate types to types to types.\n";
    let idx = build_index(md, "").unwrap();
    let hits = idx.search("types");
    assert_eq!(hits.len(), 2);
    assert_eq!(hits[0].clause.id, "7.1");
    assert!(hits[0].snippet.is_none(), "title match has no snippet");
    assert_eq!(hits[1].clause.id, "7.2");
    assert!(
        hits[1].snippet.as_deref().unwrap().contains("types"),
        "body match carries a snippet"
    );
}

#[test]
fn search_body_requires_all_tokens() {
    let md = "## <span id=\"a\"></span>**7.1 Alpha**\n\nowned redefinition here\n\n## <span id=\"b\"></span>**7.2 Beta**\n\nonly redefinition here\n";
    let idx = build_index(md, "").unwrap();
    let hits = idx.search("owned redefinition");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].clause.id, "7.1");
}

#[test]
fn search_does_not_match_ancestors_on_subclause_text() {
    let md = "## <span id=\"a\"></span>**7.1 Parent**\n\nParent prose.\n\n### <span id=\"b\"></span>**7.1.1 Child**\n\nThe isComposite flag.\n";
    let idx = build_index(md, "").unwrap();
    let hits = idx.search("iscomposite");
    assert_eq!(hits.len(), 1, "only the child clause should match");
    assert_eq!(hits[0].clause.id, "7.1.1");
}

#[test]
fn search_empty_query_returns_nothing() {
    let md = "## <span id=\"a\"></span>**7.1 Types**\n\nBody.\n";
    let idx = build_index(md, "").unwrap();
    assert!(idx.search("").is_empty());
    assert!(idx.search("   ").is_empty());
}

#[test]
fn search_real_spec_finds_body_only_terms() {
    let md = std::fs::read_to_string(
        "../../docs/spec/1-Kernel_Modeling_Language/1-Kernel_Modeling_Language.md",
    )
    .unwrap();
    let idx = build_index(&md, "").unwrap();
    // "isComposite" appears in clause bodies but in no clause title.
    let hits = idx.search("isComposite");
    assert!(!hits.is_empty(), "body search should find isComposite");
    assert!(hits.iter().all(|h| h.snippet.is_some()));
    assert!(hits.len() <= 20, "body matches are capped");
}

#[test]
fn builds_index_from_real_spec_and_bnf() {
    let md = std::fs::read_to_string(
        "../../docs/spec/1-Kernel_Modeling_Language/1-Kernel_Modeling_Language.md",
    )
    .unwrap();
    let bnf = std::fs::read_to_string("../../vendor/SysML-v2-Release/bnf/KerML-textual-bnf.kebnf")
        .unwrap();
    let idx = build_index(&md, &bnf).unwrap();
    assert!(
        idx.list_clauses().len() > 400,
        "expected >400 clauses, got {}",
        idx.list_clauses().len()
    );
    assert!(idx.productions().len() > 200);
    assert!(
        idx.clause("8.2.4.3.1").is_some(),
        "8.2.4.3.1 should be a clause"
    );
    assert!(
        !idx.productions_for_clause("8.2.4.3.1").is_empty(),
        "8.2.4.3.1 should have productions"
    );
    let body = idx.body_for_clause("8.2.4.3.1").unwrap();
    assert!(!body.is_empty(), "8.2.4.3.1 should have body text");
}

#[test]
fn extracts_figures_from_real_spec() {
    let md = std::fs::read_to_string(
        "../../docs/spec/1-Kernel_Modeling_Language/1-Kernel_Modeling_Language.md",
    )
    .unwrap();
    let idx = build_index(&md, "").unwrap();
    let figures = idx.figures();
    assert_eq!(
        figures.len(),
        md.matches("![](_page_").count(),
        "one entry per embedded image link"
    );
    let fig1 = figures.iter().find(|f| f.number == Some(1)).unwrap();
    assert_eq!(fig1.title, "KerML Syntax Layers");
    assert_eq!(fig1.clause_id, "8.3.1");
    let figs_for_clause = idx.figures_for_clause("8.3.1");
    assert!(!figs_for_clause.is_empty(), "8.3.1 should have figures");
}

#[test]
fn indexes_every_embedded_image_link() {
    use spec_mcp::assets::{SpecId, spec_md};
    for spec in [SpecId::Kerml, SpecId::Sysml] {
        let md = spec_md(spec);
        let idx = build_index(md, "").unwrap();
        let links = md.matches("![](_page_").count();
        assert_eq!(
            idx.figures().len(),
            links,
            "{} should index every embedded image link",
            spec.label()
        );
        let numbered = idx.figures().iter().filter(|f| f.number.is_some()).count();
        assert_eq!(
            numbered,
            expected_captioned(md),
            "{} captioned figures should keep their number",
            spec.label()
        );
        assert!(
            idx.figures()
                .iter()
                .filter(|f| f.number.is_none())
                .all(|f| f.title.is_empty()),
            "{} uncaptioned figures should have an empty title",
            spec.label()
        );
        let unique: std::collections::HashSet<&str> = idx
            .figures()
            .iter()
            .map(|f| f.image_path.as_str())
            .collect();
        assert_eq!(
            unique.len(),
            links,
            "{} image files should be distinct",
            spec.label()
        );
    }
}

#[test]
fn indexed_figures_resolve_to_embedded_bytes() {
    use spec_mcp::assets::{SpecId, figure_bytes, spec_md};
    for spec in SpecId::ALL {
        let idx = build_index(spec_md(spec), "").unwrap();
        for f in idx.figures() {
            let bytes = figure_bytes(spec, &f.image_path).unwrap_or_else(|| {
                panic!("{}: image '{}' is not embedded", spec.label(), f.image_path)
            });
            assert_eq!(
                &bytes[..2],
                &[0xFF, 0xD8],
                "{}: '{}' is not a JPEG",
                spec.label(),
                f.image_path
            );
        }
    }
}

#[test]
fn uncaptioned_figures_are_indexed_without_number() {
    use spec_mcp::assets::{SpecId, spec_md};
    let idx = build_index(spec_md(SpecId::Sysml), "").unwrap();
    let fig = idx
        .figures()
        .iter()
        .find(|f| f.image_path == "_page_107_Figure_0.jpeg")
        .expect("uncaptioned SysML diagram should be indexed");
    assert_eq!(fig.number, None);
    assert_eq!(fig.title, "");
    assert!(
        !fig.clause_id.is_empty(),
        "uncaptioned figure should still be attributed to a clause"
    );
    assert!(
        idx.figures_for_clause(&fig.clause_id)
            .iter()
            .any(|f| f.image_path == fig.image_path),
        "figures_for_clause should include the uncaptioned figure"
    );
}

#[test]
fn captioned_figures_keep_number_and_title() {
    use spec_mcp::assets::{SpecId, spec_md};
    for (spec, title) in [
        (SpecId::Kerml, "KerML Syntax Layers"),
        (SpecId::Sysml, "SysML Language Architecture"),
    ] {
        let idx = build_index(spec_md(spec), "").unwrap();
        let fig = idx
            .figures()
            .iter()
            .find(|f| f.number == Some(1))
            .unwrap_or_else(|| panic!("{} should have a captioned Figure 1", spec.label()));
        assert_eq!(fig.title, title);
        assert!(!fig.image_path.is_empty());
    }
}

#[test]
fn heading_form_caption_attaches() {
    use spec_mcp::assets::{SpecId, spec_md};
    let idx = build_index(spec_md(SpecId::Sysml), "").unwrap();
    let f63 = idx
        .figures()
        .iter()
        .find(|f| f.number == Some(63))
        .expect("heading-form caption (# **Figure 63. …**) attaches");
    assert_eq!(f63.title, "Variant engine4Cyl");
    assert_eq!(f63.image_path, "_page_673_Figure_1.jpeg");
}
