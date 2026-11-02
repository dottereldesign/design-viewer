import { existsSync, readdirSync, realpathSync, readFileSync, writeFileSync, mkdirSync } from "node:fs";
import { join, resolve } from "node:path";
import { spawn, execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdir, copyFile, rename } from "node:fs/promises";

// Use the optional portable compiler for this process only. An installed Visual
// Studio toolchain remains supported by Cargo when this folder is absent.
const env = { ...process.env };
let localBuild = join(
  process.env.LOCALAPPDATA || resolve(".tools"),
  "DesignViewerBuild",
);
const buildPathRecord = resolve('.tools/build-path.json');
if (!existsSync(join(localBuild,'msvc')) && existsSync(buildPathRecord)) {
  const recorded = JSON.parse(readFileSync(buildPathRecord,'utf8')).path;
  if (typeof recorded === 'string' && existsSync(join(recorded,'msvc'))) localBuild = recorded;
}
if (existsSync(join(localBuild,'msvc'))) {
  mkdirSync('.tools',{recursive:true});
  writeFileSync(buildPathRecord,JSON.stringify({path:realpathSync.native(localBuild)},null,2));
}
const tools = existsSync(resolve(".tools/msvc"))
  ? resolve(".tools/msvc")
  : join(localBuild, "msvc");
env.CARGO_TARGET_DIR ||= join(localBuild, "target");
if (existsSync(join(tools, "VC/Tools/MSVC"))) {
  const vcVersion = readdirSync(join(tools, "VC/Tools/MSVC")).sort().at(-1);
  const sdkVersion = readdirSync(join(tools, "Windows Kits/10/Lib"))
    .sort()
    .at(-1);
  const vc = join(tools, "VC/Tools/MSVC", vcVersion);
  const sdk = join(tools, "Windows Kits/10");
  const bin = join(vc, "bin/Hostx64/x64");
  env.PATH = [
    bin,
    join(sdk, "bin", sdkVersion, "x64"),
    env.PATH || env.Path,
  ].join(";");
  delete env.Path;
  env.INCLUDE = [
    join(vc, "include"),
    ...["ucrt", "shared", "um", "winrt", "cppwinrt"].map((p) =>
      join(sdk, "Include", sdkVersion, p),
    ),
  ].join(";");
  env.LIB = [
    join(vc, "lib/x64"),
    ...["ucrt", "um"].map((p) => join(sdk, "Lib", sdkVersion, p, "x64")),
  ].join(";");
  env.VCToolsInstallDir = vc + "\\";
  env.VCToolsVersion = vcVersion;
  env.WindowsSdkDir = sdk + "\\";
  env.WindowsSDKVersion = sdkVersion + "\\";
  env.WindowsSdkBinPath = join(sdk, "bin") + "\\";
  env.CC = join(bin, "cl.exe");
  env.AR = join(bin, "lib.exe");
}
const action = process.argv[2] || "dev";
const nativeTest = action === "test";
const args = nativeTest
  ? [
      "test",
      "--manifest-path",
      "src-tauri/Cargo.toml",
      ...process.argv.slice(3),
    ]
  : [
      resolve("node_modules/@tauri-apps/cli/tauri.js"),
      action,
      ...process.argv.slice(3),
    ];
if (!existsSync("src-tauri/binaries/ffmpeg.exe") || !existsSync("src-tauri/binaries/ffmpeg-license.txt") || !existsSync("src-tauri/binaries/ffmpeg-readme.txt"))
  await import("./prepare-media.mjs");
if (!existsSync("src-tauri/binaries/yt-dlp.exe")) await import("./prepare-downloader.mjs");
const child = spawn(nativeTest ? "cargo.exe" : process.execPath, args, {
  stdio: "inherit",
  env,
});
child.on("error", (e) => {
  console.error(e.message);
  process.exitCode = 1;
});
child.on("exit", async (code) => {
  process.exitCode = code ?? 1;
  if (code === 0 && action === "build") {
    try {
      await mkdir("app/binaries", { recursive: true });
      const binary = join(env.CARGO_TARGET_DIR, process.argv.includes("--debug") ? "debug" : "release", "design-viewer.exe");
      try { await copyFile(binary, "app/Design.exe"); }
      catch (error) {
        if (!["EBUSY", "EPERM", "EACCES"].includes(error.code) || !existsSync("app/Design.exe")) throw error;
        // Windows can rename a running executable; the process keeps its old
        // image while the usual shortcut starts the new file next time.
        const previous = `app/Design.previous-${Date.now()}.exe`;
        await rename("app/Design.exe", previous);
        try { await copyFile(binary, "app/Design.exe"); }
        catch (copyError) { await rename(previous, "app/Design.exe"); throw copyError; }
      }
      const version = JSON.parse(readFileSync("package.json", "utf8")).version;
      const sha256 = createHash("sha256").update(readFileSync("app/Design.exe")).digest("hex");
      let commit = "uncommitted";
      try { commit = execFileSync("git", ["rev-parse", "HEAD"], { encoding: "utf8", windowsHide: true }).trim(); } catch {}
      const dirty = execFileSync("git", ["status", "--porcelain"], { encoding: "utf8", windowsHide: true }).trim().length > 0;
      writeFileSync("app/build.json", JSON.stringify({ version, commit, dirty, sha256, builtAt: new Date().toISOString() }, null, 2) + "\n");
      if (!process.argv.includes("--no-bundle") && !process.argv.includes("--debug")) {
        const productName = JSON.parse(readFileSync("src-tauri/tauri.conf.json", "utf8")).productName;
        const installer = join(env.CARGO_TARGET_DIR, "release", "bundle", "nsis", `${productName}_${version}_x64-setup.exe`);
        if (!existsSync(installer)) throw Error("Windows installer was not created");
        await copyFile(installer, "app/Design-Setup.exe");
      }
      await copyFile(
        "src-tauri/binaries/ffmpeg.exe",
        "app/binaries/ffmpeg.exe",
      );
      await copyFile("src-tauri/binaries/ffmpeg-license.txt", "app/binaries/ffmpeg-license.txt");
      await copyFile("src-tauri/binaries/ffmpeg-readme.txt", "app/binaries/ffmpeg-readme.txt");
      await copyFile("src-tauri/binaries/yt-dlp.exe", "app/binaries/yt-dlp.exe");
      await copyFile("src-tauri/binaries/yt-dlp-licenses.txt", "app/binaries/yt-dlp-licenses.txt");
      console.log("Portable app ready: app/Design.exe");
    } catch (e) {
      console.error(e);
      process.exitCode = 1;
    }
  }
});
