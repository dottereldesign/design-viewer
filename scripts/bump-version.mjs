import { readFileSync, writeFileSync } from 'node:fs';
const part = process.argv[2] ?? 'patch';
if (!['major', 'minor', 'patch'].includes(part)) throw Error('Use major, minor or patch');
const info = JSON.parse(readFileSync('package.json', 'utf8'));
const numbers = info.version.split('.').map(Number);
const index = ['major', 'minor', 'patch'].indexOf(part);
numbers[index]++; for (let n = index + 1; n < 3; n++) numbers[n] = 0;
const version = numbers.join('.');
for (const path of ['package.json', 'package-lock.json', 'src-tauri/tauri.conf.json']) {
  const data = JSON.parse(readFileSync(path, 'utf8')); data.version = version;
  if (data.packages?.['']) data.packages[''].version = version;
  writeFileSync(path, JSON.stringify(data, null, 2) + '\n');
}
for (const path of ['src-tauri/Cargo.toml', 'src-tauri/Cargo.lock']) {
  let text = readFileSync(path, 'utf8');
  text = path.endsWith('.toml') ? text.replace(/^version = "[^"]+"/m, `version = "${version}"`) : text.replace(/(name = "design-viewer"\r?\n)version = "[^"]+"/, `$1version = "${version}"`);
  writeFileSync(path, text);
}
console.log(`Version bumped to ${version}`);
