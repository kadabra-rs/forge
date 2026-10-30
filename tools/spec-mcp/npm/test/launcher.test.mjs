import { test } from "node:test";
import assert from "node:assert/strict";
import {
  mkdtempSync, rmSync, readFileSync, writeFileSync, mkdirSync,
  symlinkSync, statSync, existsSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { execFile, execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { createServer } from "node:http";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";

import { untarOne, releaseVersion, assetName, installSkill, removeSkill, detectPlatform } from "../bin/spec-mcp.mjs";

const execFileAsync = promisify(execFile);

// Build real tar.gz archives with the system `tar` so the reader is tested
// against genuine tar output, including long-name handling. Long names are
// spelled differently per archive format: GNU format emits type-`L`
// (`./@LongLink`) entries, pax emits type-`x` path headers. GNU tar wants
// `--format=gnu` (it rejects `gnutar`); bsdtar wants the opposite spelling
// (`--format=gnutar`; `gnu` fails), so map the logical format name to the
// platform's. The long-name test runs both formats, so the reader's `L` and
// pax branches are covered on every platform.
const TAR_IS_GNU =
  execFileSync("tar", ["--version"], { encoding: "utf8", stdio: "pipe" }).includes("GNU");
function makeTarGz(files, { format = "gnutar" } = {}) {
  const dir = mkdtempSync(join(tmpdir(), "spec-mcp-tar-"));
  let out = null;
  try {
    for (const [name, content] of Object.entries(files)) {
      const p = join(dir, name);
      mkdirSync(join(p, ".."), { recursive: true });
      writeFileSync(p, content);
    }
    out = join(dir, "..", `out-${Date.now()}-${Math.random().toString(36).slice(2)}.tar.gz`);
    const fmt = format === "gnutar" ? (TAR_IS_GNU ? "gnu" : "gnutar")
      : format === "pax" ? "pax" : null;
    const args = fmt ? [`--format=${fmt}`] : [];
    execFileSync("tar", [...args, "-czf", out, "-C", dir, "."], { stdio: "pipe" });
    return readFileSync(out);
  } finally {
    rmSync(dir, { recursive: true, force: true });
    if (out) rmSync(out, { force: true });
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

for (const format of ["gnutar", "pax"]) {
  test(`untarOne extracts a long-path member (${format})`, () => {
    // Both the directory and the file name exceed the 100-byte ustar name
    // field, so the writer must spell the long path via `L` (gnutar) or
    // pax `x` headers (pax) — whichever format the test asks for.
    const name = `d1/` + "a".repeat(52) + `/` + "b".repeat(64) + `/` + "c".repeat(64) + `/spec-mcp`;
    const gz = makeTarGz({ [name]: "hello" }, { format });
    assert.equal(untarOne(gz, name).toString(), "hello");
  });
}

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

const BIN = fileURLToPath(new URL("../bin/spec-mcp.mjs", import.meta.url));

test("bin runs when invoked through a symlink (npm .bin shim)", () => {
  const dir = mkdtempSync(join(tmpdir(), "spec-mcp-bin-"));
  try {
    const link = join(dir, "spec-mcp");
    symlinkSync(BIN, link);
    const out = execFileSync(process.execPath, [link, "version"], { encoding: "utf8" });
    assert.match(out, /^spec-mcp launcher \d+\.\d+\.\d+/);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

const PLATFORM = detectPlatform();
const TEST_VERSION = "9.9.9";
const TEST_ASSET = assetName(TEST_VERSION, PLATFORM);
// Build the fake release archive on demand: a helper throw must surface as a
// test failure, not as a module-evaluation abort that silently skips
// registration of the remaining tests.
function buildFakeRelease() {
  const bin = makeTarGz({ "spec-mcp": "#!/bin/sh\necho fake\n" });
  return {
    bin,
    sha: createHash("sha256").update(bin).digest("hex"),
    asset: TEST_ASSET,
  };
}

// mode: "ok" | "corrupt" | "missing-entry" | "no-sums"
async function withReleaseServer(fake, mode, fn) {
  let assetHits = 0;
  const server = createServer((req, res) => {
    if (req.url === "/SHA256SUMS.txt") {
      if (mode === "no-sums") { res.writeHead(404).end(); return; }
      const name = mode === "missing-entry" ? "spec-mcp-9.9.9-bogus.tar.gz" : TEST_ASSET;
      res.end(`${fake.sha}  ${name}\n`);
      return;
    }
    if (req.url === `/${fake.asset}`) {
      assetHits += 1;
      if (mode === "corrupt") res.end(Buffer.concat([fake.bin, Buffer.from("x")]));
      else res.end(fake.bin);
      return;
    }
    res.writeHead(404).end();
  });
  await new Promise((r) => server.listen(0, "127.0.0.1", r));
  try {
    await fn(`http://127.0.0.1:${server.address().port}`, () => assetHits);
  } finally {
    // close() stops new connections but leaves established ones open, which
    // would pin the test process's event loop; closeAllConnections ends them.
    server.close();
    server.closeAllConnections?.();
  }
}

// The fake release server lives in the test process, so the launcher must be
// spawned asynchronously: a synchronous wait would block the event loop that
// services the server. Rejections carry code/stdout/stderr for the asserts.
const runLauncher = (args, env) =>
  execFileAsync(process.execPath, [BIN, ...args], {
    env: { ...process.env, ...env }, encoding: "utf8",
  });

test("ensureBinary downloads, verifies, and caches", async () => {
  const cache = mkdtempSync(join(tmpdir(), "spec-mcp-cache-"));
  try {
    await withReleaseServer(buildFakeRelease(), "ok", async (base, hits) => {
      await runLauncher(["install"], {
        SPEC_MCP_CACHE_DIR: cache, SPEC_MCP_VERSION: TEST_VERSION, SPEC_MCP_BASE_URL: base,
      });
      const expected = join(cache, `${TEST_VERSION}-${PLATFORM}`, "spec-mcp");
      assert.ok(existsSync(expected), "binary cached after install");
      assert.equal(statSync(expected).mode & 0o755, 0o755, "cached binary is executable");
      // second run: no re-fetch
      await runLauncher(["install"], {
        SPEC_MCP_CACHE_DIR: cache, SPEC_MCP_VERSION: TEST_VERSION, SPEC_MCP_BASE_URL: base,
      });
      assert.equal(hits(), 1, "cached binary reused; no second asset download");
    });
  } finally {
    rmSync(cache, { recursive: true, force: true });
  }
});

test("ensureBinary refuses a checksum-mismatched download", async () => {
  const cache = mkdtempSync(join(tmpdir(), "spec-mcp-cache-"));
  try {
    await withReleaseServer(buildFakeRelease(), "corrupt", async (base) => {
      let e;
      try {
        await runLauncher(["install"], {
          SPEC_MCP_CACHE_DIR: cache, SPEC_MCP_VERSION: TEST_VERSION, SPEC_MCP_BASE_URL: base,
        });
      } catch (err) {
        e = err;
      }
      assert.ok(e, "launcher must fail on checksum mismatch");
      assert.equal(e.code, 1);
      assert.match(e.stderr, /checksum mismatch/);
      assert.ok(
        !existsSync(join(cache, `${TEST_VERSION}-${PLATFORM}`, "spec-mcp")),
        "no cached binary after a failed verification"
      );
    });
  } finally {
    rmSync(cache, { recursive: true, force: true });
  }
});

test("ensureBinary refuses an asset missing from SHA256SUMS.txt", async () => {
  const cache = mkdtempSync(join(tmpdir(), "spec-mcp-cache-"));
  try {
    await withReleaseServer(buildFakeRelease(), "missing-entry", async (base) => {
      let e;
      try {
        await runLauncher(["install"], {
          SPEC_MCP_CACHE_DIR: cache, SPEC_MCP_VERSION: TEST_VERSION, SPEC_MCP_BASE_URL: base,
        });
      } catch (err) {
        e = err;
      }
      assert.ok(e, "launcher must fail when the asset is unlisted");
      assert.match(e.stderr, /is not listed in the published SHA256SUMS\.txt/);
      assert.ok(
        !existsSync(join(cache, `${TEST_VERSION}-${PLATFORM}`, "spec-mcp")),
        "no cached binary after a failed verification"
      );
    });
  } finally {
    rmSync(cache, { recursive: true, force: true });
  }
});

test("ensureBinary refuses a download without published sums", async () => {
  const cache = mkdtempSync(join(tmpdir(), "spec-mcp-cache-"));
  try {
    await withReleaseServer(buildFakeRelease(), "no-sums", async (base) => {
      let e;
      try {
        await runLauncher(["install"], {
          SPEC_MCP_CACHE_DIR: cache, SPEC_MCP_VERSION: TEST_VERSION, SPEC_MCP_BASE_URL: base,
        });
      } catch (err) {
        e = err;
      }
      assert.ok(e, "launcher must fail without SHA256SUMS.txt");
      assert.match(e.stderr, /refusing unverified download/);
      assert.ok(
        !existsSync(join(cache, `${TEST_VERSION}-${PLATFORM}`, "spec-mcp")),
        "no cached binary after a failed verification"
      );
    });
  } finally {
    rmSync(cache, { recursive: true, force: true });
  }
});

test("SPEC_MCP_BINARY bypasses the download entirely", async () => {
  const dir = mkdtempSync(join(tmpdir(), "spec-mcp-dummy-"));
  const dummy = join(dir, "dummy-spec-mcp");
  writeFileSync(dummy, "#!/bin/sh\necho dummy\n");
  try {
    const out = await runLauncher(["path"], { SPEC_MCP_BINARY: dummy });
    assert.equal(out.stdout, `${dummy}\n`);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});
