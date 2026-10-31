import { execFileSync, spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import { resolve } from 'node:path';

const version = '8.30.1';
const platform = process.platform === 'win32' ? 'windows' : process.platform === 'linux' ? 'linux' : 'darwin';
const architecture = process.arch === 'arm64' ? 'arm64' : 'x64';
const extension = platform === 'windows' ? 'zip' : 'tar.gz';
const name = `gitleaks_${version}_${platform}_${architecture}.${extension}`;
const directory = resolve('.tools', `gitleaks-${version}`);
const executable = resolve(directory, platform === 'windows' ? 'gitleaks.exe' : 'gitleaks');
async function download(url) {
  const response = await fetch(url);
  if (!response.ok) throw Error(`Gitleaks download failed: ${response.status}`);
  return Buffer.from(await response.arrayBuffer());
}
if (!existsSync(executable)) {
  await mkdir(directory, { recursive: true });
  const base = `https://github.com/gitleaks/gitleaks/releases/download/v${version}`;
  const checksumText = (await download(`${base}/gitleaks_${version}_checksums.txt`)).toString();
  const expected = checksumText.split(/\r?\n/).find(line => line.trim().endsWith(name))?.split(/\s+/)[0];
  const archive = await download(`${base}/${name}`);
  if (!expected || createHash('sha256').update(archive).digest('hex') !== expected) throw Error('Gitleaks checksum mismatch');
  const path = resolve(directory, name); await writeFile(path, archive);
  execFileSync('tar', ['-xf', path, '-C', directory], { stdio: 'inherit', windowsHide: true });
}
for (const args of [['git', '.', '--log-opts=--all', '--redact', '--no-banner'], ['stdin', '--redact', '--no-banner']]) {
  const input = args[0] === 'stdin' ? Buffer.concat([
    execFileSync('git', ['diff', '--cached', '--no-ext-diff', '--no-textconv'], { maxBuffer: 20 * 1024 * 1024 }),
    execFileSync('git', ['diff', '--no-ext-diff', '--no-textconv'], { maxBuffer: 20 * 1024 * 1024 }),
  ]) : undefined;
  const result = spawnSync(executable, args, { input, stdio: input ? ['pipe', 'inherit', 'inherit'] : 'inherit', windowsHide: true });
  if (result.error) throw result.error;
  if (result.status !== 0) process.exit(result.status ?? 1);
}
console.log('Secret scan passed: complete Git history and staged/working diffs.');
