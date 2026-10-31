use anyhow::Result;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Production {
    pub clause_id: String,
    pub name: String,
    /// Declared supertype of the production (`Name : Supertype =`); `None`
    /// for standalone productions.
    pub supertype: Option<String>,
    pub body: String,
    pub line: usize,
}

/// Parse the OMG `.kebnf` textual grammars.
///
/// File layout (verified against both KerML and SysML releases):
/// - a production header sits at column 0 as `Name =` or `Name : Supertype =`,
///   with the RHS either on the header line (lexical rules like
///   `TYPED_BY = ':'   | 'typed' 'by'`) or on the indented lines that follow;
/// - the body is the contiguous block of indented non-blank lines after the
///   header; blank, comment, or column-0 lines end it;
/// - RHS lines routinely *contain* `=` as part of the ownership/optionality
///   operators (`ownedRelationship += X`, `isNegated ?= 'not'`,
///   `visibility = VisibilityIndicator`), and those lines always carry an
///   indent, so they must never be mistaken for headers: a header is column 0
///   with `=` (or `: Type =`) attached directly to the name.
pub fn parse_bnf(src: &str) -> Result<Vec<Production>> {
    let mut current_clause = String::new();
    let mut productions = Vec::new();
    let mut lines = src.lines().enumerate().peekable();

    while let Some((idx, line)) = lines.next() {
        let trimmed = line.trim();
        if trimmed.is_empty() || line.starts_with(|c: char| c.is_whitespace()) {
            // Body lines were already consumed by the body loop below; any
            // stray indented line outside a production is just skipped.
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("// Clause ") {
            current_clause = clause_id_from_marker(rest.trim());
            continue;
        }
        if trimmed.starts_with("//") || trimmed.starts_with('!') {
            continue;
        }
        let Some((name, supertype, rhs)) = production_header(line) else {
            continue;
        };
        let mut body = String::new();
        if !rhs.is_empty() {
            push_rhs_line(&mut body, &rhs);
        }
        while let Some(&(_, next)) = lines.peek() {
            let nt = next.trim();
            if !next.starts_with(|c: char| c.is_whitespace())
                || nt.is_empty()
                || nt.starts_with("//")
            {
                break;
            }
            push_rhs_line(&mut body, nt);
            lines.next();
        }
        productions.push(Production {
            clause_id: current_clause.clone(),
            name,
            supertype,
            body: body.trim().to_string(),
            line: idx + 1,
        });
    }
    Ok(productions)
}

/// Column-0 production header. Returns the production name, its declared
/// supertype (`None` for a plain `Name =` header), and the portion of the
/// RHS that sits on the header line itself. Accepts `Name =`,
/// `Name : Supertype =`, and either form with a same-line RHS. Rejects
/// indented lines and any line where `=` is not attached to the name (or
/// `: Supertype`) — i.e. RHS assignments using the `+=` / `?=` operators.
fn production_header(line: &str) -> Option<(String, Option<String>, String)> {
    let is_ident_char = |c: char| c.is_ascii_alphanumeric() || c == '_';
    let ascii_alpha = |c: char| c.is_ascii_alphabetic();
    let name_len = line.find(|c: char| !is_ident_char(c))?;
    let name = &line[..name_len];
    if name.is_empty() || !name.starts_with(ascii_alpha) {
        return None;
    }
    let mut rest = line[name_len..].trim_start();
    let mut supertype = None;
    if let Some(after_colon) = rest.strip_prefix(':') {
        let after_colon = after_colon.trim_start();
        let ty_len = after_colon.find(|c: char| !is_ident_char(c))?;
        let ty = &after_colon[..ty_len];
        if ty.is_empty() || !ty.starts_with(ascii_alpha) {
            return None;
        }
        supertype = Some(ty.to_string());
        rest = after_colon[ty_len..].trim_start();
    }
    let rhs = rest.strip_prefix('=')?.trim().to_string();
    Some((name.to_string(), supertype, rhs))
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
