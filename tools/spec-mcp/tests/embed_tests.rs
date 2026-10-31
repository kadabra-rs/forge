//! Tests for the embedding engine and its disk cache.
//!
//! The engine test downloads the model on a cold cache; it is the only test
//! that touches it, so the tests cannot race each other on that download.
//! The cache round-trip tests never build the engine.

use std::path::Path;

use spec_mcp::embed::{MODEL_DIM, SemanticEngine, SpecEmbeddings};

#[test]
fn engine_embeds_to_normalized_384_dims() {
    // The first run needs network access: the ~23 MB model is downloaded from
    // huggingface.co into the spec-mcp cache directory. Later runs load it
    // from that cache.
    let engine = SemanticEngine::init().expect("engine init");
    let rows = engine
        .embed(&[
            "specialization".to_string(),
            "a feature redefines another".to_string(),
        ])
        .expect("embed");
    assert_eq!(rows.len(), 2);
    for row in &rows {
        assert_eq!(row.len(), MODEL_DIM);
        let norm: f32 = row.iter().map(|v| v * v).sum::<f32>().sqrt();
        assert!(
            (norm - 1.0).abs() < 1e-3,
            "row norm {norm} is not unit length"
        );
    }
    assert_ne!(
        rows[0], rows[1],
        "the two different texts must embed differently"
    );
}

/// Fake embeddings small enough for the round-trip tests: 3 clauses of
/// 4-dimensional rows. (The cache format is dimension-generic in its header;
/// [`SpecEmbeddings::decode`] still enforces `MODEL_DIM`, so the fake rows are
/// `MODEL_DIM`-sized to exercise the honest path and the wrong-dim rejection.)
fn fake_embeddings() -> (Vec<f32>, Vec<usize>) {
    let clause_idxs = vec![0usize, 2, 5];
    let mut vectors = Vec::with_capacity(3 * MODEL_DIM);
    let mut seed = 1.0f32;
    for row in 0..3 {
        for dim in 0..MODEL_DIM {
            seed += 0.1;
            vectors.push((seed * (1.0 + dim as f32 / 97.0 + row as f32)).cos());
        }
    }
    (vectors, clause_idxs)
}

fn unique_cache_dir() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "spec-mcp-embed-tests-{}-{:?}",
        std::process::id(),
        std::thread::current().name().unwrap_or("test")
    ));
    std::fs::create_dir_all(&dir).expect("create cache dir");
    dir
}

#[test]
fn cache_round_trip_preserves_vectors_and_clause_idxs() {
    let dir = unique_cache_dir();
    let (vectors, clause_idxs) = fake_embeddings();
    let embeddings = SpecEmbeddings {
        vectors: vectors.clone(),
        clause_idxs: clause_idxs.clone(),
    };
    let path = dir.join("round-trip.bin");
    embeddings.save(&path).expect("save");

    let loaded = SpecEmbeddings::load(&path, &clause_idxs).expect("load");
    assert_eq!(loaded.vectors, vectors);
    assert_eq!(loaded.clause_idxs, clause_idxs);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn cache_load_rejects_wrong_dims_and_bad_lengths() {
    let dir = unique_cache_dir();
    let (vectors, clause_idxs) = fake_embeddings();
    let path = dir.join("corrupt.bin");
    SpecEmbeddings {
        vectors: vectors.clone(),
        clause_idxs: clause_idxs.clone(),
    }
    .save(&path)
    .expect("save");

    // Header says 4 dimensions, body is 3 x MODEL_DIM floats: rejected.
    let mut bad_dim = std::fs::read(&path).expect("read");
    bad_dim[..4].copy_from_slice(&(4u32).to_le_bytes());
    let bad_dim_path = dir.join("bad-dim.bin");
    std::fs::write(&bad_dim_path, &bad_dim).expect("write");
    assert!(SpecEmbeddings::load(&bad_dim_path, &clause_idxs).is_none());

    // Header row count 3, but the body is two floats short: rejected.
    let mut bad_length = std::fs::read(&path).expect("read");
    bad_length.truncate(bad_length.len() - 8);
    let bad_length_path = dir.join("bad-length.bin");
    std::fs::write(&bad_length_path, &bad_length).expect("write");
    assert!(SpecEmbeddings::load(&bad_length_path, &clause_idxs).is_none());

    // A row count that matches no clause set: rejected.
    assert!(SpecEmbeddings::load(&path, &[0, 2]).is_none());

    // A file that does not exist is not an error, just a recompute.
    assert!(SpecEmbeddings::load(Path::new("/nonexistent/spec-mcp-cache"), &clause_idxs).is_none());
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn cache_round_trip_survives_reencoding() {
    let dir = unique_cache_dir();
    let (vectors, clause_idxs) = fake_embeddings();
    let path = dir.join("reencode.bin");
    SpecEmbeddings {
        vectors,
        clause_idxs: clause_idxs.clone(),
    }
    .save(&path)
    .expect("save");
    let loaded = SpecEmbeddings::load(&path, &clause_idxs).expect("load");
    // Re-encoding the loaded value yields the identical bytes.
    assert_eq!(
        loaded.encode().expect("encode"),
        SpecEmbeddings::load(&path, &clause_idxs)
            .unwrap()
            .encode()
            .expect("encode")
    );
    std::fs::remove_dir_all(&dir).ok();
}
