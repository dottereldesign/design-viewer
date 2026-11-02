# Solo workflow

Default: edit -> check locally -> refresh local app -> commit and push a feature branch.

Local use and GitHub backup are the priority. A pushed branch backs up the work; CI and PR integration must not delay delivery of a working local change. Open a PR when useful, and merge only after required checks pass. Main remains protected. Do not bypass checks or force-push main.

Small cosmetic changes need proportionate verification, not the whole native and browser test suite. Do not bump versions, build installers or publish releases for routine tweaks. Publish only when the user explicitly requests it; batch accumulated changes into that release.

## See local changes

- An existing development session hot reloads frontend edits, providing the fastest visual feedback.
- For the normal desktop app, `npm run package -- --no-bundle` refreshes `app/Design.exe` and `app/build.json` without creating an installer. Native compilation can still take time.
- Verify the build metadata's commit and executable checksum. Local builds may share the current release version; their commits distinguish them.
- Close and reopen the app from the normal Windows shortcut to load a replaced executable. Do not silently launch a separate Codex-context app: redirected app-data folders can show a different library.
- Documentation-only changes do not require rebuilding the app.

## When a release is requested

Integrate intended changes through passing PRs, bump matching package/Rust/Tauri versions once, run version and secret checks, and release from clean main with `npm run release:tag`. Wait for publication and verify the installer manifest and SHA-256. Use `npm run package` only when a full local installer is needed.

## Keep personal data out

Media, library databases, backups, credentials, installers and local tools stay out of Git. Run `npm run scan:secrets` after staging. Export personal libraries outside the repository. Use existing GitHub authentication; never commit tokens.
