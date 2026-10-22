import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { gzipSync } from 'node:zlib';
import { spawnSync } from 'node:child_process';
import { root, bindgenPath, bindgenVersion, run } from './lib.mjs';

const release = process.argv.includes('--release');
const bindgen = bindgenPath();
const check = spawnSync(bindgen, ['--version'], { encoding: 'utf8' });
if (check.status !== 0 || check.stdout.trim() !== 'wasm-bindgen ' + bindgenVersion) {
  throw new Error('Matching wasm-bindgen CLI is missing. Run pnpm setup:wasm.');
}
await run('cargo', [
  'build',
  '-p',
  'fotoeditor-wasm',
  '--target',
  'wasm32-unknown-unknown',
  ...(release ? ['--release'] : []),
]);
const output = join(root, 'apps/web/src/engine/pkg');
await mkdir(output, { recursive: true });
await run(bindgen, [
  join(
    root,
    'target/wasm32-unknown-unknown',
    release ? 'release' : 'debug',
    'fotoeditor_wasm.wasm',
  ),
  '--target',
  'web',
  '--out-dir',
  output,
  '--out-name',
  'fotoeditor_wasm',
]);
if (release) {
  const { default: binaryen } = await import('binaryen');
  const path = join(output, 'fotoeditor_wasm_bg.wasm');
  const module = binaryen.readBinary(await readFile(path));
  module.setFeatures(binaryen.Features.All);
  binaryen.setOptimizeLevel(2);
  binaryen.setShrinkLevel(2);
  module.optimize();
  if (!module.validate()) throw new Error('Optimized WASM failed validation');
  const bytes = module.emitBinary();
  module.dispose();
  await writeFile(path, bytes);
  const gzippedBytes = gzipSync(bytes, { level: 9 }).length;
  const report = { wasmBytes: bytes.length, gzippedBytes, limitBytes: 1500000 };
  await mkdir(join(root, 'bench/results'), { recursive: true });
  await writeFile(
    join(root, 'bench/results/wasm-size.json'),
    JSON.stringify(report, null, 2) + '\n',
  );
  console.log(
    'Release WASM: ' + bytes.length + ' bytes; gzip: ' + gzippedBytes + ' / 1500000 bytes',
  );
  if (gzippedBytes > report.limitBytes) throw new Error('P0 WASM gzip size exceeds 1.5 MB');
}
