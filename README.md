# Design

A personal Windows library for images, videos and inspiration links.

[Download Windows setup](https://github.com/dottereldesign/design-viewer/releases/latest/download/Design-Setup.exe)

1. Download **Design-Setup.exe** and run it.
2. Open **Design** from the Start menu.
3. Go to **Settings → Change Design folder** and choose your media folder.

Windows 10/11, 64-bit. Setup includes the video tools and installs WebView2 if needed.

This repo backs up the app only. Your media, saved links, projects and tags are stored separately.

Back up everything: **Settings → Export full library**. Choose a folder on your PC or an external drive; progress and any missing files are shown.

To restore, copy the whole backup folder to its permanent location, then use **Settings → Import metadata** and choose its `library.json`.

Updates: check your version in **Settings**, then use **Get latest update**. Close Design before installing the new setup.

## Build from source

Requires Node.js 20+, Rust and the Microsoft C++ Build Tools with a Windows SDK.

```powershell
npm ci
npm run desktop
```

Create an installer: `npm run package`.

[Solo GitHub workflow](docs/WORKFLOW.md) · [Development notes](docs/DEVELOPMENT.md) · [Third-party notices](THIRD_PARTY_NOTICES.md)
