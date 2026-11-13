# Fork notes

Changes in this fork that upstream Handy does not have. Keep them small so
`git rebase upstream/main` stays easy.

## Cloud transcription (OpenRouter)

All the logic lives in files upstream never touches:

- `src-tauri/src/cloud_stt/` – client, settings store (`cloud_stt.json`), commands
- `src/components/cloud-stt/CloudTranscription.tsx` – settings panel and its English strings
- `src-tauri/tests/fixtures/cloud_stt/` – responses recorded from the live API

Touch points in upstream files. After a rebase conflict, re-apply these and nothing more:

| File                                                | Change                                                                                                                                                                                                         |
| --------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `src-tauri/Cargo.toml` (+ lock)                     | `base64 = "0.22"`                                                                                                                                                                                              |
| `src-tauri/src/lib.rs`                              | `mod cloud_stt;` and `cloud_stt::chain(...)` around `specta_builder.invoke_handler()`                                                                                                                          |
| `src-tauri/src/actions.rs`                          | `let cloud = cloud_stt::is_active(app)` in `start()` bypasses the local-model gate, model load, streaming and live `start_stream`; `stop()` calls `cloud_stt::transcribe` instead of `tm.transcribe_recording` |
| `src/components/settings/models/ModelsSettings.tsx` | import and render `<CloudTranscription />`                                                                                                                                                                     |

Not covered in cloud mode: live streaming text, custom words, filler-word removal
and Chinese-script conversion (these run inside the local transcription manager).
The `--transcribe-file` CLI stays local.

Live smoke test: `OPENROUTER_API_KEY=... cargo test --lib live_ -- --ignored`.

## Updates and releases

The app updates from this fork's GitHub Releases, signed with the fork's own key.

| File                                 | Change                                                                                             |
| ------------------------------------ | -------------------------------------------------------------------------------------------------- |
| `src-tauri/tauri.conf.json`          | `plugins.updater.pubkey` is the fork's public key; `endpoints` points at `0x464F58/Handy` releases |
| `.github/workflows/fork-release.yml` | New. Builds the Linux `.deb` only                                                                  |

- **Key:** private half at `~/.tauri/handy-fork.key` (back it up; losing it strands installed copies). It lives in repo secrets `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`.
- **Release:** run "Fork Release" (Actions tab, or `gh workflow run fork-release.yml -f version=0.10.0-fork.1`), check the draft, then publish it. Releases must not be marked prerelease, or `latest` skips them.
- **Versions:** use `<upstream version>-fork.N`. The workflow patches the version on a tagged commit, so no version bumps are committed. Each release must be higher than the one installed; after rebasing onto a newer upstream, restart at `-fork.1` of that version.
- **Installing the update:** the app runs `dpkg -i` via `pkexec`. If a new dependency was added, run `sudo apt --fix-broken install`.
