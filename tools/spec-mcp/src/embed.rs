//! Sentence embeddings for semantic search: the all-MiniLM-L6-v2 (q8) model,
//! the clause text preparation it sees, and the on-disk embedding cache.
//!
//! The model is *not* embedded in this crate. The first use downloads ~23 MB
//! from huggingface.co into [`cache_dir`] (see `README.md`); every later run
//! loads it from that cache. Clause embeddings are cached beside the model and
//! recomputed only when the cache file no longer matches the corpus.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::{Context, Result, anyhow, ensure};
use fastembed::{EmbeddingModel, InitOptionsWithLength, TextEmbedding};

use crate::index::SpecIndex;

/// Embedding width of all-MiniLM-L6-v2.
pub const MODEL_DIM: usize = 384;

/// Human-readable model identity, used in error messages and `README.md`.
const MODEL_NAME: &str = "all-MiniLM-L6-v2 (q8, Xenova/all-MiniLM-L6-v2)";

/// Bumped whenever a cached vector set would change meaning: model revision,
/// text preparation, chunking, or pooling.
const CACHE_KEY: &str = "minilm6v2q8-v1";

/// Texts per inference call; the model is small, so a bigger batch keeps the
/// corpus pass to a handful of calls.
const EMBED_BATCH: usize = 64;

/// Chunk width in characters (~400 tokens), safely under the model's 512-token
/// window. 18% of the spec's clauses are longer than that window, so long
/// clauses are embedded in several chunks and pooled instead of truncated.
const CHUNK_CHARS: usize = 1600;

/// Chunks per clause; bounds compute on the ~29K-character outliers.
const MAX_CHUNKS: usize = 8;

/// The ONNX sentence-embedding session. `TextEmbedding::embed` takes
/// `&mut self`, so the session sits behind a mutex; inference is serialized,
/// which is fine for one corpus pass plus a query per call.
pub struct SemanticEngine {
    model: Mutex<TextEmbedding>,
}

impl SemanticEngine {
    /// Loads the model, downloading it into [`cache_dir`] on first use.
    ///
    /// Nothing calls this at server startup: the first semantic query pays the
    /// download (once per machine), then the model is cached on disk.
    pub fn init() -> Result<Self> {
        let dir = cache_dir();
        let mut options = InitOptionsWithLength::new(EmbeddingModel::AllMiniLML6V2Q);
        options.cache_dir = dir.clone();
        // Progress bars must never reach an MCP stdio stream.
        options.show_download_progress = false;
        let model = TextEmbedding::try_new(options).with_context(|| {
            format!(
                "could not load the {MODEL_NAME} embedding model from {}: the first run \
                 downloads it from huggingface.co, so it needs network access",
                dir.display()
            )
        })?;
        Ok(Self {
            model: Mutex::new(model),
        })
    }

    /// Unit-length embeddings for `texts`, in input order.
    pub fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        let mut model = self
            .model
            .lock()
            .map_err(|_| anyhow!("the embedding model lock is poisoned"))?;
        let mut rows = Vec::with_capacity(texts.len());
        for batch in texts.chunks(EMBED_BATCH) {
            let batch_rows = model
                .embed(batch, None)
                .context("embedding a batch failed")?;
            ensure!(
                batch_rows.len() == batch.len(),
                "the model returned {} rows for a batch of {}",
                batch_rows.len(),
                batch.len()
            );
            rows.extend(batch_rows.into_iter().map(normalize));
        }
        Ok(rows)
    }
}

/// Clause embeddings for one spec: one L2-normalized row per embedded clause.
pub struct SpecEmbeddings {
    /// Flat row-major, L2-normalized, [`MODEL_DIM`] per row.
    pub vectors: Vec<f32>,
    /// Row `r` ↔ `clauses[clause_idxs[r]]`; clauses with no embeddable text are
    /// excluded.
    pub clause_idxs: Vec<usize>,
}

impl SpecEmbeddings {
    /// Embeds every clause of `index` that has text: one mean-pooled row each.
    pub fn build(engine: &SemanticEngine, index: &SpecIndex) -> Result<Self> {
        let (clause_idxs, documents) = clause_documents(index);
        Self::embed_documents(engine, clause_idxs, documents)
    }

    /// Embeddings for `index`, taken from the disk cache when a matching file
    /// exists and computed otherwise (then stored best-effort).
    pub fn load_or_build(
        engine: &SemanticEngine,
        spec_label: &str,
        index: &SpecIndex,
    ) -> Result<Self> {
        let (clause_idxs, documents) = clause_documents(index);
        let path = cache_path(spec_label, index.source_len(), index.list_clauses().len());
        if let Some(cached) = Self::load(&path, &clause_idxs) {
            return Ok(cached);
        }
        let built = Self::embed_documents(engine, clause_idxs, documents)?;
        // An unwritable cache costs a recomputation next time, nothing more.
        let _ = built.save(&path);
        Ok(built)
    }

    /// Embeds `documents` (one chunk list per clause) and mean-pools each
    /// clause's chunk rows into a single unit-length row.
    fn embed_documents(
        engine: &SemanticEngine,
        clause_idxs: Vec<usize>,
        documents: Vec<Vec<String>>,
    ) -> Result<Self> {
        let flat: Vec<String> = documents.iter().flatten().cloned().collect();
        let rows = engine.embed(&flat)?;
        ensure!(
            rows.len() == flat.len(),
            "embedded {} rows for {} chunks",
            rows.len(),
            flat.len()
        );
        let mut vectors = Vec::with_capacity(clause_idxs.len() * MODEL_DIM);
        let mut at = 0;
        for chunks in &documents {
            let window = &rows[at..at + chunks.len()];
            at += chunks.len();
            vectors.extend(mean_pool(window));
        }
        Ok(Self {
            vectors,
            clause_idxs,
        })
    }

    /// `[u32 dim][u32 rows][f32 …]`, little-endian.
    pub fn encode(&self) -> Result<Vec<u8>> {
        ensure!(
            self.vectors.len() == self.clause_idxs.len() * MODEL_DIM,
            "{} vectors do not form {} rows of {MODEL_DIM}",
            self.vectors.len(),
            self.clause_idxs.len()
        );
        let mut out = Vec::with_capacity(8 + self.vectors.len() * 4);
        out.extend_from_slice(&(MODEL_DIM as u32).to_le_bytes());
        out.extend_from_slice(&(self.clause_idxs.len() as u32).to_le_bytes());
        for value in &self.vectors {
            out.extend_from_slice(&value.to_le_bytes());
        }
        Ok(out)
    }

    /// Decodes a cache file into `clause_idxs` rows, or `None` when its header
    /// and byte length do not describe exactly that many [`MODEL_DIM`] rows (a
    /// stale or corrupt file is simply recomputed).
    pub fn decode(bytes: &[u8], clause_idxs: Vec<usize>) -> Option<Self> {
        let dim = u32::from_le_bytes(bytes.get(0..4)?.try_into().ok()?) as usize;
        let rows = u32::from_le_bytes(bytes.get(4..8)?.try_into().ok()?) as usize;
        if dim != MODEL_DIM || rows != clause_idxs.len() {
            return None;
        }
        let body = bytes.get(8..)?;
        if body.len() != rows * dim * 4 {
            return None;
        }
        let vectors = body
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect();
        Some(Self {
            vectors,
            clause_idxs,
        })
    }

    /// Writes the cache through a temporary file and a rename, so a process
    /// killed mid-write never leaves a half-written cache behind.
    pub fn save(&self, path: &Path) -> Result<()> {
        let bytes = self.encode()?;
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, &bytes).with_context(|| format!("writing {}", tmp.display()))?;
        std::fs::rename(&tmp, path).with_context(|| format!("renaming {}", path.display()))
    }

    /// Reads and validates a cache file at `path`; `None` when it is missing,
    /// corrupt, or describes a different row set.
    pub fn load(path: &Path, clause_idxs: &[usize]) -> Option<Self> {
        let bytes = std::fs::read(path).ok()?;
        Self::decode(&bytes, clause_idxs.to_vec())
    }
}

/// Directory for the downloaded model and the clause-embedding cache:
/// `$SPEC_MCP_CACHE`, else `$XDG_CACHE_HOME/spec-mcp`, else
/// `$HOME/.cache/spec-mcp`, else the system temp directory. A directory that
/// cannot be created falls back to the temp directory instead of failing: an
/// unwritable cache only costs recomputation.
pub(crate) fn cache_dir() -> PathBuf {
    let dir = env_path("SPEC_MCP_CACHE")
        .or_else(|| env_path("XDG_CACHE_HOME").map(|p| p.join("spec-mcp")))
        .or_else(|| env_path("HOME").map(|p| p.join(".cache").join("spec-mcp")))
        .unwrap_or_else(|| std::env::temp_dir().join("spec-mcp"));
    if std::fs::create_dir_all(&dir).is_ok() {
        return dir;
    }
    let fallback = std::env::temp_dir().join("spec-mcp");
    let _ = std::fs::create_dir_all(&fallback);
    fallback
}

fn env_path(var: &str) -> Option<PathBuf> {
    std::env::var_os(var)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

/// Cache file for one spec's clause embeddings. The model key, spec label,
/// source length, and clause count make a stale file self-invalidating.
fn cache_path(spec_label: &str, md_len: usize, clause_count: usize) -> PathBuf {
    cache_dir().join(format!(
        "embeddings-{spec_label}-{CACHE_KEY}-{md_len}-{clause_count}.bin"
    ))
}

/// Arena indices of the clauses that have embeddable text, plus their chunked
/// documents in the same order.
fn clause_documents(index: &SpecIndex) -> (Vec<usize>, Vec<Vec<String>>) {
    let mut clause_idxs = Vec::new();
    let mut documents = Vec::new();
    let texts = index.own_texts();
    for (i, clause) in index.list_clauses().iter().enumerate() {
        let Some(text) = texts.get(i) else {
            continue;
        };
        let chunks = chunk_text(&clause_document(&clause.id, &clause.title, text));
        if chunks.is_empty() {
            continue;
        }
        clause_idxs.push(i);
        documents.push(chunks);
    }
    (clause_idxs, documents)
}

/// Embedding input for one clause: its id and title (they name the subject even
/// when the body is short) followed by its own prose, with fenced code blocks
/// dropped and whitespace collapsed. Grammar listings are noise for a prose
/// model and are already served by `get_syntax` and `find_symbol`.
fn clause_document(id: &str, title: &str, text: &str) -> String {
    let mut doc = format!("{id} {title}\n");
    doc.push_str(&collapse_whitespace(&strip_code_fences(text)));
    doc
}

/// Removes fenced code blocks, fences included.
fn strip_code_fences(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_fence = false;
    for line in text.split_inclusive('\n') {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if !in_fence {
            out.push_str(line);
        }
    }
    out
}

fn collapse_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Splits whitespace-collapsed text into chunks of at most [`CHUNK_CHARS`]
/// characters, breaking on whitespace, keeping at most [`MAX_CHUNKS`].
fn chunk_text(text: &str) -> Vec<String> {
    let mut chunks: Vec<String> = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        for piece in split_long_word(word) {
            let extra = piece.chars().count() + usize::from(!current.is_empty());
            if !current.is_empty() && current.chars().count() + extra > CHUNK_CHARS {
                chunks.push(std::mem::take(&mut current));
                if chunks.len() == MAX_CHUNKS {
                    return chunks;
                }
            }
            if !current.is_empty() {
                current.push(' ');
            }
            current.push_str(piece);
        }
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    chunks
}

/// Splits a token longer than a chunk on character boundaries, so no chunk can
/// exceed [`CHUNK_CHARS`] and get silently truncated by the model.
fn split_long_word(word: &str) -> Vec<&str> {
    if word.chars().count() <= CHUNK_CHARS {
        return vec![word];
    }
    let mut pieces = Vec::new();
    let mut rest = word;
    while rest.chars().count() > CHUNK_CHARS {
        let cut = rest
            .char_indices()
            .nth(CHUNK_CHARS)
            .map_or(rest.len(), |(byte, _)| byte);
        pieces.push(&rest[..cut]);
        rest = &rest[cut..];
    }
    if !rest.is_empty() {
        pieces.push(rest);
    }
    pieces
}

/// Coordinate-wise mean of a clause's chunk rows, L2-normalized. Pooling is
/// what gives a long clause full coverage: every chunk contributes instead of
/// only the model's first 512 tokens.
fn mean_pool(rows: &[Vec<f32>]) -> Vec<f32> {
    let mut acc = vec![0.0f32; MODEL_DIM];
    if rows.is_empty() {
        return acc;
    }
    for row in rows {
        for (slot, value) in acc.iter_mut().zip(row) {
            *slot += value;
        }
    }
    let count = rows.len() as f32;
    for slot in &mut acc {
        *slot /= count;
    }
    normalize(acc)
}

/// Scales a row to unit L2 norm; a zero row is left as it is.
fn normalize(mut row: Vec<f32>) -> Vec<f32> {
    let norm = row.iter().map(|value| value * value).sum::<f32>().sqrt();
    if norm > 0.0 {
        for value in &mut row {
            *value /= norm;
        }
    }
    row
}
