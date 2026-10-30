#!/usr/bin/env node
// spec-mcp launcher: resolves the self-contained spec MCP binary for this
// platform (download + SHA-256 verify + cache, or SPEC_MCP_BINARY), runs it
// over stdio, and can install/remove the bundled `sysml-modeling` agent skill.
//
//   npx @kadabra-rs/spec-mcp                      # serve MCP on stdio
//   npx @kadabra-rs/spec-mcp install              # fetch binary only
//   npx @kadabra-rs/spec-mcp skill install        # skill -> agent dirs (user-level)
//   npx @kadabra-rs/spec-mcp skill install --project --agent omp
//   npx @kadabra-rs/spec-mcp skill remove
//   npx @kadabra-rs/spec-mcp path                 # print binary path
//
// Environment:
//   SPEC_MCP_BINARY    absolute path to a prebuilt spec-mcp (skips download)
//   SPEC_MCP_VERSION   override the release tag version (e.g. 0.2.0)
//   SPEC_MCP_BASE_URL  override the download base URL (mirrors/tests)
//   SPEC_MCP_CACHE_DIR override the cache dir (default ~/.cache/spec-mcp)

import { spawn } from "node:child_process";
import { createHash } from "node:crypto";
import {
  existsSync, mkdirSync, readFileSync, rmSync, writeFileSync, chmodSync,
  realpathSync, renameSync,
} from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { homedir } from "node:os";

const HERE = dirname(fileURLToPath(import.meta.url));
const PKG = JSON.parse(readFileSync(join(HERE, "..", "package.json"), "utf8"));
const SKILL_NAME = "sysml-modeling";
const GITHUB_REPO = "kadabra-rs/forge";

const err = (msg) => process.stderr.write(`${msg}\n`);
const fail = (msg) => { err(`spec-mcp: ${msg}`); process.exit(1); };

export function detectPlatform() {
  const arch = process.arch === "arm64" ? "arm64"
    : process.arch === "x64" ? "x64" : null;
  const os = process.platform === "darwin" ? "darwin"
    : process.platform === "linux" ? "linux" : null;
  if (!os || !arch) {
    fail(`no prebuilt spec-mcp binary for ${process.platform}/${process.arch}; \
build it (cargo build --release -p spec-mcp in the forge repo, tools/spec-mcp) \
and set SPEC_MCP_BINARY, or run: cargo install --git https://github.com/${GITHUB_REPO} --bin spec-mcp`);
  }
  if (os === "linux" && !process.report?.getReport?.().header?.glibcVersionRuntime) {
    fail(`no prebuilt spec-mcp binary for linux/${arch}; build it \
(cargo build --release -p spec-mcp in the forge repo, tools/spec-mcp) and set SPEC_MCP_BINARY`);
  }
  return `${os}-${arch}`;
}

export function releaseVersion() {
  return process.env.SPEC_MCP_VERSION ?? PKG.version;
}

export function assetName(version, platform) {
  return `spec-mcp-${version}-${platform}.tar.gz`;
}

export function cacheDir() {
  return process.env.SPEC_MCP_CACHE_DIR ?? join(homedir(), ".cache", "spec-mcp");
}

function fetchBuffer(url) {
  return (async () => {
    const res = await fetch(url);
    if (!res.ok) throw new Error(`HTTP ${res.status} for ${url}`);
    return Buffer.from(await res.arrayBuffer());
  })();
}

function sha256(buf) {
  return createHash("sha256").update(buf).digest("hex");
}

// Extract one named member from a tar.gz using only node builtins: gzip via
// zlib, then a minimal USTAR/PAX reader (release archives hold one regular
// file, but pax size/longname overrides are honored defensively).
import { gunzipSync } from "node:zlib";

export function untarOne(gz, memberName) {
  const tar = gunzipSync(gz);
  let off = 0;
  let paxName = null;
  let paxSize = null;
  while (off + 512 <= tar.length) {
    const h = tar.subarray(off, off + 512);
    if (h[0] === 0) break; // end-of-archive zero block
    const field = (s, e) => h.toString("utf8", s, e).split("\0")[0].trim();
    const rawName = field(0, 100);
    const prefix = field(345, 500);
    const size = parseInt(field(124, 136), 8) || 0;
    const type = String.fromCharCode(h[156]);
    off += 512;
    const dataStart = off;
    // GNU longname and pax extended headers describe the *next* entry.
    if (type === "L") {
      paxName = tar.subarray(dataStart, dataStart + size).toString("utf8").split("\0")[0];
      off += Math.ceil(size / 512) * 512;
      continue;
    }
    if (type === "x" || type === "g") {
      const pax = tar.subarray(dataStart, dataStart + size).toString("utf8");
      const m = /(?:^|\n)size=(\d+)/.exec(pax);
      paxSize = m ? parseInt(m[1], 10) : null;
      const p = /(?:^|\n)path=([^\n]+)/.exec(pax);
      if (p) paxName = p[1];
      off += Math.ceil(size / 512) * 512;
      continue;
    }
    const entryName = paxName ?? (prefix ? `${prefix}/` : "") + rawName;
    const normalized = entryName.replace(/^\.\//, "");
    const isFile = type === "0" || type === "\0";
    const effectiveSize = paxSize ?? size;
    paxName = null;
    paxSize = null;
    if (isFile && normalized === memberName) {
      return tar.subarray(dataStart, dataStart + effectiveSize);
    }
    off += Math.ceil(size / 512) * 512;
  }
  throw new Error(`member ${memberName} not found in archive`);
}

export async function ensureBinary({ quiet = false } = {}) {
  if (process.env.SPEC_MCP_BINARY) {
    const p = process.env.SPEC_MCP_BINARY;
    if (!existsSync(p)) fail(`SPEC_MCP_BINARY points at a missing file: ${p}`);
    return p;
  }
  const version = releaseVersion();
  const platform = detectPlatform();
  const dir = join(cacheDir(), `${version}-${platform}`);
  const bin = join(dir, "spec-mcp");
  if (existsSync(bin)) return bin;

  const base = process.env.SPEC_MCP_BASE_URL ??
    `https://github.com/${GITHUB_REPO}/releases/download/spec-mcp-v${version}`;
  const asset = assetName(version, platform);
  err(`spec-mcp: downloading ${asset} (v${version})`);
  const [archive, sums] = await Promise.all([
    fetchBuffer(`${base}/${asset}`),
    fetchBuffer(`${base}/SHA256SUMS.txt`).catch(() => null),
  ]);
  if (sums) {
    const expected = sums.toString("utf8").split("\n")
      .map((l) => l.trim().split(/\s+/))
      .filter(([h, f]) => h && f && (f === asset || f.endsWith("/" + asset)))
      .map(([h]) => h)[0];
    if (!expected) fail(`${asset} is not listed in the published SHA256SUMS.txt`);
    const got = sha256(archive);
    if (got !== expected) fail(`checksum mismatch for ${asset}: expected ${expected}, got ${got}`);
    if (!quiet) err(`spec-mcp: checksum OK`);
  } else {
    fail("could not fetch SHA256SUMS.txt from the release; refusing unverified download");
  }

  const data = untarOne(archive, "spec-mcp");
  mkdirSync(dir, { recursive: true });
  const tmp = `${bin}.tmp-${process.pid}`;
  try {
    writeFileSync(tmp, data);
    chmodSync(tmp, 0o755);
    renameSync(tmp, bin);
  } catch (e) {
    rmSync(tmp, { force: true });
    throw e;
  }
  if (!quiet) err(`spec-mcp: cached at ${bin}`);
  return bin;
}

// -- skill install ------------------------------------------------------------

export function skillSource() {
  const packaged = join(HERE, "..", "skill", "SKILL.md");
  if (existsSync(packaged)) return packaged;
  // Inside the forge repo checkout, fall back to the canonical skill location.
  const canonical = join(HERE, "..", "..", "..", "..", ".omp", "skills", SKILL_NAME, "SKILL.md");
  if (existsSync(canonical)) return canonical;
  fail(`bundled skill missing from package (${packaged})`);
}

export function skillTargets(scope, agents) {
  const home = homedir();
  const roots = {
    omp: scope === "project" ? ".omp/skills" : join(home, ".omp/agent/skills"),
    claude: scope === "project" ? ".claude/skills" : join(home, ".claude/skills"),
    codex: scope === "project" ? ".codex/skills" : join(home, ".codex/skills"),
    agents: scope === "project" ? ".agents/skills" : join(home, ".agents/skills"),
  };
  const list = agents.includes("all")
    ? ["omp", "claude", "codex", "agents"] : agents;
  for (const a of list) if (!Object.hasOwn(roots, a)) fail(`unknown agent '${a}' (omp|claude|codex|agents|all)`);
  return list.map((a) => ({ agent: a, dir: join(roots[a], SKILL_NAME) }));
}

export function installSkill({ scope = "user", agents = ["omp"], force = false } = {}) {
  const src = skillSource();
  const content = readFileSync(src);
  const results = [];
  for (const { agent, dir } of skillTargets(scope, agents)) {
    const target = join(dir, "SKILL.md");
    let state = "installed";
    if (existsSync(target)) {
      const cur = readFileSync(target);
      if (cur.equals(content)) { state = "up-to-date"; }
      else if (!force) { state = "skipped (modified locally; --force to overwrite)"; }
      else { writeFileSync(target, content); state = "overwritten"; }
    } else {
      mkdirSync(dir, { recursive: true });
      writeFileSync(target, content);
      state = "installed";
    }
    results.push({ agent, scope, path: target, state });
  }
  return results;
}

export function removeSkill({ scope = "user", agents = ["omp"] } = {}) {
  const results = [];
  for (const { agent, dir } of skillTargets(scope, agents)) {
    const existed = existsSync(join(dir, "SKILL.md"));
    if (existed) rmSync(dir, { recursive: true, force: true });
    results.push({ agent, scope, path: dir, state: existed ? "removed" : "absent" });
  }
  return results;
}

// -- arg parsing ---------------------------------------------------------------

function parseArgs(argv) {
  const opts = { scope: "user", agents: [], force: false };
  const positional = [];
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a === "--project") opts.scope = "project";
    else if (a === "--user") opts.scope = "user";
    else if (a === "--force") opts.force = true;
    else if (a === "--agent") {
      const v = argv[++i];
      if (!v) fail("--agent requires a value (omp|claude|codex|agents|all, repeatable)");
      if (v === "all") opts.agents = ["all"];
      else opts.agents.push(...v.split(","));
    } else if (a === "--help" || a === "-h") {
      positional.length = 0;
      positional.push("help");
    }
    else if (a.startsWith("-")) fail(`unknown option: ${a} (see --help)`);
    else positional.push(a);
  }
  if (opts.agents.length === 0) opts.agents = ["omp"];
  return { positional, opts };
}

const USAGE = `spec-mcp ${PKG.version} — OMG KerML/SysML v2 specification MCP server

usage: spec-mcp [command] [options]

commands
  (default)          serve the MCP server over stdio (for MCP client config)
  install            download + verify the platform binary only
  skill install      install the '${SKILL_NAME}' agent skill
  skill remove       remove the '${SKILL_NAME}' skill
  path               print the resolved binary path
  version            print the launcher version

options (skill commands)
  --user             install to user-level agent dirs (default)
  --project          install to <cwd>/.<agent>/skills (checked in with your repo)
  --agent <a>        omp|claude|codex|agents|all (default omp; repeatable/comma list)
  --force            overwrite locally modified SKILL.md

MCP client config (project .mcp.json / .omp/mcp.json):
  { "mcpServers": { "spec": { "command": "npx", "args": ["-y", "@kadabra-rs/spec-mcp"] } } }
bun: use "command": "bunx", same args.
`;

async function main() {
  const { positional, opts } = parseArgs(process.argv.slice(2));
  const cmd = positional.join(" ") || "serve";
  switch (cmd) {
    case "help":
      process.stdout.write(USAGE);
      return;
    case "version":
      process.stdout.write(`spec-mcp launcher ${PKG.version}\n`);
      return;
    case "path":
      process.stdout.write(`${await ensureBinary({ quiet: true })}\n`);
      return;
    case "install":
      await ensureBinary();
      return;
    case "skill install":
      for (const r of installSkill(opts)) err(`spec-mcp: skill ${r.state}: ${r.path}`);
      err(`spec-mcp: skill '${SKILL_NAME}' ready for ${opts.agents.join(", ")} (${opts.scope}-level)`);
      return;
    case "skill remove":
      for (const r of removeSkill(opts)) err(`spec-mcp: skill ${r.state}: ${r.path}`);
      return;
    case "serve": {
      const bin = await ensureBinary({ quiet: true });
      const args = process.env.SPEC_MCP_CRATES_DIR ? [process.env.SPEC_MCP_CRATES_DIR] : [];
      const child = spawn(bin, args, { stdio: "inherit" });
      child.on("error", (e) => fail(`failed to exec ${bin}: ${e.message}`));
      for (const sig of ["SIGINT", "SIGTERM", "SIGHUP"]) {
        process.on(sig, () => {
          if (child.exitCode === null && child.signalCode === null) child.kill(sig);
          else {
            // Re-raised from the exit handler below: restore default and die.
            process.removeAllListeners(sig);
            process.kill(process.pid, sig);
          }
        });
      }
      child.on("exit", (code, signal) => {
        if (signal) process.kill(process.pid, signal);
        else process.exit(code ?? 1);
      });
      return;
    }
    default:
      fail(`unknown command: ${cmd} (see --help)`);
  }
}

// Run only when executed as a program; importers (tests) get the functions.
// npm exposes bins through symlinks while Node realpath-resolves the main
// module, so compare realpaths rather than the URL of the invoked path.
if (process.argv[1] && realpathSync(process.argv[1]) === fileURLToPath(import.meta.url)) {
  await main();
}
