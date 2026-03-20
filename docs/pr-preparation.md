## PR Preparation

This document defines the expected local workflow before pushing a PR update.
The goal is to catch CI failures locally, keep history reviewable, and avoid
late fixup commits for formatting, lint, or lockfile drift.

### Required Local Verification

From the workspace root, run the full local gate before pushing:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo doc --workspace --locked --all-features --no-deps --document-private-items
cargo test --workspace --all-features
```

These commands mirror the checks that most often block the PR:

- `cargo fmt --all --check` catches formatting drift.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` catches
  lint issues across library code, tests, and examples.
- `cargo doc --workspace --locked --all-features --no-deps --document-private-items`
  catches documentation warnings and lockfile drift.
- `cargo test --workspace --all-features` catches behavioral regressions.

If a narrower local command is useful during development, use it as a fast
iteration tool, but do not treat it as a replacement for the full pre-push run.

### Commit Hygiene

Each commit should carry the mechanical updates required by its own crate or
workspace changes.

In practice, that means:

- If a commit changes code in a crate, make sure the committed version is
  already formatted.
- If a commit changes dependencies, workspace membership, or anything else that
  affects resolution, include the matching `Cargo.lock` update in the same
  logical commit.
- Do not rely on a final "fix CI" commit for formatting or lockfile drift if
  those updates belong to earlier commits.

Good outcomes:

- A crate-introducing commit already includes its lockfile update.
- A test-heavy commit already includes the clippy-clean version of those tests.
- A query or scoring commit already includes any required rustfmt rewrite.

Bad outcomes:

- A top-of-stack "fmt" commit that only cleans up earlier commits.
- A late `Cargo.lock` fixup after dependency edits have already been pushed.
- A PR update that passes local targeted checks but fails the full workspace
  gate in CI.

### When `Cargo.lock` Must Move With the Commit

`Cargo.lock` should be updated in the same logical change when a commit:

- adds a new crate to the workspace
- adds or removes dependencies
- changes dependency versions or features
- changes workspace dependency configuration in a way that affects resolution

If `cargo doc --locked` or `cargo check --locked` fails locally, treat that as a
signal that the current commit stack has unresolved lockfile drift.

### Suggested Pre-Push Checklist

Before pushing:

1. Make sure each commit is focused and reviewable.
2. Make sure formatting and lockfile updates live with the commits that require
   them.
3. Run the full local verification sequence.
4. Push only after the local workspace is green.

### Optional Jujutsu Workflow

This section is only for contributors using a `jj`-managed Git repo. Git-only
contributors can ignore it.

Recommended `jj` workflow:

- Use `jj new` to start a new logical change instead of piling unrelated work
  into the current one.
- Use `jj commit <paths> -m "..."` when only part of the working copy belongs in
  the current change.
- Use `jj absorb` to move small formatting, lint, or lockfile fixups back into
  the commits that introduced the affected files.
- Use `jj abandon` to remove empty fixup commits after absorption.
- Remember that bookmarks do not move automatically; verify bookmark placement
  before pushing.

This keeps the pushed stack reviewable and avoids top-of-stack CI cleanup
commits that obscure the actual feature history.
