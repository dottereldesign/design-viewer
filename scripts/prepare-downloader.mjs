import { mkdir, writeFile, rename } from 'node:fs/promises';
import { createHash } from 'node:crypto';
const version = '2026.10.26';
const base = `https://github.com/yt-dlp/yt-dlp/releases/download/${version}`;
async function get(url) {
  const response = await fetch(url);
  if (!response.ok) throw Error(`Download failed (${response.status}): ${url}`);
  return Buffer.from(await response.arrayBuffer());
}
const checksums = (await get(`${base}/SHA2-256SUMS`)).toString();
const expected = checksums.split(/\r?\n/).find(line => /\s\*?yt-dlp\.exe$/.test(line))?.split(/\s/)[0];
if (!expected) throw Error('yt-dlp checksum missing');
const binary = await get(`${base}/yt-dlp.exe`);
if (createHash('sha256').update(binary).digest('hex') !== expected) throw Error('yt-dlp checksum mismatch');
await mkdir('src-tauri/binaries', { recursive: true });
await writeFile('src-tauri/binaries/yt-dlp.exe.pending', binary);
await rename('src-tauri/binaries/yt-dlp.exe.pending', 'src-tauri/binaries/yt-dlp.exe');
await writeFile('src-tauri/binaries/yt-dlp-licenses.txt', await get(`https://raw.githubusercontent.com/yt-dlp/yt-dlp/${version}/THIRD_PARTY_LICENSES.txt`));
console.log(`yt-dlp ${version} ready; SHA-256 verified.`);
