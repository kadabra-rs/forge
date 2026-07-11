use spec_mcp::index::build_index;

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
    assert!(
        figures.len() >= 30,
        "expected >=30 figures, got {}",
        figures.len()
    );
    let fig1 = figures.iter().find(|f| f.number == 1).unwrap();
    assert_eq!(fig1.title, "KerML Syntax Layers");
    assert_eq!(fig1.clause_id, "8.3.1");
    let figs_for_clause = idx.figures_for_clause("8.3.1");
    assert!(!figs_for_clause.is_empty(), "8.3.1 should have figures");
}
