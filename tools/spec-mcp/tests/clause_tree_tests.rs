use spec_mcp::clause_tree::extract_clause_tree;

const FRAGMENT: &str = "\
# **Kernel Modeling Language**

## <span id=\"page-26-0\"></span>**1 Scope**

### <span id=\"page-36-1\"></span>**6.1 Language Architecture**

#### <span id=\"page-40-4\"></span>**7.2.2 Elements and Relationships**

## <span id=\"page-98-0\"></span>**8 Metamodel**

#### **8.3.2.1.1 Overview**
";

#[test]
fn extracts_numbered_headings_with_anchors() {
    let tree = extract_clause_tree(FRAGMENT).unwrap();
    let ids: Vec<&str> = tree.iter().map(|n| n.id.as_str()).collect();
    assert!(ids.contains(&"1"));
    assert!(ids.contains(&"6.1"));
    assert!(ids.contains(&"7.2.2"));
    assert!(ids.contains(&"8"));
    assert!(ids.contains(&"8.3.2.1.1"));
}

#[test]
fn captures_anchor_id_from_span() {
    let tree = extract_clause_tree(FRAGMENT).unwrap();
    let scope = tree.iter().find(|n| n.id == "1").unwrap();
    assert_eq!(scope.anchor, "page-26-0");
    let metamodel = tree.iter().find(|n| n.id == "8").unwrap();
    assert_eq!(metamodel.anchor, "page-98-0");
}

#[test]
fn anchor_empty_when_no_span() {
    let tree = extract_clause_tree(FRAGMENT).unwrap();
    let overview = tree.iter().find(|n| n.id == "8.3.2.1.1").unwrap();
    assert_eq!(overview.anchor, "");
}

#[test]
fn captures_line_number_level_and_title() {
    let tree = extract_clause_tree(FRAGMENT).unwrap();
    let la = tree.iter().find(|n| n.id == "6.1").unwrap();
    assert_eq!(la.title, "Language Architecture");
    assert_eq!(la.level, 3);
    assert!(la.line > 0);
}

#[test]
fn ignores_non_numbered_headings() {
    let tree = extract_clause_tree(FRAGMENT).unwrap();
    assert!(!tree.iter().any(|n| n.title.contains("Kernel Modeling")));
}

#[test]
fn body_range_covers_section_content() {
    let tree = extract_clause_tree(FRAGMENT).unwrap();
    let scope = tree.iter().find(|n| n.id == "1").unwrap();
    let body = &FRAGMENT[scope.body_start..scope.body_end];
    assert!(body.contains("6.1"));
    assert!(!body.starts_with('#'));
}

#[test]
fn extracts_real_spec_clause_tree() {
    let src = std::fs::read_to_string(
        "../../docs/spec/1-Kernel_Modeling_Language/1-Kernel_Modeling_Language.md",
    )
    .expect("read spec md");
    let tree = extract_clause_tree(&src).unwrap();
    assert!(
        tree.len() > 400,
        "expected >400 clauses, got {}",
        tree.len()
    );
    let scope = tree.iter().find(|n| n.id == "1").unwrap();
    assert_eq!(scope.title, "Scope");
    assert_eq!(scope.anchor, "page-26-0");
    assert!(
        tree.iter().any(|n| n.id == "8.2.4.3.1"),
        "8.2.4.3.1 should be a clause"
    );
}
