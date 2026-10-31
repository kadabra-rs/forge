//! Eval gate for the `semantic_search` tool against lexical `search_sections`.
//!
//! One ignored test, run manually:
//!   `cargo test --release -- --ignored semantic_vs_lexical_recall --nocapture`
//!
//! The ground-truth clause ids below were established by reading the candidate
//! clauses' headings and bodies with `get_section`/`spec_toc` — a careful
//! reader accepting these clauses as the answer to the query is not
//! borrowing the model's own ranking. The printed table (lexical vs
//! semantic top-5 per query, per-query hit flags, recall@5, exclusive
//! wins/losses) is this phase's evidence; the keep rule is asserted at the
//! end: semantic recall strictly higher, at least 3 exclusive semantic
//! wins, at most 1 exclusive semantic loss. If it fails, the drop
//! procedure applies (revert; see the roadmap tracker) — the table is not
//! tuned to pass.

use spec_mcp::assets::SpecId;
use spec_mcp::index::SearchMode;
use spec_mcp::tools::SpecServer;
use std::path::PathBuf;
use std::time::Instant;

/// The embeddings-cache directory, mirroring `embed.rs::cache_dir` so a test
/// can see whether a spec's embedding file already exists before first use.
fn cache_dir() -> PathBuf {
    let env_path = |var: &str| {
        std::env::var_os(var)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
    };
    let dir = env_path("SPEC_MCP_CACHE")
        .or_else(|| env_path("XDG_CACHE_HOME").map(|p| p.join("spec-mcp")))
        .or_else(|| env_path("HOME").map(|p| p.join(".cache").join("spec-mcp")))
        .unwrap_or_else(|| std::env::temp_dir().join("spec-mcp"));
    let _ = std::fs::create_dir_all(&dir);
    dir
}

fn has_embedding_cache(spec_label: &str, index_len: usize, clause_count: usize) -> bool {
    let name = format!("embeddings-{spec_label}-minilm6v2q8-v1-{index_len}-{clause_count}.bin");
    cache_dir().join(name).exists()
}

/// One spec's timing evidence, printed when `SPEC_MCP_EVAL_TIMING=1`: first
/// semantic call (model load + cold clause-embedding build, or a warm cache
/// load) and one call afterwards (pure query latency).
fn time_first_and_warm_query(
    server: &SpecServer,
    spec: SpecId,
    index_len: usize,
    clause_count: usize,
) {
    if std::env::var("SPEC_MCP_EVAL_TIMING").is_err() {
        return;
    }
    let cached_before = has_embedding_cache(spec.label(), index_len, clause_count);
    let first = Instant::now();
    let result = server
        .semantic_search_impl(spec, "how does conjugation flip feature direction", Some(1))
        .expect("first semantic call");
    let first_ms = first.elapsed().as_secs_f64() * 1000.0;
    let cached_after = has_embedding_cache(spec.label(), index_len, clause_count);
    let kind = if cached_before {
        "warm cache load"
    } else if cached_after {
        "cold build (computed + written to cache)"
    } else {
        "cold build (unwritable cache, recomputed)"
    };
    let warm = Instant::now();
    let result_warm = server
        .semantic_search_impl(spec, "how does conjugation flip feature direction", Some(1))
        .expect("warm semantic call");
    let warm_ms = warm.elapsed().as_secs_f64() * 1000.0;
    println!(
        "[timing] {} first call: {first_ms:.0} ms ({kind}) -> top1 {} ({:.3}); warm query: {warm_ms:.0} ms -> top1 {} ({:.3})",
        spec.label(),
        result.hits[0].id,
        result.hits[0].score,
        result_warm.hits[0].id,
        result_warm.hits[0].score,
    );
}

/// (query, expected clause ids a careful reader would accept). `None`
/// expected ids = NO_GROUND_TRUTH: excluded from both denominators.
const QUERIES: &[(SpecId, &str, &[&str])] = &[
    // KerML (chapter 7 semantics, 8.3/8.4 abstract syntax / semantics).
    (
        SpecId::Kerml,
        "how does conjugation flip feature direction",
        &["7.3.2.4", "8.3.3.1.2", "8.3.3.1.5"],
    ),
    (
        SpecId::Kerml,
        "rules for redefining a feature",
        &["7.3.4.5", "8.3.3.3.8"],
    ),
    (
        SpecId::Kerml,
        "difference between a type and a feature",
        &["7.3.2", "7.3.2.1", "7.3.4.1"],
    ),
    (
        SpecId::Kerml,
        "what makes an association a link",
        &["7.4.5.1", "8.3.4.4.2", "8.4.4.5.1"],
    ),
    (
        SpecId::Kerml,
        "multiplicity constraints on features",
        &["7.4.12", "8.3.3.1.9", "8.4.4.12.1"],
    ),
    (
        SpecId::Kerml,
        "specialization cycle detection semantics",
        &["7.3.2.3", "8.3.3.1.8"],
    ),
    // SysML (chapter 7 systems modeling layer).
    (
        SpecId::Sysml,
        "requirement satisfaction semantics",
        &["7.21.1", "7.21.4"],
    ),
    (
        SpecId::Sysml,
        "how ports connect items across a boundary",
        &["7.12.1", "7.13.2"],
    ),
    (
        SpecId::Sysml,
        "allocating behavior to structural elements",
        &["7.15.1", "7.15.2"],
    ),
    (
        SpecId::Sysml,
        "entry and exit actions of a state",
        &["7.18.1", "7.18.2"],
    ),
    (
        SpecId::Sysml,
        "attribute usage typing and default values",
        &["7.7.2", "7.6.3"],
    ),
    (
        SpecId::Sysml,
        "view and viewpoint rendering of models",
        &["7.26.1", "7.26.2", "7.26.3"],
    ),
];

#[test]
#[ignore = "embeds the full corpus; run manually"]
fn semantic_vs_lexical_recall() {
    let server =
        SpecServer::embedded(std::path::PathBuf::from("../../crates")).expect("embedded server");

    println!();
    println!(
        "Query set: {} queries ({} with ground truth)",
        QUERIES.len(),
        QUERIES.iter().filter(|q| !q.2.is_empty()).count()
    );
    println!("{}", "=".repeat(100));

    // Timing evidence (SPEC_MCP_EVAL_TIMING=1): the first semantic call per
    // spec pays for the model load plus the clause-embedding build (or a
    // warm cache load); every query in the loop below then runs warm.
    if std::env::var("SPEC_MCP_EVAL_TIMING").is_ok() {
        for spec in SpecId::ALL {
            let index = spec_mcp::index::build_index(
                spec_mcp::assets::spec_md(spec),
                spec_mcp::assets::spec_bnf(spec),
            )
            .expect("index build");
            time_first_and_warm_query(
                &server,
                spec,
                index.source_len(),
                index.list_clauses().len(),
            );
        }
    }

    let mut scored = 0usize;
    let mut lex_hits = 0usize;
    let mut sem_hits = 0usize;
    let mut sem_exclusive_wins = 0usize;
    let mut sem_exclusive_losses = 0usize;

    for (spec, query, expected) in QUERIES {
        // Lexical: id/title tier first, then figure captions, then body
        // (tf-idf ranked) — exactly what the search_sections tool returns.
        let lexical: Vec<String> = server
            .search_sections_impl(*spec, query, SearchMode::default())
            .expect("lexical search")
            .iter()
            .map(|h| h.id.clone())
            .take(5)
            .collect();

        let semantic = server
            .semantic_search_impl(*spec, query, Some(5))
            .expect("semantic search");
        let semantic_ids: Vec<String> = semantic.hits.iter().map(|h| h.id.clone()).collect();

        let expected_set = expected
            .iter()
            .copied()
            .collect::<std::collections::HashSet<_>>();
        let lex_hit = lexical.iter().any(|id| expected_set.contains(id.as_str()));
        let sem_hit = semantic_ids
            .iter()
            .any(|id| expected_set.contains(id.as_str()));

        let verdict = if expected.is_empty() {
            "NO_GROUND_TRUTH (excluded)".to_string()
        } else {
            scored += 1;
            if lex_hit {
                lex_hits += 1;
            }
            if sem_hit {
                sem_hits += 1;
            }
            if sem_hit && !lex_hit {
                sem_exclusive_wins += 1;
            }
            if lex_hit && !sem_hit {
                sem_exclusive_losses += 1;
            }
            format!("lex={} sem={}", lex_hit, sem_hit)
        };

        println!("{:<58} {}", query, verdict);
        println!("    expected:  {:?}", expected);
        println!("    lexical 5:   {}", lexical.join(", "));
        println!("    semantic 5:  {}", semantic_ids.join(", "));
        println!();
    }

    // Recall@5: fraction of ground-truthed queries whose expected set
    // intersects the side's top-5.
    let lex_recall = lex_hits as f64 / scored as f64;
    let sem_recall = sem_hits as f64 / scored as f64;

    println!(
        "recall@5  lexical: {lex_hits}/{scored} = {lex_recall:.3}   semantic: {sem_hits}/{scored} = {sem_recall:.3}"
    );
    println!(
        "exclusive: semantic-only wins = {sem_exclusive_wins}, semantic-only losses = {sem_exclusive_losses}"
    );

    let keep = sem_recall > lex_recall && sem_exclusive_wins >= 3 && sem_exclusive_losses <= 1;
    println!(
        "verdict: {} (keep rule: sem_recall5 > lex_recall5 && exclusive_wins >= 3 && exclusive_losses <= 1)",
        if keep { "KEPT" } else { "DROP-REQUIRED" }
    );
    assert!(
        keep,
        "semantic search failed the keep rule: lex_recall5={lex_recall:.3} sem_recall5={sem_recall:.3} wins={sem_exclusive_wins} losses={sem_exclusive_losses} — run the drop procedure (roadmap tracker)"
    );
}

/// A 3-clause synthetic spec, shared by the engine-touching tests below so
/// each test process builds it once (the model downloads on first engine use).
fn test_server() -> SpecServer {
    let md = "## <span id=\"p1\"></span>**7.3.2 Types**\n\n\
              A type is a classifier of things; types may specialize other types.\n\n\
              ### <span id=\"p2\"></span>**7.3.2.3 Specialization**\n\n\
              A specialization asserts that the specific type is a kind of the general type.\n\n\
              ### <span id=\"p3\"></span>**7.3.2.4 Conjugation**\n\n\
              Conjugation reverses the direction of in and out features of the original type.\n";
    SpecServer::with_spec_source(
        SpecId::Kerml,
        md,
        "",
        std::path::PathBuf::from("../../crates"),
    )
    .unwrap()
}

#[test]
fn semantic_search_ranks_and_clamps() {
    let server = test_server();
    let result = server
        .semantic_search_impl(SpecId::Kerml, "reversing feature direction", None)
        .expect("semantic search");
    assert_eq!(result.hits.len(), 3, "all three clauses have text");
    for hit in &result.hits {
        assert!(hit.score.is_finite());
        assert!(
            hit.score <= 1.0 + 1e-3,
            "cosine of unit rows cannot exceed 1: {}",
            hit.score
        );
        assert!(hit.snippet.is_some(), "each hit carries a snippet");
    }
    for pair in result.hits.iter().zip(result.hits.iter().skip(1)) {
        assert!(pair.0.score >= pair.1.score, "sorted by score desc");
    }
    assert!(
        result
            .hits
            .iter()
            .map(|h| h.id.as_str())
            .any(|id| id == "7.3.2.4"),
        "the conjugation clause must be reachable: {:?}",
        result.hits.iter().map(|h| &h.id).collect::<Vec<_>>()
    );

    let one = server
        .semantic_search_impl(SpecId::Kerml, "reversing feature direction", Some(1))
        .expect("semantic search, limit 1");
    assert_eq!(one.hits.len(), 1, "limit 1 returns exactly one hit");
}

#[test]
fn semantic_search_rejects_bad_params() {
    let server = test_server();
    assert!(
        server
            .semantic_search_impl(SpecId::Kerml, "", None)
            .is_err()
    );
    assert!(
        server
            .semantic_search_impl(SpecId::Kerml, "   ", None)
            .is_err()
    );
    // Unloaded spec: the index-error path shared with the other tools.
    assert!(
        server
            .semantic_search_impl(SpecId::Sysml, "anything", None)
            .is_err()
    );
}
