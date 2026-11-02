# Development workflow

Prioritize fast local iteration and GitHub backups. A normal task is complete when the requested change works locally and its source is committed and pushed. Publishing an installer is a separate task, only when the user explicitly asks for a release.

## Default: local change and GitHub backup

1. Inspect the working tree and preserve unrelated user changes. Use a short-lived `feat/<topic>`, `fix/<topic>` or `docs/<topic>` branch from current main when starting new work; continue an existing unfinished branch when appropriate. Never discard work or block a small edit on unrelated branch housekeeping.
2. Implement the request. Scale verification to risk: for a small image, CSS, spacing or text tweak, use the relevant build or visual check. Do not run the entire native/browser test suite for a cosmetic change. Run broader tests for behavior, database or native changes.
3. Make the change available locally before waiting on GitHub CI. An existing development session can refresh frontend changes immediately. For the normal installed/portable desktop app, use `npm run package -- --no-bundle` to refresh `app/Design.exe` and `app/build.json` without building an installer. Native compilation may still take time; explain that when relevant. Do not run a second full package build just for a cosmetic change.
4. Commit only the requested changes and push promptly using existing GitHub authentication. Run `npm run scan:secrets` after staging. Keep media, library data, credentials, installers, caches and local tools out of Git. Never print secrets. No additional permission is needed for routine commits and pushes.
5. Open a concise PR when useful to integrate the branch. GitHub checks may run asynchronously: do not make the user wait for CI, a merge, or a release before reporting a verified local update and successful push. A pushed branch is a valid backup. Report any pending integration honestly. Merge only after required checks pass; never bypass branch protections or require the sole owner to approve their own work. Do not force-push main.
6. Do not bump versions, create release tags, publish GitHub releases, or build installers for routine local changes. Multiple local changes can share the current version. Use the Git commit and build metadata to identify local builds.

## Explicit releases only

When the user asks to publish a release, collect the intended changes, merge passing PRs, update clean local main, bump all versions once with `npm run version:bump -- patch` (or minor for a substantial release), and commit/push through the normal PR checks. Run `npm run check:version` and `npm run scan:secrets`. From clean, synchronized main, run `npm run release:tag`. Wait for the Windows release workflow, then verify its version, commit and SHA-256 manifest before claiming publication. Run `npm run package` if a matching local installer is needed. Never rewrite published tags.

## Local app and data

Verify `app/build.json` records the intended commit and matches the executable hash after a local desktop build. A running process needs to restart to load a replaced executable; clearly tell the user to reopen from their usual shortcut. Do not silently launch the app under Codex's redirected app-data context or automate the user's desktop. Development hot reload is the fastest route when an established dev session is available; do not confuse its data with the normal app's library.

The repository is public and MIT-licensed; personal media and library data remain private. Never commit tokens or add a personal access token to workflows; releases use the short-lived `GITHUB_TOKEN`. Pin third-party Actions to commit SHAs. Do not use `pull_request_target` to execute untrusted PR code. See [the solo workflow](docs/WORKFLOW.md).
