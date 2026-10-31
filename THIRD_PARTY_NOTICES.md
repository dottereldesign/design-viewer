# Third-party components

Design uses Tauri, React, Rust, SQLite, TanStack Virtual, Lucide and other open-source dependencies. Exact versions are recorded in `package-lock.json` and `src-tauri/Cargo.lock`.

## FFmpeg

The installer includes FFmpeg supplied through [ffmpeg-static](https://github.com/eugeneware/ffmpeg-static). Its licence and build/source information are installed as `binaries/ffmpeg-license.txt` and `binaries/ffmpeg-readme.txt`.

## yt-dlp

The installer includes yt-dlp 2026.10.26 from its [official release](https://github.com/yt-dlp/yt-dlp/releases/tag/2026.10.26), verified against the published SHA-256 checksum. Its third-party notices are installed as `binaries/yt-dlp-licenses.txt`.

## Microsoft WebView2

The installer downloads the Microsoft WebView2 runtime only when needed. This runtime is provided by Microsoft under its own terms.
