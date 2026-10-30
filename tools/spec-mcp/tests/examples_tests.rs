use spec_mcp::assets::{SpecId, example_paths, example_source};
use spec_mcp::tools::SpecServer;

fn test_server() -> SpecServer {
    SpecServer::embedded(std::path::PathBuf::from("../../crates")).unwrap()
}

#[test]
fn lists_model_sources_for_both_specs() {
    for (spec, range, ext) in [
        (SpecId::Kerml, 50..=70, ".kerml"),
        (SpecId::Sysml, 240..=340, ".sysml"),
    ] {
        let paths = example_paths(spec);
        assert!(
            range.contains(&paths.len()),
            "{} should have a populated example corpus, got {}",
            spec.label(),
            paths.len()
        );
        assert!(paths.is_sorted(), "{} paths should be sorted", spec.label());
        assert!(
            paths.iter().all(|p| p.ends_with(ext)),
            "{} should only include {ext} sources",
            spec.label()
        );
    }
}

#[test]
fn every_listed_path_round_trips_through_source_lookup() {
    for spec in SpecId::ALL {
        for path in example_paths(spec) {
            let source = example_source(spec, &path)
                .unwrap_or_else(|| panic!("{}: '{}' should resolve", spec.label(), path));
            assert!(
                !source.is_empty(),
                "{}: '{}' should have content",
                spec.label(),
                path
            );
            assert!(
                std::str::from_utf8(source.as_ref()).is_ok(),
                "{}: '{}' should be UTF-8",
                spec.label(),
                path
            );
        }
    }
}

#[test]
fn rejects_traversal_and_absolute_paths() {
    for bad in [
        "../../Cargo.toml",
        "../sysml/src/examples/X.sysml",
        "examples/../../Cargo.toml",
        "/etc/passwd",
        "",
    ] {
        assert!(
            example_source(SpecId::Sysml, bad).is_none(),
            "'{bad}' must be rejected"
        );
        assert!(
            example_source(SpecId::Kerml, bad).is_none(),
            "'{bad}' must be rejected for kerml too"
        );
    }
}

#[test]
fn get_example_returns_source_for_space_containing_path() {
    let server = test_server();
    let path = "training/32. Requirements/Requirement Definitions.sysml";
    let example = server
        .get_example_impl(SpecId::Sysml, path)
        .expect("space-containing path should resolve");
    assert_eq!(example.path, path);
    assert!(
        example.source.contains("requirement def"),
        "source should be real SysML, got: {}",
        &example.source[..example.source.len().min(120)]
    );
    let expected_len = example_source(SpecId::Sysml, path).unwrap().len();
    assert_eq!(example.source.len(), expected_len);
}

#[test]
fn get_example_returns_none_for_unknown_path() {
    let server = test_server();
    assert!(
        server
            .get_example_impl(SpecId::Sysml, "training/does-not-exist.sysml")
            .is_none()
    );
    assert!(
        server
            .get_example_impl(SpecId::Sysml, "../../Cargo.toml")
            .is_none()
    );
}

#[test]
fn list_examples_impl_reports_path_and_size_for_every_source() {
    let server = test_server();
    for spec in SpecId::ALL {
        let entries = server.list_examples_impl(spec);
        assert_eq!(entries.len(), example_paths(spec).len());
        for entry in &entries {
            assert!(!entry.path.is_empty());
            let source = example_source(spec, &entry.path).unwrap();
            assert_eq!(
                entry.bytes,
                source.len(),
                "{}: {}",
                spec.label(),
                entry.path
            );
        }
        assert!(
            entries
                .iter()
                .any(|e| e.path.starts_with("training/") || e.path.starts_with("Simple Tests/")),
            "{} listing should group by top-level directory",
            spec.label()
        );
    }
}

#[test]
fn kerml_example_source_is_model_text() {
    let server = test_server();
    let paths = example_paths(SpecId::Kerml);
    let path = paths
        .iter()
        .find(|p| p.contains("Simple Tests"))
        .expect("KerML corpus has a Simple Tests folder");
    let example = server.get_example_impl(SpecId::Kerml, path).unwrap();
    assert!(
        example.source.contains("package"),
        "KerML source should contain a package declaration"
    );
}
