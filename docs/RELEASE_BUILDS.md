# GitHub-hosted release builds

`Release builds` (`.github/workflows/release.yml`) compiles Places on GitHub's
Mac arm64 (`macos-15`) and Windows x64 (`windows-2022`) runners. Local application
compilation and ZIP uploading are no longer required for this path. The existing
`Rust verification` workflow remains independent.

## Build a candidate

In Actions, select **Release builds → Run workflow**, select the desired branch
or tag, and leave **upload_to_draft** unchecked. Both jobs run formatting, strict
Clippy, CPU tests, the locked all-feature workspace release build, production
Luau refusal checks, packaging and command-line resource discovery. Download
`release-macOS-arm64` and `release-Windows` from the completed run (14-day retention).

Changes to this workflow, its `tools/release/` helpers or its focused tests also
build candidates on pushes to `0.14.0-dev`/`main` and on pull requests. These
automatic runs cannot upload to a release. Ordinary Rust CI does not bake maps.

GitHub requires a manually dispatched workflow to exist on the default branch.
Until this workflow reaches `main` through the normal reviewed integration,
the branch push checks exercise it; manual dispatch may not be available. Adding
this workflow does not itself merge a branch, create a tag or create a release.

## Attach applications to a release

1. Complete the normal release review and ensure Cargo.toml, Cargo.lock, the first
   versioned changelog entry and `tools/release-projects.json` agree.
2. Create the intended `vX.Y.Z` tag at the reviewed commit and an associated
   **draft** GitHub release through the normal release process.
3. Run **Release builds** on that exact tag with **upload_to_draft** checked.
   Equivalent CLI: `gh workflow run release.yml --ref vX.Y.Z -f upload_to_draft=true`.
4. Inspect the successful jobs, download/test the applications, add the separately
   validated maps/guide and complete the release manifest/notes. Publish the draft
   only after that review. This workflow leaves it unpublished.

For version X.Y.Z the release receives:

- `Places-X.Y.Z-macOS-arm64.zip`: `Places/` portable layout and `Places.app/`,
  both including player, editor, compiler, SDL, runtime assets and notices.
- `Places-X.Y.Z-Windows.zip`: `Places/` with all three EXEs, SDL, DXC and VC
  runtime DLLs, runtime assets, licenses and the existing payload receipt.
- `HOSTED_BUILD_MANIFEST.json`: source commit, run ID, Rust/runner identities,
  menu and archive hashes, signing status and explicit test limitations.
- `HOSTED_SHA256SUMS.txt`: application archive checksums.

Only the upload job has `contents: write`; build jobs have read-only repository
access. Preflight checks the version and exact tag; draft discovery is deferred
to the upload job because draft visibility requires push access. It uses the
paginated release list, not the published-release tag lookup. A missing or
inaccessible draft stops upload after the builds complete.
Checkout credentials are not persisted. The upload job accepts only
both platform outputs from its own run and commit. It rechecks the tag and draft
before uploading and never uses `--clobber`. Existing identical assets can be
skipped after an interrupted upload; differing namesakes stop the run before
new uploads. A rebuilt run can have different ZIP bytes: inspect any partial
draft assets manually before retrying. Existing levels, guide, release notes,
`RELEASE_MANIFEST.json` and `SHA256SUMS.txt` are not replaced. A failed or partially
uploaded draft stays unpublished; uploads are not an atomic transaction.

## Maps, signing and acceptance

Applications contain no gameplay maps. The small bundled menu is rebuilt on
each platform and checked against current source; a stale local menu is never
silently reused. The existing map inventory and `tools/package-levels.py` still
govern the separate Levels ZIP. Gameplay-map baking, source-currency checks and
visual acceptance remain an explicit release task; this first workflow does not
claim to automate them or regenerate the illustrated guide.

Mac packaging relocates SDL and applies the existing ad-hoc signatures. It does
not notarize or use a Developer ID. The actual minimum OS is derived from the
executables and SDL into the bundle and `MACOS_RUNTIME.json`; a different runner
can yield a different minimum from an earlier locally built release. Windows
packages are unsigned, with hash-pinned official SDL/DXC downloads and VC runtime
files/notices from the hosted Visual Studio installation. No signing secrets or
personal access token are required. Runner/Homebrew/VC updates can change binary
bytes; this is source-identified packaging, not a bit-reproducibility claim.

Hosted command-line checks do not establish graphics, audio, controller behavior,
clean-machine Gatekeeper/SmartScreen trust or native gameplay acceptance. Keep
those checks separate. Linux and Intel Mac distributions are not added.

References: [GitHub manual triggers](https://docs.github.com/en/actions/reference/workflows-and-actions/events-that-trigger-workflows#workflow_dispatch),
[hosted runner images](https://github.com/actions/runner-images),
[draft release guidance](https://docs.github.com/en/repositories/releasing-projects-on-github/managing-releases-in-a-repository).
