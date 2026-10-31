# Design

A personal Windows library for images, videos and inspiration links.

[Download Windows setup](https://github.com/dottereldesign/design-viewer/releases/latest/download/Design-Setup.exe)

1. Download **Design-Setup.exe** and run it.
2. Open **Design** from the Start menu.
3. Go to **Settings → Change Design folder** and choose your media folder.

Windows 10/11, 64-bit. Sign into GitHub to download from this private repo. Setup includes the video tools and installs WebView2 if needed.

This repo backs up the app only. Your media, saved links, projects and tags are stored separately.

To restore your organisation data, use **Settings → Import metadata** with a previously exported backup.

## Build from source

Requires Node.js 20+, Rust and the Microsoft C++ Build Tools with a Windows SDK.

```powershell
npm ci
npm run desktop
```

Create an installer: `npm run package`.

[Development notes](docs/DEVELOPMENT.md) · [Third-party notices](THIRD_PARTY_NOTICES.md)
