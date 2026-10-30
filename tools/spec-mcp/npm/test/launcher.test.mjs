import { test } from "node:test";
import assert from "node:assert/strict";
import { gzipSync } from "node:zlib";
import { mkdtempSync, rmSync, readFileSync, writeFileSync, mkdirSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { execFileSync } from "node:child_process";
import { untarOne, releaseVersion, assetName, installSkill, removeSkill } from "../bin/spec-mcp.mjs";

// Build real tar.gz archives with the system `tar` so the reader is tested
// against genuine GNU tar output, including long-name handling (GNU tar
// emits pax headers for paths that don't fit the ustar name field).
function makeTarGz(files, { format = "gnu" } = {}) {
  const dir = mkdtempSync(join(tmpdir(), "spec-mcp-tar-"));
  try {
    for (const [name, content] of Object.entries(files)) {
      const p = join(dir, name);
      mkdirSync(join(p, ".."), { recursive: true });
      writeFileSync(p, content);
    }
    const out = join(dir, "..", `out-${Date.now()}-${Math.random().toString(36).slice(2)}.tar.gz`);
    const args = format ? [`--format=${format}`] : [];
    execFileSync("tar", [...args, "-czf", out, "-C", dir, "."], { stdio: "pipe" });
    return readFileSync(out);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

test("untarOne extracts a plain member", () => {
  const gz = makeTarGz({ "spec-mcp": "#!/bin/sh\necho hi\n", other: "x" });
  assert.equal(untarOne(gz, "spec-mcp").toString(), "#!/bin/sh\necho hi\n");
});

test("untarOne extracts a large (>512-byte) member", () => {
  const data = "A".repeat(600);
  const gz = makeTarGz({ "spec-mcp": data });
  assert.equal(untarOne(gz, "spec-mcp").toString(), data);
});

test("untarOne extracts a long-path member via pax headers", () => {
  const name = `deep/${"n".repeat(60)}/${"x".repeat(60)}/spec-mcp`;
  const gz = makeTarGz({ [name]: "hello" });
  assert.equal(untarOne(gz, name).toString(), "hello");
});

test("untarOne throws for missing member", () => {
  const gz = makeTarGz({ other: "x" });
  assert.throws(() => untarOne(gz, "spec-mcp"), /not found/);
});

test("asset naming follows the release convention", () => {
  const v = releaseVersion();
  assert.match(v, /^\d+\.\d+\.\d+/);
  assert.equal(assetName("1.2.3", "linux-x64"), "spec-mcp-1.2.3-linux-x64.tar.gz");
  assert.equal(assetName("1.2.3", "darwin-arm64"), "spec-mcp-1.2.3-darwin-arm64.tar.gz");
});

test("skill install/remove round-trip against a fake HOME", (t) => {
  const home = mkdtempSync(join(tmpdir(), "spec-mcp-home-"));
  t.after(() => rmSync(home, { recursive: true, force: true }));
  const savedHome = process.env.HOME;
  process.env.HOME = home;
  try {
    const results = installSkill({ scope: "user", agents: ["omp"] });
    assert.equal(results.length, 1);
    assert.equal(results[0].state, "installed");
    assert.match(readFileSync(results[0].path, "utf8"), /^---\nname: sysml-modeling/);
    // idempotent second run
    assert.equal(installSkill({ scope: "user", agents: ["omp"] })[0].state, "up-to-date");
    // local modification is protected without --force
    const target = results[0].path;
    writeFileSync(target, "---\nname: sysml-modeling\nmodified\n");
    assert.match(installSkill({ scope: "user", agents: ["omp"] })[0].state, /^skipped/);
    assert.equal(installSkill({ scope: "user", agents: ["omp"], force: true })[0].state, "overwritten");
    // multi-agent install + remove all
    assert.equal(installSkill({ scope: "user", agents: ["claude", "codex"] }).length, 2);
    const removed = removeSkill({ scope: "user", agents: ["all"] });
    assert.equal(removed.length, 4);
    assert.ok(removed.every((r) => ["removed", "absent"].includes(r.state)));
    assert.equal(removeSkill({ scope: "user", agents: ["omp"] })[0].state, "absent");
  } finally {
    process.env.HOME = savedHome;
  }
});
