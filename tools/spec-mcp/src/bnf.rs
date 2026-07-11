use anyhow::Result;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Production {
    pub clause_id: String,
    pub name: String,
    pub body: String,
    pub line: usize,
}

pub fn parse_bnf(src: &str) -> Result<Vec<Production>> {
    let mut current_clause = String::new();
    let mut productions = Vec::new();
    let mut lines = src.lines().enumerate().peekable();

    while let Some((idx, line)) = lines.next() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("// Clause ") {
            current_clause = clause_id_from_marker(rest.trim());
            continue;
        }
        if trimmed.starts_with("//") || trimmed.starts_with('!') {
            continue;
        }
        if let Some(name) = production_name(trimmed) {
            let mut body = String::new();
            let after_name = trimmed[name.len()..].trim_start();
            let rhs = after_name
                .strip_prefix(':')
                .map(|s| s.trim_start())
                .and_then(|s| s.strip_prefix("= "))
                .or_else(|| after_name.strip_prefix("= "))
                .unwrap_or("");
            if !rhs.is_empty() {
                push_rhs_line(&mut body, rhs);
            }
            while let Some(&(_, next)) = lines.peek() {
                let nt = next.trim();
                if nt.is_empty()
                    || nt.starts_with("// Clause ")
                    || nt.starts_with("// ")
                    || nt.starts_with("//")
                    || production_name(nt).is_some()
                {
                    break;
                }
                push_rhs_line(&mut body, nt);
                lines.next();
            }
            productions.push(Production {
                clause_id: current_clause.clone(),
                name: name.to_string(),
                body: body.trim().to_string(),
                line: idx + 1,
            });
        }
    }
    Ok(productions)
}

fn production_name(line: &str) -> Option<String> {
    let candidate = line.split_whitespace().next()?;
    let name = candidate.trim_end_matches(':').trim_end_matches('=');
    if name.is_empty() {
        return None;
    }
    if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return None;
    }
    if !name
        .chars()
        .next()
        .map(|c| c.is_ascii_alphabetic())
        .unwrap_or(false)
    {
        return None;
    }
    if !line.contains('=') {
        return None;
    }
    Some(name.to_string())
}

fn push_rhs_line(body: &mut String, rhs: &str) {
    if !body.is_empty() {
        body.push('\n');
    }
    body.push_str(rhs);
}

fn clause_id_from_marker(rest: &str) -> String {
    rest.split_whitespace()
        .next()
        .map(|t| t.trim_end_matches('.').to_string())
        .unwrap_or_default()
}
