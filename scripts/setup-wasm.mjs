import { mkdir, writeFile, rm } from 'node:fs/promises';
import { join } from 'node:path';
import { root, toolName, bindgenPath, bindgenVersion, run } from './lib.mjs';
import { spawnSync } from 'node:child_process';

await run('rustup', ['target', 'add', 'wasm32-unknown-unknown']);
const check = spawnSync(bindgenPath(), ['--version'], { encoding: 'utf8' });
if (check.status === 0 && check.stdout.trim() === 'wasm-bindgen ' + bindgenVersion) {
  console.log(check.stdout.trim() + ' is ready.');
} else {
  const tools = join(root, '.tools');
  await mkdir(tools, { recursive: true });
  const archive = join(tools, toolName + '.tar.gz');
  const url =
    'https://github.com/wasm-bindgen/wasm-bindgen/releases/download/' +
    bindgenVersion +
    '/' +
    toolName +
    '.tar.gz';
  const response = await fetch(url);
  if (!response.ok) throw new Error('Tool download failed: ' + response.status + ' ' + url);
  await writeFile(archive, Buffer.from(await response.arrayBuffer()));
  await run('tar', ['-xzf', archive, '-C', tools]);
  await rm(archive);
  await run(bindgenPath(), ['--version']);
}
