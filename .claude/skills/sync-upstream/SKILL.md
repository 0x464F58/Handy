---
name: sync-upstream
description: Merge new upstream Handy changes (cjpais/Handy) into this fork's `fork` branch, resolve conflicts, verify the fork's touch points still work, and report. Use when upstream publishes a release or the user asks to sync, update or catch up with upstream.
---

# Sync the fork with upstream

The fork's own code is isolated (see `FORK.md`, the source of truth for the
touch points). Syncing means: merge upstream, keep every fork touch point
working, prove it with the tests, and stop before anything is pushed.

Rules that apply throughout: plain branches, no worktrees; never push, merge
into `fork`, or dispatch a release without the user's go-ahead; never print
secrets or read `.env` / `~/.tauri/*.key`. Use `$CLAUDE_JOB_DIR/tmp` for temp
files when it is set.

## 1. Preconditions

- `git status --short` must be clean, and the branch must be `fork`. If not,
  stop and say what is dirty.
- `git remote get-url upstream` must be `https://github.com/cjpais/Handy.git`;
  add it if missing.
- `git fetch upstream --tags` and `git fetch origin`.
- `fork` must match `origin/fork` (`git rev-list --count fork..origin/fork` is 0).

## 2. See what is coming

- `git log --oneline fork..upstream/main` (nothing new: report that and stop).
- Latest upstream release: `gh release list -R cjpais/Handy -L 3`. Note whether
  the new commits include a release or are only commits on `main`; the user
  mostly cares about releases.
- Upstream version after the merge: `git show upstream/main:src-tauri/tauri.conf.json | grep '"version"'`.
- Read the upstream diff of the files the fork touches before merging, so
  semantic breaks are expected, not discovered:
  `git diff fork...upstream/main -- src-tauri/src/actions.rs src-tauri/src/lib.rs src/components/settings/models/ModelsSettings.tsx src-tauri/tauri.conf.json .github/workflows/build.yml`

## 3. Merge on a sync branch

```bash
git checkout -b sync/upstream-<version-or-YYYY-MM-DD> fork
git merge --no-ff upstream/main -m "chore: merge upstream <version or sha>"
```

Merge, not rebase: the history is not rewritten, no force-push is needed, and
conflicts are resolved once. Do not push the sync branch yet.

### Resolving conflicts

Resolve with the `FORK.md` table, taking upstream's side first and then
re-applying only the fork's hunk:

- `src-tauri/tauri.conf.json`: keep upstream's `version` and everything else,
  but **always keep the fork's `plugins.updater.pubkey` and `endpoints`**.
- `src-tauri/Cargo.toml`: keep upstream's changes plus `base64 = "0.22"`.
- `src-tauri/Cargo.lock`: do not hand-merge. Take upstream's, then run
  `cargo update -w` / `cargo check` and let it add `base64` back.
- `src-tauri/src/actions.rs`: re-apply the `cloud` gate in `start()` (local
  load, model-loaded check, `model_supports_streaming`, `start_stream`) and the
  cloud branch around `transcribe_recording` in `stop()`.
- `src-tauri/src/lib.rs`: `mod cloud_stt;` and `cloud_stt::chain(...)` around
  `specta_builder.invoke_handler()`.
- `src/components/settings/models/ModelsSettings.tsx`: the import and
  `<CloudTranscription />` block above `<ModelTable`.
- `src/bindings.ts` or other generated files: regenerate, do not hand-merge.
- Anything else in conflict is upstream's: take upstream's version.

If upstream moved or rewrote the code a hook sits in, re-create the hook in the
new location with the same behavior, keep it as small as before, and update
`FORK.md`.

## 4. Verify the touch points survived (a clean merge is not proof)

Each of these must hold; fix and re-check if not:

- `grep -n "cloud_stt" src-tauri/src/lib.rs src-tauri/src/actions.rs` shows the
  module declaration, `chain`, `is_active` in `start()`, `is_active` and
  `transcribe` in `stop()`.
- `grep -n "CloudTranscription" src/components/settings/models/ModelsSettings.tsx`.
- `tauri.conf.json` still has the fork `pubkey` (it must equal, ignoring the trailing newline, the contents of
  `~/.tauri/handy-fork.key.pub`; never read the private key) and the
  `0x464F58/Handy` endpoint, and `version` is upstream's.
- Assumptions `cloud_stt` makes about upstream APIs still compile and mean the
  same: `settings::get_settings`, `selected_language`, `spoken_languages`,
  `portable::store_path`, `tauri_plugin_store::StoreExt`, `Transcription {
  text, language }`, the specta `invoke_handler()` type.
- `.github/workflows/fork-release.yml` still matches `build.yml`: every input it
  passes (`platform`, `target`, `build-args`, `sign-binaries`, `asset-prefix`,
  `upload-artifacts`, `release-id`, `ref`) must still exist, with the same type.
- `README.md` still opens with the fork banner above `# Handy`, and `FORK.md`'s "Differences from upstream" is still accurate.
- A behavior change in the transcription path that cloud mode bypasses (new
  text post-processing inside `TranscriptionManager`, new gates in `start()`)
  is worth flagging to the user even when it compiles.

## 5. Build and test

`PKG_CONFIG_PATH` must include the system pkgconfig dirs, because Homebrew's
`pkg-config` shadows them:

```bash
export PKG_CONFIG_PATH=/usr/lib/x86_64-linux-gnu/pkgconfig:/usr/share/pkgconfig
cd src-tauri && cargo fmt --check && cargo test --lib && cargo clippy --lib && cd ..
bun install
npx tsc --noEmit -p . && npx eslint src && bun run check:translations
bun run format:check
```

All must pass. A failing fork test is a regression from the merge: write or
extend a test that captures it before fixing the code. A failing upstream test
that also fails on `upstream/main` alone is not ours; say so.

Optional, when upstream changed the audio or settings code the fork reads:
`OPENROUTER_API_KEY` is in `.env`; run the live smoke test with
`set -a; . ./.env; set +a; cargo test --lib live_ -- --ignored` without
printing the key.

## 6. Report, then wait

Give the user: upstream range merged (`<old>..<new>`), the upstream version,
conflicts and how each was resolved, any touch point that had to move, test
results, and anything that changes fork behavior. Update `FORK.md` if the
touch-point table changed, and commit that on the sync branch.

Then ask before doing the following, each only on a go-ahead:

1. Land it: `git checkout fork && git merge --ff-only sync/<name>`, then
   `git push origin fork`, and delete the sync branch.
2. Keep `main` an upstream mirror: `git push origin upstream/main:main`.
3. Release: suggest the version `<upstream version>-fork.1` (a new upstream
   version restarts at `.1`; a fork-only change bumps `N`), then
   `gh workflow run fork-release.yml -R 0x464F58/Handy --ref fork -f version=<v>`,
   follow the run, check `latest.json` has `linux-x86_64-deb`, and leave
   publishing the draft to the user.
