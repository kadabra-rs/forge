#!/bin/sh
# Installs the external SysML v2 tooling used by the forge agent workflow into
# $HOME/.local/bin: spec42 (parser/validator CLI; replaces the removed
# spec42-mcp server, which upstream retired in v0.50.0) and sysand (the SysML v2/KerML
# package manager). Both are fetched at their latest release; the script never touches
# shell profiles.
#
# Configuration via environment variables:
#   SPEC42_VERSION   Release version to install (e.g. 0.57.0 or v0.57.0).
#                    Default: latest non-prerelease.
#   SYSAND_VERSION   Release version to install (e.g. 0.2.1).
#                    Default: latest non-prerelease.
#   FORCE            Set to 1 to re-download even when the desired version is
#                    already installed.
#
# Usage:
#   sh tools/install-sysml-tooling.sh
#
# Checksums: spec42 release assets are verified against the published
# SHA256SUMS.txt. Sysand publishes no checksums; its archive integrity relies
# on the HTTPS GitHub release download (same posture as sysand.com/install.sh).

set -eu

INSTALL_DIR="${HOME:-}/.local/bin"
TMP_DIR=""
STAGING=""

say() { printf '%s\n' "$*"; }
fail() { printf 'error: %s\n' "$*" >&2; exit 1; }

cleanup() {
  [ -z "$TMP_DIR" ] || rm -rf "$TMP_DIR"
  [ -z "$STAGING" ] || rm -f "$STAGING"
}
trap cleanup EXIT

need_cmd() {
  command -v "$1" >/dev/null 2>&1 || fail "$1 is required"
}

detect_os() {
  case "$(uname -s)" in
    Linux)  os="linux" ;;
    Darwin) os="macos" ;;
    *) fail "unsupported operating system: $(uname -s)" ;;
  esac
}

detect_arch() {
  case "$(uname -m)" in
    x86_64|amd64)
      if [ "$os" = "macos" ] &&
        [ "$(sysctl -n sysctl.proc_translated 2>/dev/null || true)" = "1" ]; then
        arch="arm64"
      else
        arch="x86_64"
      fi
      ;;
    arm64|aarch64) arch="arm64" ;;
    *) fail "unsupported architecture: $(uname -m)" ;;
  esac
}

# Pick the gnu build when glibc 2.35+ is available, the static musl build
# otherwise (mirrors the official sysand installer's rule).
detect_libc() {
  libc="musl"
  if [ "$os" = "macos" ]; then
    return 0
  fi
  glibc_output="$(getconf GNU_LIBC_VERSION 2>/dev/null || true)"
  case "$glibc_output" in
    glibc\ *) glv="${glibc_output#glibc }" ;;
    *) glv="" ;;
  esac
  [ -n "$glv" ] || return 0
  major="${glv%%.*}"
  minor="${glv#*.}"
  minor="${minor%%[!0-9]*}"
  case "$major" in ''|*[!0-9]*) return 0 ;; esac
  if [ "$major" -gt 2 ] || { [ "$major" -eq 2 ] && [ "${minor:-0}" -ge 35 ]; }; then
    libc="gnu"
  fi
}

# Resolve the latest release tag of owner/repo from the /releases/latest
# redirect, avoiding the rate-limited unauthenticated GitHub API. The header
# line ends with CRLF, so strip carriage returns before extracting the tag.
latest_tag() {
  curl -fsSI "https://github.com/$1/releases/latest" 2>/dev/null |
    tr -d '\r' |
    sed -n 's/.*\/tag\/\(v[0-9][^ "]*\).*/\1/p' | head -n1
}

normalize_tag() {
  case "$1" in
    v*) printf '%s' "$1" ;;
    *)  printf 'v%s' "$1" ;;
  esac
}

download() {
  curl -fsSL "$1" -o "$2" || fail "failed to download $1"
}

sha256_of() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | cut -d' ' -f1
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$1" | cut -d' ' -f1
  else
    fail "sha256sum or shasum is required"
  fi
}

# Atomic same-filesystem rename; the destination never holds a partial binary.
atomic_install() {
  mkdir -p "$INSTALL_DIR"
  STAGING="$(mktemp "${INSTALL_DIR}/.tmp.$2.XXXXXX")"
  cp "$1" "$STAGING"
  chmod 755 "$STAGING"
  mv -f "$STAGING" "${INSTALL_DIR}/$2"
  STAGING=""
}

installed_has_version() {
  [ -x "${INSTALL_DIR}/$1" ] || return 1
  v="$("${INSTALL_DIR}/$1" --version 2>/dev/null)" || return 1
  [ "${v##* }" = "$2" ]
}

install_spec42() {
  say "== spec42"
  if [ -n "${SPEC42_VERSION:-}" ]; then
    tag="$(normalize_tag "$SPEC42_VERSION")"
  else
    tag="$(latest_tag elan8/spec42)"
    [ -n "$tag" ] || fail "could not resolve the latest spec42 release"
  fi
  ver="${tag#v}"

  case "$os:$arch" in
    linux:x86_64) target="linux-x64" ;;
    macos:arm64)  target="darwin-arm64" ;;
    macos:x86_64) target="darwin-x64" ;;
    *) say "warning: spec42 publishes no ${os}/${arch} build; skipping spec42"; return 0 ;;
  esac
  asset="spec42-${ver}-${target}.tar.gz"
  base="https://github.com/elan8/spec42/releases/download/${tag}"

  if [ "${FORCE:-0}" != "1" ] && installed_has_version spec42 "$ver"; then
    say "spec42 ${ver} already installed, skipping (FORCE=1 to reinstall)"
    return 0
  fi

  say "fetching ${asset} (${tag})"
  download "${base}/${asset}" "${TMP_DIR}/${asset}"
  download "${base}/SHA256SUMS.txt" "${TMP_DIR}/spec42-SHA256SUMS.txt"
  want="$(awk -v a="$asset" 'length($2) >= length(a) && substr($2, length($2) - length(a) + 1) == a && (length($2) == length(a) || substr($2, length($2) - length(a), 1) == "/") { print $1; exit }' "${TMP_DIR}/spec42-SHA256SUMS.txt")"
  [ -n "$want" ] || fail "asset ${asset} not listed in the published SHA256SUMS.txt"
  got="$(sha256_of "${TMP_DIR}/${asset}")"
  [ "$want" = "$got" ] ||
    fail "checksum mismatch for ${asset}: expected ${want}, got ${got}"
  say "checksum OK (${got})"

  mkdir -p "${TMP_DIR}/spec42-x"
  tar -xzf "${TMP_DIR}/${asset}" -C "${TMP_DIR}/spec42-x"
  [ -f "${TMP_DIR}/spec42-x/spec42" ] ||
    fail "spec42 binary not found at the archive root"
  atomic_install "${TMP_DIR}/spec42-x/spec42" spec42
  v="$("${INSTALL_DIR}/spec42" --version)" ||
    fail "installed spec42 does not run on this system"
  say "installed $v"
}

install_sysand() {
  say "== sysand"
  if [ -n "${SYSAND_VERSION:-}" ]; then
    tag="$(normalize_tag "$SYSAND_VERSION")"
  else
    tag="$(latest_tag sensmetry/sysand)"
    [ -n "$tag" ] || fail "could not resolve the latest sysand release"
  fi

  if [ "$os" = "linux" ]; then
    asset="sysand-linux-${arch}-${libc}.tar.gz"
  else
    asset="sysand-macos-${arch}.tar.gz"
  fi
  base="https://github.com/sensmetry/sysand/releases/download/${tag}"

  if [ "${FORCE:-0}" != "1" ] && installed_has_version sysand "${tag#v}"; then
    say "sysand ${tag} already installed, skipping (FORCE=1 to reinstall)"
    return 0
  fi

  say "fetching ${asset} (${tag})"
  download "${base}/${asset}" "${TMP_DIR}/${asset}"
  mkdir -p "${TMP_DIR}/sysand-x"
  tar -xzf "${TMP_DIR}/${asset}" -C "${TMP_DIR}/sysand-x"
  [ -f "${TMP_DIR}/sysand-x/sysand" ] ||
    fail "sysand binary not found at the archive root"
  atomic_install "${TMP_DIR}/sysand-x/sysand" sysand
  v="$("${INSTALL_DIR}/sysand" --version)" ||
    fail "installed sysand does not run on this system"
  say "installed $v"
}

main() {
  case "${1:-}" in
    -h|--help)
      sed -n '2,/^$/p' "$0" | sed 's/^# \{0,1\}//'
      exit 0
      ;;
    "") ;;
    *) fail "unknown option: $1 (configuration is via environment variables, see --help)" ;;
  esac

  need_cmd curl
  need_cmd uname
  need_cmd tar
  need_cmd mktemp
  [ -n "${HOME:-}" ] || fail "HOME must be set"

  detect_os
  detect_arch
  detect_libc

  TMP_DIR="$(mktemp -d)"

  install_spec42
  install_sysand

  case ":${PATH:-}:" in
    *":${INSTALL_DIR}:"*) ;;
    *) say "note: add ${INSTALL_DIR} to PATH to use the tools in this shell" ;;
  esac
  if [ -e "${INSTALL_DIR}/spec42-mcp" ]; then
    say "warning: ${INSTALL_DIR}/spec42-mcp is obsolete (upstream removed the MCP server in spec42 v0.50.0); delete it"
  fi

  say "done."
}

main "$@"
