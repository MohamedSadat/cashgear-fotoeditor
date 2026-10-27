import { chromium, expect } from '@playwright/test';
import { spawn } from 'node:child_process';
import { mkdir, stat, writeFile, readdir } from 'node:fs/promises';
import { createReadStream } from 'node:fs';
import { createHash } from 'node:crypto';
import { resolve } from 'node:path';
import os from 'node:os';
import { setTimeout as delay } from 'node:timers/promises';
const base = 'http://127.0.0.1:4188';
const fixture = resolve('bench/results/fixtures/p5-photo.png');
const trialCount = Number(process.env.P8_EXPORT_TRIALS ?? 4);
if (!Number.isInteger(trialCount) || trialCount < 1 || trialCount > 10)
  throw Error('P8_EXPORT_TRIALS must be 1-10');
const digest = async (path) => {
  const hash = createHash('sha256');
  for await (const bytes of createReadStream(path)) hash.update(bytes);
  return hash.digest('hex');
};
const report = {
  measuredAt: new Date().toISOString(),
  machine: { os: os.version(), cpu: os.cpus()[0].model, ram: os.totalmem() },
  dataset: {
    file: fixture,
    bytes: (await stat(fixture)).size,
    sha256: await digest(fixture),
    width: 6000,
    height: 4000,
  },
  runtimeAssets: await Promise.all(
    (await readdir('apps/web/dist/assets'))
      .filter((file) => /\.(js|wasm)$/.test(file))
      .map(async (file) => ({ file, sha256: await digest(resolve('apps/web/dist/assets', file)) })),
  ),
  method:
    'Sequential hardware browser/API pairs with fresh D: profiles. Original 24 MP photo remains unchanged. PNG then JPEG then lossy/lossless WebP, each first and three warm trials by default. Total API time includes source capture/hydration/persistence, isolated codec startup/encode, output storage/flush and File publication. OS caches not cleared. Reserved logical CPU sampled at 20 ms includes the complete admitted helper allowance; allocator peak records actual live codec allocation capacities (conservative moving realloc), excluding JS input counted in owner baseline. Excludes allocator arena overhead, browser/driver/process memory. No image-export latency threshold was specified.',
  results: [],
};
const output = 'bench/results/p8-export-performance.json';
const save = () => writeFile(output, JSON.stringify(report, null, 2) + '\n');
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
      /* Wait for preview. */
    }
    await delay(100);
  }
  for (const channel of (process.env.P8_EXPORT_CHANNELS ?? 'chrome,msedge').split(','))
    for (const backend of (process.env.P8_EXPORT_BACKENDS ?? 'WebGPU,WebGL2').split(',')) {
      const profile = resolve(
        `bench/results/p8-export-profiles/${channel}-${backend}-${Date.now()}`,
      );
      await mkdir(profile, { recursive: true });
      const context = await chromium.launchPersistentContext(profile, {
        channel,
        headless: true,
        viewport: { width: 1280, height: 850 },
        deviceScaleFactor: 1,
      });
      try {
        const page = context.pages()[0];
        page.setDefaultTimeout(600000);
        const errors = [];
        page.on('pageerror', (error) => errors.push(String(error)));
        await page.goto(
          base + '/?diagnostic=1&document=1' + (backend === 'WebGL2' ? '&gpu=webgl2' : ''),
        );
        await expect(page.getByTestId('engine-status')).toHaveText('Engine ready');
        await expect(page.getByTestId('gpu-backend')).toHaveText(backend);
        const gpu = await page.evaluate(async () => {
          const adapter = await navigator.gpu?.requestAdapter();
          return adapter
            ? {
                vendor: adapter.info.vendor,
                architecture: adapter.info.architecture,
                isFallbackAdapter: adapter.info.isFallbackAdapter,
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
        await page.getByTestId('open-image').setInputFiles(fixture);
        await expect(page.getByTestId('operation-status')).toHaveText(/Opened in/, {
          timeout: 600000,
        });
        await page.waitForFunction(() => {
          const state = JSON.parse(
            document.querySelector('[data-testid="gpu-canvas"]').dataset.snapshot,
          );
          return (
            state?.document &&
            !state.operation &&
            !state.painting &&
            state.visibleTiles > 0 &&
            state.refining === 0
          );
        });
        const documentSize = await page.evaluate(
          () =>
            JSON.parse(document.querySelector('[data-testid="gpu-canvas"]').dataset.snapshot)
              .document,
        );
        if (documentSize.width !== 6000 || documentSize.height !== 4000)
          throw Error('Wrong 24 MP export fixture');
        const result = {
          channel,
          backend,
          browser: context.browser()?.version(),
          gpu,
          adapter,
          profile,
          trials: [],
        };
        report.results.push(result);
        await save();
        for (const [format, lossless] of [
          ['png', false],
          ['jpeg', false],
          ['webp', false],
          ['webp', true],
        ])
          for (let trial = 0; trial < trialCount; trial++) {
            console.log('exporting', channel, backend, format, lossless, trial);
            const measured = await page.evaluate(
              async ({ format, lossless, trial }) => {
                const api = window.__fotoeditor;
                const state = () =>
                  JSON.parse(document.querySelector('[data-testid="gpu-canvas"]').dataset.snapshot);
                const serial = (state()?.imageExportTiming?.serial ?? 0) + 1;
                let reservedCpuPeak = 0,
                  gpuPeak = 0;
                const sample = () => {
                  const s = state();
                  if (!s) return;
                  reservedCpuPeak = Math.max(
                    reservedCpuPeak,
                    s.memory.document + s.memory.history + s.memory.temporary,
                  );
                  gpuPeak = Math.max(gpuPeak, s.memory.gpu);
                };
                const timer = setInterval(sample, 20);
                const start = performance.now();
                let file;
                try {
                  file = await api.exportImage({
                    format,
                    lossless,
                    source: 'full',
                    quality: 85,
                    embedIcc: true,
                    matte: [255, 255, 255],
                  });
                } finally {
                  clearInterval(timer);
                  sample();
                }
                const totalMs = performance.now() - start;
                let timing;
                for (let i = 0; i < 1000; i++) {
                  timing = state()?.imageExportTiming;
                  if (timing?.serial === serial) break;
                  await new Promise((resolve) => setTimeout(resolve, 5));
                }
                if (timing?.serial !== serial || timing.bytes !== file.size)
                  throw Error('Missing export timing');
                if (
                  reservedCpuPeak > 768 * 1048576 ||
                  gpuPeak > 128 * 1048576 ||
                  timing.logicalCpuAtCodecPeakBytes > 768 * 1048576 ||
                  timing.codecHeapPeakBytes > timing.codecHeapLimitBytes
                )
                  throw Error('Export memory limit exceeded');
                return {
                  format,
                  lossless,
                  trial,
                  totalMs,
                  bytes: file.size,
                  reservedCpuPeak,
                  gpuPeak,
                  timing,
                };
              },
              { format, lossless, trial },
            );
            result.trials.push(measured);
            console.log(JSON.stringify(measured));
            await save();
          }
        result.errors = errors;
        if (errors.length) throw Error(errors.join('\n'));
        if (channel === 'chrome' && backend === 'WebGPU') {
          await page.getByRole('button', { name: 'Export image…', exact: true }).click();
          await page.getByLabel('Export format').selectOption('jpeg');
          await page.screenshot({ path: 'bench/results/p8-export-dialog.png' });
        }
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
