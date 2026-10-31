use anyhow::Result;
use regex::Regex;
use regex::escape;
use std::collections::HashSet;
use std::path::Path;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CodeSignal {
    ParserMethod(String),
    TokenKind(String),
    DefKind(String),
    SerializerFn(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImplStatus {
    Implemented,
    Partial,
    NotStarted,
    OutOfScope,
}

#[derive(Clone, Debug)]
pub struct ImplReport {
    pub status: ImplStatus,
    pub entry_points: Vec<String>,
}

pub fn derive_signals(clause_id: &str, production_names: &[&str]) -> Vec<CodeSignal> {
    let mut out = Vec::new();
    if clause_id.starts_with("8.2.2.") {
        for &name in production_names {
            if let Some(t) = lexical_token_kind(name) {
                out.push(CodeSignal::TokenKind(t));
            }
        }
    } else if clause_id.starts_with("8.2.") {
        for &name in production_names {
            if let Some(m) = parser_method_name(name) {
                out.push(CodeSignal::ParserMethod(m));
            }
        }
    }
    out
}

fn lexical_token_kind(production: &str) -> Option<String> {
    let map: &[(&str, &str)] = &[
        ("SINGLE_LINE_NOTE", "Comment"),
        ("MULTILINE_NOTE", "Comment"),
        ("REGULAR_COMMENT", "Comment"),
        ("STRING_VALUE", "StringLiteral"),
        ("DECIMAL_VALUE", "IntLiteral"),
        ("EXPONENTIAL_VALUE", "IntLiteral"),
        ("RESERVED_KEYWORD", "ReservedKeyword"),
        ("RESERVED_SYMBOL", "ReservedSymbol"),
        ("UNRESTRICTED_NAME", "UnrestrictedName"),
        ("BASIC_NAME", "Ident"),
    ];
    map.iter()
        .find(|(p, _)| *p == production)
        .map(|(_, t)| t.to_string())
}

fn parser_method_name(production: &str) -> Option<String> {
    let snake = to_snake(production);
    if snake.is_empty() {
        None
    } else {
        Some(format!("parse_{snake}"))
    }
}

fn to_snake(name: &str) -> String {
    let normalized = name.replace("Declaration", "Decl");
    let mut out = String::new();
    for (i, c) in normalized.chars().enumerate() {
        if c.is_uppercase() {
            if i > 0 {
                out.push('_');
            }
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

pub fn find_implementation(
    clause_id: &str,
    production_names: &[&str],
    crates_dir: &Path,
) -> Result<ImplReport> {
    let signals = derive_signals(clause_id, production_names);
    if signals.is_empty() {
        return Ok(ImplReport {
            status: ImplStatus::OutOfScope,
            entry_points: vec![],
        });
    }
    let mut hits = Vec::new();
    for sig in &signals {
        let needle = match sig {
            CodeSignal::ParserMethod(m) => format!("fn {m}"),
            CodeSignal::TokenKind(t) => format!("{t},"),
            CodeSignal::DefKind(d) => format!("{d},"),
            CodeSignal::SerializerFn(f) => format!("fn {f}"),
        };
        let found = grep_crates(crates_dir, &needle)?;
        hits.extend(found);
    }
    let status = if hits.is_empty() {
        ImplStatus::NotStarted
    } else {
        ImplStatus::Implemented
    };
    Ok(ImplReport {
        status,
        entry_points: hits,
    })
}

fn grep_crates(dir: &Path, needle: &str) -> Result<Vec<String>> {
    // `rg` is not preinstalled on every CI runner image, so scan the tree
    // directly instead of shelling out.
    let re = Regex::new(&escape(needle))?;
    let mut hits = Vec::new();
    search_rust_files(dir, &re, &mut hits)?;
    let mut seen = HashSet::new();
    hits.retain(|h| seen.insert(h.clone()));
    Ok(hits)
}

fn search_rust_files(dir: &Path, re: &Regex, hits: &mut Vec<String>) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let target = entry.path();
        if target.is_dir() {
            search_rust_files(&target, re, hits)?;
            continue;
        }
        if target.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        let content = std::fs::read_to_string(&target)?;
        let rel = target
            .strip_prefix(".")
            .unwrap_or(&target)
            .display()
            .to_string();
        for (i, line) in content.lines().enumerate() {
            if re.is_match(line) {
                hits.push(format!("{rel}:{}:{}", i + 1, line));
            }
        }
    }
    Ok(())
}
