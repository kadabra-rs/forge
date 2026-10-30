//! Spec assets embedded into the binary at compile time.
//!
//! The Markdown conversions of the OMG spec PDFs, the official textual BNF
//! grammars, and every figure image linked from the Markdown are compiled in,
//! so the release binary is self-contained: copy it anywhere and it serves
//! both specs without any filesystem layout around it.

use std::borrow::Cow;

use rust_embed::RustEmbed;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Which OMG specification to query. Every tool takes one of these.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum SpecId {
    /// KerML 1.0 — Kernel Modeling Language (kernel semantics + textual syntax).
    Kerml,
    /// OMG SysML v2 — Systems Modeling Language (systems layer built on KerML).
    Sysml,
}

impl SpecId {
    pub const ALL: [SpecId; 2] = [SpecId::Kerml, SpecId::Sysml];

    pub fn label(self) -> &'static str {
        match self {
            SpecId::Kerml => "kerml",
            SpecId::Sysml => "sysml",
        }
    }
}

#[derive(RustEmbed)]
#[folder = "../../docs/spec/1-Kernel_Modeling_Language"]
#[include = "*.jpeg"]
struct KermlFigures;

#[derive(RustEmbed)]
#[folder = "../../docs/spec/2a-OMG_Systems_Modeling_Language"]
#[include = "*.jpeg"]
struct SysmlFigures;

/// Worked-example models from the OMG release: the KerML examples tree.
#[derive(RustEmbed)]
#[folder = "../../vendor/SysML-v2-Release/kerml/src/examples"]
#[include = "**/*.kerml"]
struct KermlExamples;

/// Worked-example models from the OMG release: the SysML v2 examples,
/// training, and validation trees.
#[derive(RustEmbed)]
#[folder = "../../vendor/SysML-v2-Release/sysml/src"]
#[include = "**/*.sysml"]
struct SysmlExamples;

/// Full Markdown conversion of the spec PDF (see docs/spec/README.md).
pub fn spec_md(spec: SpecId) -> &'static str {
    match spec {
        SpecId::Kerml => include_str!(
            "../../../docs/spec/1-Kernel_Modeling_Language/1-Kernel_Modeling_Language.md"
        ),
        SpecId::Sysml => include_str!(
            "../../../docs/spec/2a-OMG_Systems_Modeling_Language/2a-OMG_Systems_Modeling_Language.md"
        ),
    }
}

/// Official OMG textual-notation grammar (.kebnf) for the spec.
pub fn spec_bnf(spec: SpecId) -> &'static str {
    match spec {
        SpecId::Kerml => {
            include_str!("../../../vendor/SysML-v2-Release/bnf/KerML-textual-bnf.kebnf")
        }
        SpecId::Sysml => {
            include_str!("../../../vendor/SysML-v2-Release/bnf/SysML-textual-bnf.kebnf")
        }
    }
}

/// Raw JPEG bytes of a figure image referenced from the spec Markdown, by its
/// relative file name (e.g. `_page_131_Figure_3.jpeg`).
pub fn figure_bytes(spec: SpecId, file_name: &str) -> Option<Cow<'static, [u8]>> {
    match spec {
        SpecId::Kerml => KermlFigures::get(file_name).map(|f| f.data),
        SpecId::Sysml => SysmlFigures::get(file_name).map(|f| f.data),
    }
}

/// Relative paths of every embedded worked-example model for the spec,
/// sorted so they group by top-level directory (`examples/`, `training/`,
/// `validation/`). KerML paths are relative to
/// `kerml/src/examples/`, SysML paths to `sysml/src/`.
pub fn example_paths(spec: SpecId) -> Vec<String> {
    let mut paths: Vec<String> = match spec {
        SpecId::Kerml => KermlExamples::iter().map(|f| f.into_owned()).collect(),
        SpecId::Sysml => SysmlExamples::iter().map(|f| f.into_owned()).collect(),
    };
    paths.sort();
    paths
}

/// Raw UTF-8 bytes of one embedded example model, by its exact relative path
/// as returned from [`example_paths`]. Paths that are absolute, contain a
/// backslash, or contain empty/`.`/`..` segments are rejected.
pub fn example_source(spec: SpecId, rel_path: &str) -> Option<Cow<'static, [u8]>> {
    if !is_safe_rel_path(rel_path) {
        return None;
    }
    match spec {
        SpecId::Kerml => KermlExamples::get(rel_path).map(|f| f.data),
        SpecId::Sysml => SysmlExamples::get(rel_path).map(|f| f.data),
    }
}

fn is_safe_rel_path(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path.contains('\\')
        && path
            .split('/')
            .all(|seg| !seg.is_empty() && seg != "." && seg != "..")
}
