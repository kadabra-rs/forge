# @kadabra-rs/spec-mcp

Self-contained [MCP](https://modelcontextprotocol.io) server for the **OMG KerML 1.0** and
**SysML v2** specifications — an authoritative oracle agents consult *before* writing model
code, so syntax and semantics come from the spec instead of being invented.

Everything is embedded in a single binary (no repo checkout needed at runtime):

- full clause text of both specs (Markdown conversions of the OMG PDFs)
- the official textual-notation grammars (`.kebnf`)
- every spec figure/diagram as an inline image
- the OMG worked-example model corpus (KerML examples + SysML training/validation models)

## Run with npx / bunx

```bash
npx -y @kadabra-rs/spec-mcp            # serve over stdio (downloads + SHA-256 verifies the binary)
bunx @kadabra-rs/spec-mcp              # same under bun
npx -y @kadabra-rs/spec-mcp install    # prefetch the binary only
npx -y @kadabra-rs/spec-mcp path       # print the cached binary path
```

MCP client config (`.mcp.json`, `.omp/mcp.json`, Claude Desktop, …):

```json
{
  "mcpServers": {
    "spec": { "command": "npx", "args": ["-y", "@kadabra-rs/spec-mcp"] }
  }
}
```

First run caches the platform binary under `~/.cache/spec-mcp/<version>-<platform>/`;
subsequent runs start instantly.

## Tools

Every tool takes `spec: "kerml" | "sysml"`.

| Tool | Purpose |
| --- | --- |
| `search_sections` | full-text clause search (title hits first, body hits with snippets) |
| `get_section` | clause body as Markdown + defined BNF productions + figures (images inline) |
| `get_syntax` | official `.kebnf` production bodies by name, with defining clause |
| `get_figure` | one captioned diagram by printed figure number, inline |
| `list_examples` | index of the official worked-example model corpus |
| `get_example` | full source of one example `.kerml`/`.sysml` file |
| `follow_link` | resolve `#anchor` cross-references between clauses |
| `spec_toc` | nested clause outline (chapters → sections → clauses), optional max_depth |
| `find_implementation` | map a clause to compiler code (only inside the forge repo) |

## Bundled agent skill

The package ships a `sysml-modeling` skill teaching agents the two-surface workflow
(**spec-mcp = documentation oracle**, **spec42 CLI = validator**): ground every construct in a
BNF production / clause, learn shape from the official examples, then validate with
`spec42 check`.

```bash
npx -y @kadabra-rs/spec-mcp skill install                    # -> ~/.omp/agent/skills/sysml-modeling
npx -y @kadabra-rs/spec-mcp skill install --agent all        # omp + claude + codex + agents dirs
npx -y @kadabra-rs/spec-mcp skill install --project          # -> ./.omp/skills (commit it)
npx -y @kadabra-rs/spec-mcp skill remove
```

## Environment

| Variable | Effect |
| --- | --- |
| `SPEC_MCP_BINARY` | use a prebuilt binary, skip download |
| `SPEC_MCP_VERSION` | override the release version |
| `SPEC_MCP_BASE_URL` | override the download base URL (mirrors/tests) |
| `SPEC_MCP_CACHE_DIR` | override the cache directory |
| `SPEC_MCP_CRATES_DIR` | passed to the server for `find_implementation` |

Binaries are released on [GitHub](https://github.com/kadabra-rs/forge/releases) with
`SHA256SUMS.txt`; the launcher refuses unverified downloads.

## License

AGPL-3.0-only (same as the spec-mcp source in
[`kadabra-rs/forge`](https://github.com/kadabra-rs/forge/tree/main/tools/spec-mcp)).
