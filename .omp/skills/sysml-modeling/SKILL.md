---
name: sysml-modeling
description: Use when creating, extending, reviewing, validating, or exploring SysML v2 and KerML models, or when asked how something should be modeled per the OMG specification. Grounds syntax and semantics in the spec-mcp oracle (clauses, BNF productions, figures, official worked examples) and validates models with the spec42 CLI.
---

# SysML v2 / KerML modeling with the spec oracle and spec42

Two distinct surfaces, never interchangeable:

| Surface | What it is | Use it for |
| --- | --- | --- |
| **`spec` MCP server** (package `@kadabra-rs/spec-mcp`) | The OMG KerML 1.0 + SysML v2 specifications embedded in a binary: clause text, official BNF grammars, diagrams, and the OMG worked-example model corpus. It is *documentation*, not a validator. | How something is modeled: normative rules, concrete syntax (BNF), figures, real example models, clause citations. |
| **`spec42` CLI** | SysML v2 / KerML language tooling (parser, resolver, validator). It *compiles* model files. | Does my model parse/resolve/validate; what do diagnostics mean; resolved structure (`model-export`). |

Typical loop: learn from the oracle → write the smallest slice → validate with spec42 → repair root causes → cite the oracle when uncertain.

## Ground rules

- Never invent concrete syntax. If you are not sure a construct exists or how it is written, ask the oracle: `get_syntax` (BNF production bodies) and `get_section` (the clause that defines it). Every syntax claim you write down should trace to a BNF production or a clause ID; when reporting, cite it (e.g. "§8.2.2.21.2").
- Before modeling an unfamiliar concept (requirements, ports, successions, variance, metadata, views…), look at how the OMG themselves modeled it: `list_examples` then `get_example`. The corpus is the official SysML v2 release material — training examples (`NN. <Topic>/…`), validation suites, KerML examples. Copy *shape* from examples only after confirming the governing syntax with `get_syntax`.
- Semantics questions ("may a RequirementUsage specialize…", "what does conjugates do to directions") go to `search_sections` + `get_section`, never to memory.
- Validation is what `spec42` says, not what the oracle says, and not what the source text looks like. Never claim a model is valid without an observed `spec42 check` run; never claim the oracle validated anything — it does not compile.

## spec oracle tools (MCP, all take `spec: "kerml" | "sysml"`)

| Tool | Returns |
| --- | --- |
| `search_sections {spec, query}` | Clauses whose id/title/body match, body hits with snippets. Entry point for any question. |
| `get_section {spec, clause_id}` | Clause body as Markdown + BNF production names defined there + figures; diagram images arrive inline (uncaptioned diagrams have `number: null`, captioned ones carry their printed number). |
| `get_syntax {spec, production}` | `.kebnf` production bodies (exact or substring match, exact first) with the defining clause. |
| `get_figure {spec, figure_number}` | One captioned diagram, inline, with title and owning clause. |
| `list_examples {spec}` | Index of the official worked-example corpus (paths grouped by topic). |
| `get_example {spec, path}` | Full source of one example `.kerml`/`.sysml` file from the corpus. |
| `follow_link {spec, link}` | Resolves `#anchor` cross-references found in clause text to their clause. |
| `list_sections {spec}` | Full clause tree (id, title, anchor) — orient before drilling in. |
| `find_implementation {spec, clause_id}` | Maps a clause to compiler code in the forge repo (only meaningful there). |

The KerML spec holds the kernel semantics (clause 7.x), textual + abstract syntax (8.x); SysML v2 builds the systems layer on top. When a SysML clause says "see KerML 8.4.x", switch `spec` and follow it.

## spec42 CLI contract

```bash
command -v spec42 || echo "spec42 missing: in the forge repo run: sh tools/install-sysml-tooling.sh; elsewhere download a platform archive from https://github.com/elan8/spec42/releases and verify it against that release's SHA256SUMS.txt"

spec42 doctor --format json                                   # environment/stdlib first on suspect setups
spec42 check <path-or-dir> --format json                      # validation report
spec42 check file.sysml --workspace-root <dir> --format json  # single file inside a multi-file project
spec42 explain-diagnostic --code <code> --format json         # stable meaning of a diagnostic code
spec42 model-export <dir> --format json                       # resolved structure (elements, relationships, connectors)
spec42 model-export <dir> --max-nodes 500 --format json       # bounded; ALWAYS inspect projection.truncation
```

Reading `check`: gate on `summary.error_count == 0`; report `summary.warning_count` separately; per-document diagnostics carry `code`, `message`, LSP `range`; `advice` flags environment problems (missing standard library). Many `unresolved_*` codes → run `doctor` before rewriting imports or types. A clean check proves the file compiles — not that the model satisfies what the user intended.

Reading `model-export`: it is resolved truth (specialization, typing, connector topology, metadata), the source text is not. Check `projection.schema_version`, state when a bounded export truncated.

There is no spec42 MCP server or HTTP API — upstream removed both in v0.50.0; the CLI is the only surface. If the binary is missing, do not claim validation happened: write "Not validated with Spec42" and offer an unvalidated draft.

## Modeling workflow (small steps, always)

1. **Frame.** System boundary, owning package, exact requested change. Inspect existing model conventions first (imports, package layout, naming).
2. **Ground.** `search_sections` → `get_section` for the governing clauses; `get_syntax` for every construct you intend to emit; `get_example` on one or two official examples of that construct. Collect: normative rules, production bodies, example shapes.
3. **Separate.** Facts from the conversation vs assumptions vs open questions. Do not turn every utterance into a requirement; attribute claims and preserve ambiguity.
4. **Slice.** Smallest coherent change; definitions/usages stay in their owning packages; one authoritative definition per concept, reference it instead of duplicating.
5. **Edit.** Minimal diff, existing conventions preserved.
6. **Validate.** `spec42 check` with correct workspace scope. New error → root cause first, in this order: toolchain/stdlib → syntax → names/imports → typing/specialization → relations/behavior → traceability. Re-run after each fix; stop a speculative chain the moment it introduces new diagnostics; keep a baseline count so preexisting errors aren't blamed on you.
7. **Verify structure.** When resolved relationships matter, `model-export` and cite the projection — with truncation status.
8. **Report.** What changed; exact check scope and counts; clause/BNF citations backing the syntax; remaining warnings; assumptions; one next question when a material decision lacks evidence.

## Repair and review specifics

- Reviewing a model or diff: read it, `check` it, then verify constructs against `get_syntax` — a construct can compile under spec42 yet be modelled against the spec's intent; the clause text is the arbiter of intent, cite it.
- Unusual syntax in someone's model is not a license to copy it: confirm in the grammar before reusing.
- Never suppress diagnostics by deleting content, disabling the standard library, or rewriting imports to hide errors; fix the root cause or report it.

## Anti-fabrication checklist

- Syntax invented without a `get_syntax`/`get_section` citation → do not emit it.
- "Validated" without an observed `spec42 check` JSON → say "Not validated with Spec42".
- "Complete structure" from a `--max-nodes` export → check `projection.truncation` first.
- Relationship not present in `model-export` projection → report it as missing/inferred, never as verified.
