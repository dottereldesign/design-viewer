import ffmpeg from "ffmpeg-static";
import { mkdir, copyFile } from "node:fs/promises";
await mkdir("src-tauri/binaries", { recursive: true });
await copyFile(ffmpeg, "src-tauri/binaries/ffmpeg.exe");
await copyFile(`${ffmpeg}.LICENSE`, "src-tauri/binaries/ffmpeg-license.txt");
await copyFile(`${ffmpeg}.README`, "src-tauri/binaries/ffmpeg-readme.txt");
console.log("FFmpeg and notices ready.");
