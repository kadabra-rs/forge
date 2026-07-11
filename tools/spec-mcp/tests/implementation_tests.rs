use spec_mcp::implementation::{CodeSignal, ImplStatus, derive_signals, find_implementation};

#[test]
fn derives_parse_method_name_from_production() {
    let signals = derive_signals("8.2.4.3.1", &["FeatureDeclaration"]);
    assert!(
        signals
            .iter()
            .any(|s| matches!(s, CodeSignal::ParserMethod(m) if m == "parse_feature_decl"))
    );
}

#[test]
fn derives_token_kind_from_lexical_production() {
    let signals = derive_signals("8.2.2.2", &["SINGLE_LINE_NOTE", "MULTILINE_NOTE"]);
    assert!(
        signals
            .iter()
            .any(|s| matches!(s, CodeSignal::TokenKind(t) if t == "Comment"))
    );
}

#[test]
fn classifies_implemented_when_signal_found() {
    let crates_dir = std::path::PathBuf::from("../../crates");
    let status = find_implementation("8.2.4.3.1", &["FeatureDeclaration"], &crates_dir).unwrap();
    assert_eq!(
        status.status,
        ImplStatus::Implemented,
        "feature decl should be implemented: {:?}",
        status.entry_points
    );
    assert!(
        status
            .entry_points
            .iter()
            .any(|e| e.contains("parse_feature_decl"))
    );
}

#[test]
fn classifies_not_started_when_signal_absent() {
    let crates_dir = std::path::PathBuf::from("../../crates");
    let status = find_implementation("8.2.5.4", &["AssociationDeclaration"], &crates_dir).unwrap();
    assert_eq!(
        ImplStatus::NotStarted,
        status.status,
        "association decl should be not-started: {:?}",
        status.entry_points
    );
}

#[test]
fn classifies_out_of_scope_when_no_signals() {
    let crates_dir = std::path::PathBuf::from("../../crates");
    let status = find_implementation("8.4.3.1", &[], &crates_dir).unwrap();
    assert_eq!(status.status, ImplStatus::OutOfScope);
}
