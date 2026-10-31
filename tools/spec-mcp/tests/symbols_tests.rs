use spec_mcp::index::build_index;
use spec_mcp::symbols::SymbolKind;

const BNF: &str = "\
// Clause 8.2.4.1.2 Specialization

Specialization =
    'specializes'

// Clause 8.2.4.3.1 Features

FeatureDeclaration : Feature =
    'feature'
";

fn all(sym: &str) -> Vec<(String, SymbolKind)> {
    let idx = build_index("", BNF).unwrap();
    let prods = idx.productions();
    let (exact, partial) = idx.symbols().lookup(sym, prods);
    exact
        .iter()
        .chain(partial.iter())
        .map(|(p, occ)| (p.name.clone(), occ.kind))
        .collect()
}

#[test]
fn keyword_literal_is_indexed_as_keyword() {
    let pairs = all("specializes");
    assert!(
        pairs
            .iter()
            .any(|(name, kind)| name == "Specialization" && kind == &SymbolKind::Keyword),
        "'specializes' is a quoted literal in Specialization; pairs={pairs:?}"
    );
}

#[test]
fn production_name_is_a_definition_and_supertype_a_reference() {
    let pairs = all("Feature");
    assert!(
        pairs
            .iter()
            .any(|(name, kind)| name == "FeatureDeclaration" && kind == &SymbolKind::Definition),
        "FeatureDeclaration defines its own name; pairs={pairs:?}"
    );
    assert!(
        pairs
            .iter()
            .any(|(name, kind)| name == "FeatureDeclaration" && kind == &SymbolKind::Reference),
        "the declared supertype Feature is referenced; pairs={pairs:?}"
    );
    assert!(
        pairs
            .iter()
            .any(|(name, kind)| name == "FeatureDeclaration" && kind == &SymbolKind::Keyword),
        "'feature' is a quoted literal in FeatureDeclaration; pairs={pairs:?}"
    );
}

#[test]
fn body_identifier_is_a_reference() {
    let src = "\
// Clause 8.2.2.1 Line Terminators

WHITE_SPACE =
    ' ' | '\\t' | LINE_TERMINATOR
";
    let idx = build_index("", src).unwrap();
    let prods = idx.productions();
    let (exact, partial) = idx.symbols().lookup("line_terminator", prods);
    let pairs: Vec<(String, SymbolKind)> = exact
        .iter()
        .chain(partial.iter())
        .map(|(p, occ)| (p.name.clone(), occ.kind))
        .collect();
    assert!(
        pairs
            .iter()
            .any(|(name, kind)| name == "WHITE_SPACE" && kind == &SymbolKind::Reference),
        "LINE_TERMINATOR referenced in the WHITE_SPACE body; pairs={pairs:?}"
    );
}
