// Copies the canonical repo skill (.omp/skills/sysml-modeling) into the npm
// package payload (skill/). Runs as the package `prepare` script, i.e. before
// npm pack/publish, so the published tarball ships the skill without a second
// maintained copy.
import { cpSync, mkdirSync, existsSync, rmSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const pkgDir = dirname(dirname(fileURLToPath(import.meta.url))); // npm/
const repoRoot = join(pkgDir, "..", "..", ".."); // tools/spec-mcp/npm -> repo root
const src = join(repoRoot, ".omp", "skills", "sysml-modeling");
const dest = join(pkgDir, "skill");

if (!existsSync(join(src, "SKILL.md"))) {
  console.error(`spec-mcp: canonical skill not found at ${src}`);
  process.exit(1);
}
rmSync(dest, { recursive: true, force: true });
mkdirSync(dest, { recursive: true });
cpSync(src, dest, { recursive: true });
console.log(`spec-mcp: staged skill ${src} -> ${dest}`);
