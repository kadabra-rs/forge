use spec_mcp::bnf::parse_bnf;

const SAMPLE: &str = "\
// Clause 8.2.2.1 Line Terminators and White Space

LINE_TERMINATOR =
    '\\n' | '\\r' | '\\r\\n'

WHITE_SPACE =
    ' ' | '\\t' | '\\f' | LINE_TERMINATOR

// Clause 8.2.4.3.1 Features

Feature =
    'feature' FeatureDeclaration?
";

#[test]
fn parses_clause_markers_and_productions() {
    let productions = parse_bnf(SAMPLE).unwrap();
    let names: Vec<&str> = productions.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["LINE_TERMINATOR", "WHITE_SPACE", "Feature"]);

    let line_term = &productions[0];
    assert_eq!(line_term.clause_id, "8.2.2.1");
    assert_eq!(line_term.body.trim(), "'\\n' | '\\r' | '\\r\\n'");

    let feature = &productions[2];
    assert_eq!(feature.clause_id, "8.2.4.3.1");
    assert_eq!(feature.body.trim(), "'feature' FeatureDeclaration?");
}

#[test]
fn assigns_productions_to_most_recent_clause_marker() {
    let productions = parse_bnf(SAMPLE).unwrap();
    assert_eq!(productions[0].clause_id, "8.2.2.1");
    assert_eq!(productions[1].clause_id, "8.2.2.1");
    assert_eq!(productions[2].clause_id, "8.2.4.3.1");
}

#[test]
fn parses_inheritance_style_production() {
    let src = "\
// Clause 8.2.3.1 Test

FeatureDeclaration : Feature =
    'feature' FeatureName?
";
    let productions = parse_bnf(src).unwrap();
    assert_eq!(productions.len(), 1);
    assert_eq!(productions[0].name, "FeatureDeclaration");
    assert_eq!(productions[0].clause_id, "8.2.3.1");
    assert_eq!(productions[0].body.trim(), "'feature' FeatureName?");
}

#[test]
fn skips_regular_comments_between_productions() {
    let src = "\
// Clause 8.2.2.2 Notes

// This is a regular comment, not a clause marker
SINGLE_LINE_NOTE =
    '//' LINE_TEXT

// Notes:
//   1. Some note text.
MULTILINE_NOTE =
    '//*' COMMENT_TEXT '*/'
";
    let productions = parse_bnf(src).unwrap();
    let names: Vec<&str> = productions.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["SINGLE_LINE_NOTE", "MULTILINE_NOTE"]);
    assert_eq!(productions[0].clause_id, "8.2.2.2");
    assert_eq!(productions[1].clause_id, "8.2.2.2");
}

#[test]
fn parses_real_kerml_bnf_without_error() {
    let src = std::fs::read_to_string("../../vendor/SysML-v2-Release/bnf/KerML-textual-bnf.kebnf")
        .expect("read kebnf");
    let productions = parse_bnf(&src).unwrap();
    assert!(
        productions.len() > 200,
        "expected ~290 productions, got {}",
        productions.len()
    );
    assert_eq!(productions[0].name, "LINE_TERMINATOR");
    assert_eq!(productions[0].clause_id, "8.2.2.1");
    let feature = productions
        .iter()
        .find(|p| p.name == "Feature")
        .expect("Feature production");
    assert_eq!(feature.clause_id, "8.2.4.3.1");
    let feat_decl = productions
        .iter()
        .find(|p| p.name == "FeatureDeclaration")
        .expect("FeatureDeclaration production");
    assert_eq!(feat_decl.clause_id, "8.2.4.3.1");
}
