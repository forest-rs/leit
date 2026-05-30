#!/usr/bin/env bash
# Git merge driver for Cargo.lock — regenerate from the merged manifests instead
# of attempting a line-based merge (which can produce invalid TOML).
#
# Register once (per clone), then set `Cargo.lock merge=cargo-lock` in
# .gitattributes:
#   git config merge.cargo-lock.name   "regenerate Cargo.lock"
#   git config merge.cargo-lock.driver ".github/hooks/cargo-lock-merge.sh %O %A %B %P"
#
# Args from git: %O=$1 (ancestor)  %A=$2 (ours / output path)  %B=$3 (theirs)  %P=$4 (path)
#
# NB: git invokes merge drivers from the repo root and merges the Cargo.toml
# manifests before this runs, so the lockfile is fully derivable here. This
# driver is a no-op under jj and GitHub server-side merges (they don't use git
# merge drivers).
set -uo pipefail

out="${2:?missing %A output path}"

# Regenerate the worktree lockfile from the (already-merged) manifests, then
# write it to the merge output. Prefer the rustup toolchain via mise; fall back
# to bare cargo (Homebrew) which is fine for this host-only operation.
if mise exec -- cargo generate-lockfile >/dev/null 2>&1 \
  || cargo generate-lockfile >/dev/null 2>&1; then
  cp Cargo.lock "$out"
  exit 0
fi

# Could not regenerate (e.g. offline with an uncached dependency): keep our side
# (already present in %A) rather than emit conflict markers. The pre-push
# `--locked` gate will catch any residual drift before it reaches CI.
exit 0
