# Development workflow

The owner authorizes this routine for each requested app change. Complete it automatically; do not ask again to push a feature branch, open its pull request, merge passing work or publish its installer.

1. Start from an up-to-date `main`. Create one short-lived `feat/<topic>` or `fix/<topic>` branch. Never include unrelated user changes.
2. Implement the request and appropriate tests. Push meaningful progress to the feature branch and open a draft pull request if work continues across sessions.
3. For app changes, bump all versions with `npm run version:bump -- patch` (use `minor` for a substantial feature). Do this once per feature, not once per commit.
4. Run relevant local checks, `npm run check:version` and `npm run scan:secrets` after staging. Keep media, app data, credentials, installers, caches and local build tools out of Git. Never print secrets.
5. Push the feature branch and open/ready a concise pull request. Wait for `Check app` and `Check native` to pass. Fix failures; never bypass the checks or require the sole owner to approve their own work.
6. Squash merge the completed PR into `main`, delete the merged remote branch and update local `main` with a fast-forward pull. Do not force-push `main` or rewrite published tags.
7. For app changes, run `npm run release:tag` from clean `main`. The tag builds and publishes the Windows installer automatically. Wait for that workflow and verify its version, commit and SHA-256 manifest. Do not claim an update is released while it is still building.
8. Build/deploy the same version locally using `npm run package`. This refreshes `app/Design.exe`, `app/Design-Setup.exe` and `app/build.json`; verify the recorded commit and executable hash. A running process still needs to restart to load new code. Do not automate the user's desktop or silently launch the app under Codex's redirected app-data context. Report the version that is actually running or clearly explain that reopening is needed.

Use the existing GitHub authentication. Never commit tokens or add a personal access token to workflows; releases use the short-lived `GITHUB_TOKEN`. Pin third-party Actions to commit SHAs. Do not use `pull_request_target` to execute untrusted PR code.

The repository is public and MIT-licensed; personal media and library data remain private. See [the solo workflow](docs/WORKFLOW.md).
