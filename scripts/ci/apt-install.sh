#!/usr/bin/env bash
# Resilient apt installs for CI (the IFML nightly flake, run 45993292506).
#
#   scripts/ci/apt-install.sh [--if-missing <command>] <package-or-./file.deb>...
#
# `apt-get update` is all-or-nothing: one third-party source answering 403
# (packages.microsoft.com, preinstalled on the GitHub runner image) fails the
# whole update. Nothing our gates install comes from a third-party source, so
# this script:
#   1. disables every source under sources.list.d except the distro's own
#      (`ubuntu.sources` / `ubuntu-*`) — renamed `*.disabled-by-ci`, never
#      deleted, and left disabled for the rest of the job so a later
#      `playwright install --with-deps` (its own apt-get update) is covered;
#   2. with `--if-missing <command>`, stops there when <command> is on PATH;
#   3. otherwise runs `apt-get update` (retried, backoff) and installs.
#
# Pinned by crates/codegraph/tests/ci_apt_contract.rs. Test hooks:
#   CI_APT_SOURCES_DIR  sources dir (default /etc/apt/sources.list.d)
#   CI_APT_DRY_RUN=1    print the apt commands instead of running them
set -euo pipefail

sources_dir="${CI_APT_SOURCES_DIR:-/etc/apt/sources.list.d}"
if_missing=""
if [ "${1:-}" = "--if-missing" ]; then
  if_missing="${2:?--if-missing needs a command}"
  shift 2
fi
if [ "$#" -eq 0 ]; then
  echo "usage: $0 [--if-missing <command>] <package>..." >&2
  exit 2
fi

run() {
  if [ "${CI_APT_DRY_RUN:-}" = "1" ]; then
    echo "+ $*"
  else
    sudo "$@"
  fi
}

disable_third_party_sources() {
  [ -d "$sources_dir" ] || return 0
  local f base
  for f in "$sources_dir"/*; do
    [ -e "$f" ] || continue
    base="$(basename "$f")"
    case "$base" in
      ubuntu.sources | ubuntu-*.sources | ubuntu.list | ubuntu-*.list) continue ;;
      *.disabled-by-ci) continue ;;
      *.list | *.sources) ;;
      *) continue ;;
    esac
    echo "apt-install: disabling third-party source $base"
    if [ "${CI_APT_DRY_RUN:-}" = "1" ]; then
      mv "$f" "$f.disabled-by-ci"
    else
      sudo mv "$f" "$f.disabled-by-ci"
    fi
  done
}

disable_third_party_sources

if [ -n "$if_missing" ] && command -v "$if_missing" >/dev/null 2>&1; then
  echo "apt-install: $if_missing already installed ($(command -v "$if_missing")) — skipping apt"
  exit 0
fi

attempt=1
until run apt-get update -o Acquire::Retries=3; do
  if [ "$attempt" -ge 3 ]; then
    echo "apt-install: apt-get update failed after $attempt attempts" >&2
    exit 1
  fi
  echo "::warning::apt-get update failed (attempt $attempt); retrying"
  sleep $((attempt * 10))
  attempt=$((attempt + 1))
done
run apt-get install -y "$@"
