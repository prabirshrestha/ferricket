---
name: release-new-version
description: Release a new version of the Ferricket Rust crate from this repository by pulling latest, confirming CI is green, bumping Cargo.toml, refreshing Cargo.lock, verifying the embedded frontend, committing, pushing, waiting for commit CI, then pushing a tag that triggers GitHub Actions publishing. Use for explicit invocations like "/release-new-version" or requests like "release a new version", "bump Cargo.toml", or "publish ferricket".
---

# Release New Version

## Overview

Release the `ferricket` crate and `fer` executable with a pull-first flow, explicit version bump, lockfile refresh, verified embedded frontend, semantic release commit, and a release tag push that triggers automated publishing.

## Preconditions

- Work from the repository root unless the user explicitly directs otherwise.
- Run this release flow from the `main` branch only. If the current branch is not `main`, stop and ask before switching branches.
- Treat the release as live unless the user asks for a dry run.
- Inspect `git status --short --branch` before changing files.
- If the worktree has user changes, do not overwrite them. Ask before mixing release edits into a dirty worktree.
- Pull before editing when the branch tracks a remote: `git pull --ff-only`.
- Keep unrelated untracked files out of the release commit.

## Upstream CI Gate

After pulling latest, check that CI for the current `main` commit is green before making release edits.

Use GitHub CLI when available:

```bash
gh run list --workflow ci.yaml --branch main --limit 5 --json databaseId,headSha,status,conclusion,workflowName,displayTitle,createdAt,url
```

Confirm the latest CI run for `HEAD` completed with `success`. If its status cannot be determined or is not green, stop and ask the user whether to run the full local CI-equivalent checks before continuing.

## Version Bump

1. Read the current version from the root `Cargo.toml` package table.
2. Inspect published versions with `cargo search ferricket --limit 1`, tags with `git tag --sort=-version:refname | head`, and commits since the latest release tag.
3. Use the version requested by the user. Otherwise, choose the semantic version bump from commit impact and tell the user which bump you chose.
4. Update only the root `Cargo.toml` package `version`.
5. Refresh the workspace package versions in `Cargo.lock`, then build Ferricket through the repository workflow:

```bash
make lock
make build
```

Verify that `Cargo.lock` changed only for the `ferricket` package version.

6. Run `cargo package --locked --allow-dirty` and inspect the packaged file list. Confirm the committed `web/dist` assets are included and no required source is omitted.
7. Verify that only `Cargo.toml` and `Cargo.lock` changed before continuing. A release must use the already-reviewed committed frontend bundle; do not silently include unrelated regenerated assets.

## Validation

When upstream `HEAD` CI was green and the release edit is only the version bump plus lockfile refresh, the required `make build` and package check above are enough before committing.

If upstream CI was not green, could not be confirmed, or the release edit includes anything beyond the version bump and lockfile refresh, ask the user before running the full CI-equivalent checks from `.github/workflows/ci.yaml`:

```bash
make check
git diff --exit-code -- web/dist
```

## Publishing

Do not run `cargo publish` manually. This repository publishes from GitHub Actions when a matching release tag is pushed.

The workflow tag patterns are:

```text
v0.[0-9]+.[0-9]+
v0.[0-9]+.[0-9]+-beta.[0-9]+
v0.[0-9]+.[0-9]+-alpha.[0-9]+
```

Before creating a tag, confirm the tag exactly matches the `Cargo.toml` version with a leading `v` and does not already exist locally or remotely.

## Git Commit And Tag

After the build, package check, and any requested validation pass:

1. Commit only `Cargo.toml` and `Cargo.lock` with `chore: release vX.Y.Z`.
2. Push the commit: `git push origin HEAD`.
3. Wait for CI on that exact pushed commit to complete successfully.
4. Create an annotated tag: `git tag -a vX.Y.Z -m "vX.Y.Z"`.
5. Push the tag: `git push origin vX.Y.Z`.
6. Check the GitHub Actions run for the tag through completion, because that run publishes to crates.io.
7. Confirm the new version appears on crates.io with `cargo search ferricket --limit 1`. Allow for registry indexing delay before treating absence as failure.

Do not create or push the release tag until the release commit CI is green. Do not tell the user to publish manually unless the GitHub Actions release path is intentionally unavailable.

## Reporting

Tell the user:

- Previous version and new version.
- Whether upstream CI for the previous commit was green.
- Build, package, and validation commands run and whether they passed.
- That publishing was performed by GitHub Actions after the tag push.
- Commit hash, tag name, tag-workflow result, and crates.io visibility.
- Any skipped steps and why.
