import { chromium, expect } from '@playwright/test';
import { spawn, execFileSync } from 'node:child_process';
import { mkdir, stat, writeFile, readdir } from 'node:fs/promises';
import { createReadStream } from 'node:fs';
import { createHash } from 'node:crypto';
import { resolve } from 'node:path';
import os from 'node:os';
import { setTimeout as delay } from 'node:timers/promises';
const base = 'http://127.0.0.1:4188',
  fixture = resolve('bench/results/fixtures/p8-layers-20-6000x4000.cgfoto');
const trialCount = Number(process.env.P8_SAVE_TRIALS ?? 4);
if (!Number.isInteger(trialCount) || trialCount < 1 || trialCount > 10)
  throw Error('P8_SAVE_TRIALS must be 1-10');
const digest = async (path) => {
  const h = createHash('sha256');
  for await (const bytes of createReadStream(path)) h.update(bytes);
  return h.digest('hex');
};
const report = {
  measuredAt: new Date().toISOString(),
  sourceRevision: execFileSync('git', ['rev-parse', 'HEAD'], {
    encoding: 'utf8',
    windowsHide: true,
  }).trim(),
  sourceDirty: Boolean(
    execFileSync('git', ['status', '--porcelain'], { encoding: 'utf8', windowsHide: true }).trim(),
  ),
  runtimeAssets: await Promise.all(
    (await readdir('apps/web/dist/assets'))
      .filter((file) => /\.(js|wasm)$/.test(file))
      .map(async (file) => ({ file, sha256: await digest(resolve('apps/web/dist/assets', file)) })),
  ),
  machine: { os: os.version(), cpu: os.cpus()[0].model, ram: os.totalmem() },
  dataset: {
    file: fixture,
    bytes: (await stat(fixture)).size,
    sha256: await digest(fixture),
    width: 6000,
    height: 4000,
    layers: 20,
    tiles: 7680,
    distinctDense: true,
    generator: 'fotoeditor-io/examples/layers.rs; seed 0x9e3779b9',
  },
  method:
    'Single worker; hardware only; sequential fresh browser/API profiles on D:; source legacy project opened once into cold raw OPFS backing and normal complete composite before saves. FSA-compatible actual OPFS FileSystemFileHandle writes. Save timer includes thumbnail, full composite PNG, every distinct LZ4 tile, ZIP directory, write, flush and writable.close. Excludes input open, app startup, output hashing/download verification. First save versus warm repeated saves; OS caches not cleared, no disk-cold claim. Logical CPU/GPU observed at 50 ms from normal snapshots, excluding allocator/browser/driver overhead.',
  targetMs: 3000,
  results: [],
};
const out = process.env.P8_SAVE_REPORT ?? 'bench/results/p8-save-performance.json';
const save = () => writeFile(out, JSON.stringify(report, null, 2) + '\n');
await save();
const server = spawn(
  process.execPath,
  [
    resolve('apps/web/node_modules/vite/bin/vite.js'),
    'preview',
    '--host',
    '127.0.0.1',
    '--port',
    '4188',
    '--strictPort',
  ],
  { cwd: resolve('apps/web'), stdio: 'ignore', windowsHide: true },
);
try {
  for (let i = 0; i < 100; i++) {
    try {
      if ((await fetch(base)).ok) break;
    } catch {
      /* Wait for the local preview server. */
    }
    await delay(100);
  }
  for (const channel of (process.env.P8_SAVE_CHANNELS ?? 'chrome,msedge').split(','))
    for (const backend of (process.env.P8_SAVE_BACKENDS ?? 'WebGPU,WebGL2').split(',')) {
      const profile = resolve(`bench/results/p8-save-profiles/${channel}-${backend}-${Date.now()}`);
      await mkdir(profile, { recursive: true });
      const context = await chromium.launchPersistentContext(profile, {
        channel,
        headless: true,
        viewport: { width: 1280, height: 850 },
        deviceScaleFactor: 1,
        acceptDownloads: true,
      });
      try {
        const page = context.pages()[0];
        page.setDefaultTimeout(600000);
        const errors = [];
        page.on('pageerror', (e) => errors.push(String(e)));
        await page.goto(
          base + '/?diagnostic=1&document=1' + (backend === 'WebGL2' ? '&gpu=webgl2' : ''),
        );
        await expect(page.getByTestId('engine-status')).toHaveText('Engine ready', {
          timeout: 60000,
        });
        await expect(page.getByTestId('gpu-backend')).toHaveText(backend);
        const gpu = await page.evaluate(async () => {
          const a = await navigator.gpu?.requestAdapter();
          return a
            ? {
                vendor: a.info.vendor,
                architecture: a.info.architecture,
                isFallbackAdapter: a.info.isFallbackAdapter,
              }
            : null;
        });
        const adapter = await page.getByTestId('gpu-backend').getAttribute('title');
        if (
          !gpu?.vendor ||
          gpu.isFallbackAdapter ||
          /swiftshader|warp|software|llvmpipe|basic render/i.test(adapter ?? '')
        )
          throw Error('Hardware GPU required');
        const r = {
          channel,
          backend,
          browser: context.browser()?.version(),
          adapter,
          gpu,
          profile,
          trials: [],
        };
        report.results.push(r);
        await save();
        console.log('opening', channel, backend);
        await page.getByTestId('open-project').setInputFiles(fixture);
        await expect(page.getByTestId('operation-status')).toHaveText(/Opened in/, {
          timeout: 600000,
        });
        await page.waitForFunction(() => {
          const state = JSON.parse(
            document.querySelector('[data-testid="gpu-canvas"]').dataset.snapshot,
          );
          return (
            state?.document?.width === 6000 &&
            state.document.height === 4000 &&
            state.document.tiles === 7680 &&
            state.layers.nodes.length === 20 &&
            !state.operation &&
            !state.painting &&
            state.layers.compositing === 0 &&
            state.visibleTiles > 0 &&
            state.refining === 0
          );
        });
        const before = JSON.parse(
          await page.getByTestId('gpu-canvas').getAttribute('data-snapshot'),
        );
        if (
          before.document.width !== 6000 ||
          before.document.height !== 4000 ||
          before.document.tiles !== 7680 ||
          before.layers.nodes.length !== 20
        )
          throw Error('Wrong dense source');
        r.before = before;
        await save();
        for (let trial = 0; trial < trialCount; trial++) {
          console.log('saving', channel, backend, trial);
          const measure = await page.evaluate(async (trial) => {
            const api = window.__fotoeditor;
            const root = await navigator.storage.getDirectory();
            const dir = await root.getDirectoryHandle('p8-measured-save', { create: true });
            const handle = await dir.getFileHandle('dense.cgfoto', { create: true });
            let peakCpu = 0,
              peakGpu = 0;
            const sample = () => {
              const s = JSON.parse(
                document.querySelector('[data-testid="gpu-canvas"]').dataset.snapshot,
              );
              peakCpu = Math.max(
                peakCpu,
                s.memory.document + s.memory.history + s.memory.temporary,
              );
              peakGpu = Math.max(peakGpu, s.memory.gpu);
            };
            sample();
            const timer = setInterval(sample, 50);
            const start = performance.now();
            let bytes;
            try {
              bytes = await api.saveProjectFile(handle);
            } finally {
              clearInterval(timer);
              sample();
            }
            const saveMs = performance.now() - start;
            const file = await handle.getFile();
            if (file.size !== bytes) throw Error('Saved size mismatch');
            if (peakCpu > 768 * 1048576 || peakGpu > 128 * 1048576)
              throw Error('Logical residency exceeds budget');
            const signature = Array.from(new Uint8Array(await file.slice(0, 4).arrayBuffer()));
            window.__p8Saved = file;
            let timing;
            for (let i = 0; i < 1000; i++) {
              timing = JSON.parse(
                document.querySelector('[data-testid="gpu-canvas"]').dataset.snapshot,
              )?.projectSaveTiming;
              if (timing?.serial === trial + 1) break;
              await new Promise((resolve) => setTimeout(resolve, 5));
            }
            if (timing?.serial !== trial + 1) throw Error('Missing save timing');
            return {
              trial,
              saveMs,
              bytes,
              peakCpu,
              peakGpu,
              signature,
              timing,
              belowTarget: saveMs < 3000,
            };
          }, trial);
          r.trials.push(measure);
          await save();
          console.log(JSON.stringify(measure));
        }
        const warm = r.trials
          .slice(1)
          .map((trial) => trial.saveMs)
          .sort((a, b) => a - b);
        r.warmPercentilesMs = warm.length
          ? Object.fromEntries(
              [
                ['p50', 0.5],
                ['p95', 0.95],
                ['p99', 0.99],
              ].map(([name, p]) => [name, warm[Math.ceil(warm.length * p) - 1]]),
            )
          : null;
        const path = resolve(`bench/results/p8-save-batched-${channel}-${backend}.cgfoto`);
        const download = page.waitForEvent('download');
        await page.evaluate(() => {
          const a = document.createElement('a');
          a.href = URL.createObjectURL(window.__p8Saved);
          a.download = 'dense.cgfoto';
          a.click();
        });
        await (await download).saveAs(path);
        r.output = { path, bytes: (await stat(path)).size, sha256: await digest(path) };
        r.errors = errors;
        if (errors.length) throw Error(errors.join('\n'));
        await save();
      } finally {
        await context.close();
      }
    }
  report.complete = true;
  await save();
} catch (error) {
  report.error = String(error);
  await save();
  throw error;
} finally {
  server.kill();
}
