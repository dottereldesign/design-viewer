# Solo workflow

`main` → feature branch → pull request → checks → squash merge → version tag → Windows release.

One branch per change. Commit and push progress there. When the feature works and checks pass, merge it and delete the branch. No `develop` branch, approval queue or manual copying between branches.

For app changes, bump the version once with `npm run version:bump -- patch` (or `minor`). After merging, run `npm run release:tag` on clean `main`. GitHub builds the installer and publishes it with a checksum and source-commit manifest. Existing releases stay available for rollback.

`main` requires the two automated checks and a pull request, with zero required reviewer approvals. Agent instructions in `AGENTS.md` make pushing, merging and releasing part of completing a task.

## Avoid stale app builds

- `npm run package` refreshes both the local executable and installer, and writes `app/build.json` with the version, Git commit and executable checksum.
- Package, Rust and Tauri versions must match; CI rejects mismatches.
- Settings shows the running app's version and links to the latest installer.
- Close the old process and reopen the updated app to load a new executable. Replacing the file alone does not update a running process.
- Prefer the existing normal Windows shortcut when reopening. Codex-launched processes can use redirected app-data folders and show a different library.

## Keep personal data out

Media folders, SQLite databases, backups, `.env` files, installers and local tools are ignored. Gitleaks scans Git history and staged changes. GitHub secret scanning and push protection provide another check; none can guarantee detection of every possible secret.

Export full libraries to a location outside the repo. Exported media and links are personal data and must never be committed.

Based on [GitHub Flow](https://docs.github.com/en/get-started/using-github/github-flow), [protected branches](https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/managing-protected-branches/about-protected-branches) and [GitHub secret protection](https://docs.github.com/en/code-security/getting-started/quickstart-for-securing-your-repository).
