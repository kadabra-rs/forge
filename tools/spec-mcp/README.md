# spec-mcp

Self-contained MCP server for the **OMG KerML 1.0** and **SysML v2** specifications.
It is the documentation oracle for agents: clause text, the official `.kebnf`
grammars, every spec figure, and the OMG worked-example model corpus are all
embedded in the binary — copy it anywhere and it serves both specs with no
filesystem layout around it.

> This is *not* a validator. It does not parse or compile models; it answers
> "how does the spec say to model this?" Compile/validate models with
> `spec42` (see `tools/install-sysml-tooling.sh` and the `sysml-modeling` skill).

## Tools

Every tool takes `spec: "kerml" | "sysml"`.

| Tool | Purpose |
| --- | --- |
| `search_sections` | full-text clause search (id/title hits first, then body hits with snippets) |
| `get_section` | clause body as Markdown + BNF production names it defines + figures (images inlined, up to 6) |
| `get_syntax` | official `.kebnf` production bodies by name (exact or substring), with defining clause |
| `get_figure` | one captioned diagram by printed figure number, inline |
| `list_examples` | index of the official worked-example corpus (KerML examples, SysML training/validation) |
| `get_example` | full source of one example `.kerml`/`.sysml` file from the corpus |
| `follow_link` | resolve `#anchor` cross-references found in clause text |
| `list_sections` | complete clause tree (id, title, anchor) |
| `find_implementation` | map a clause to `crates/` compiler code (meaningful only inside the forge repo) |

## Layout

| Path | Contents |
| --- | --- |
| `src/assets.rs` | embedded assets: spec Markdown (`include_str!`), BNF, figures + example corpus (`rust_embed`) |
| `src/clause_tree.rs` | clause headings → tree (tree-sitter-markdown) |
| `src/index.rs` | search index: clauses, bodies, figure refs, span anchors |
| `src/bnf.rs` | `.kebnf` production parser (column-0 `Name =` / `Name : Sup =` headers) |
| `src/tools.rs` | MCP tools (`rmcp`) + JSON DTOs |
| `src/implementation.rs` | `find_implementation` signal derivation + crates grep |
| `tests/` | integration tests, several over the *real* embedded specs |
| `npm/` | npm launcher package `@kadabra-rs/spec-mcp` (download + verify + cache + skill install) |

Embedded sources (compile-time, from the repo):

- `docs/spec/1-Kernel_Modeling_Language/` and `docs/spec/2a-OMG_Systems_Modeling_Language/`
  (Markdown + figure JPEGs; see `docs/spec/README.md`)
- `vendor/SysML-v2-Release/bnf/*.kebnf`
- `vendor/SysML-v2-Release/kerml/src/examples/**/*.kerml` (58 files)
- `vendor/SysML-v2-Release/sysml/src/**/*.sysml` (251 files: examples, training, validation)

## Build, test, run

```bash
cd tools/spec-mcp
cargo build --release            # target/release/spec-mcp (~24 MB, self-contained)
cargo test                       # 62 tests incl. real-spec coverage
cargo clippy --all-targets -- -D warnings
cargo fmt --check

# serve over stdio (optional first arg: crates/ dir for find_implementation)
./target/release/spec-mcp ../../crates
```

MCP client config in this repo (`.mcp.json` at the root) points at the local
release binary; external users install via npm:

```bash
npx -y @kadabra-rs/spec-mcp      # see npm/README.md
```

## Releases

Tag `spec-mcp-v<X.Y.Z>` (version must match both `Cargo.toml` and
`npm/package.json`). `.github/workflows/spec-mcp-release.yml` builds
linux-x64/arm64 + darwin-arm64/x64 binaries, publishes a GitHub release with
`SHA256SUMS.txt`, and publishes the npm launcher package.

## Skill

The `sysml-modeling` agent skill (canonical copy: `.omp/skills/sysml-modeling/`
at the repo root; staged into the npm package on `prepare`) teaches agents the
workflow: ask this oracle for syntax/semantics with citations → model from the
official examples → validate with `spec42 check`.
