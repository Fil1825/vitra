import fs from 'node:fs';

const { version } = JSON.parse(fs.readFileSync('package.json', 'utf8'));
const tauri = JSON.parse(fs.readFileSync('src-tauri/tauri.conf.json', 'utf8')).version;
const cargo = fs.readFileSync('Cargo.toml', 'utf8').match(/^version\s*=\s*"([^"]+)"/m)?.[1];
if (version !== tauri || version !== cargo)
  throw new Error('Keep package.json, Tauri, and Cargo versions in sync');
const tag = process.env.GITHUB_REF;
if (tag?.startsWith('refs/tags/') && tag !== `refs/tags/v${version}`)
  throw new Error('Release tag must match the application version');
console.log(`Vitra ${version}`);
