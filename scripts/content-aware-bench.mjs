import { chromium, expect } from '@playwright/test';
import { spawn, execFileSync } from 'node:child_process';
import { readFile, readdir, writeFile, mkdir } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { resolve } from 'node:path';
import os from 'node:os';
import { setTimeout as delay } from 'node:timers/promises';

const base = process.env.PERF_BASE_URL ?? 'http://127.0.0.1:4188';
const out = process.env.CONTENT_AWARE_REPORT ?? 'bench/results/p9-m3-performance.json';
const hash = (bytes) => createHash('sha256').update(bytes).digest('hex');
const fixture = await readFile('bench/results/fixtures/large.png');
const percentile = (a, p) =>
  [...a].sort((x, y) => x - y)[Math.min(a.length - 1, Math.floor(a.length * p))];
const report = {
  measuredAt: new Date().toISOString(),
  revision: execFileSync('git', ['rev-parse', 'HEAD'], {
    encoding: 'utf8',
    windowsHide: true,
  }).trim(),
  dirty: Boolean(
    execFileSync('git', ['status', '--porcelain'], { encoding: 'utf8', windowsHide: true }).trim(),
  ),
  machine: { os: os.version(), cpu: os.cpus()[0].model, ram: os.totalmem() },
  fixture: {
    file: 'bench/results/fixtures/large.png',
    bytes: fixture.length,
    sha256: hash(fixture),
    width: 6000,
    height: 4000,
  },
  runtimeAssets: Object.fromEntries(
    await Promise.all(
      (await readdir('apps/web/dist/assets'))
        .filter((n) => /\.(wasm|js|css)$/.test(n))
        .map(async (n) => [n, hash(await readFile('apps/web/dist/assets/' + n))]),
    ),
  ),
  method:
    'Serial installed hardware Chrome/Edge, actual WebGPU/WebGL2, fresh context per API/source. Normal populated 6000x4000 document at 100%, 1280x850 DPR1. Centered 48x48 and 200x200 hard rectangular selections, current raw raster or all visible into separate empty raster. Fixed deterministic seed, 7x7 patches, six iterations per scale, automatic 128..512px neighborhood. First and three warm Apply calls per size; exact Undo outside timing. OS/file caches not cleared. API-to-observed-committed-frame includes automation/worker/cooperative CPU/GPU/persistence; kernel timing excludes yields/composition. Worker probes read logical CPU/GPU residency at cooperative slices, before commit and across composition tiles/readback; main-thread observation adds presented-frame cadence and maximum snapshot gap. Memory includes admitted scratch capacity, with at most 2.25 MiB additional synchronous compositor scratch reported separately; no physical RAM/VRAM or changed-pixel fps claim. No separate roadmap M3 speed threshold.',
  results: [],
};
await mkdir('bench/results', { recursive: true });
const save = () => writeFile(out, JSON.stringify(report, null, 2) + '\n');
await save();
const server = process.env.PERF_BASE_URL
  ? undefined
  : spawn(
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
      // The preview process may still be starting.
    }
    await delay(100);
  }
  for (const channel of (process.env.PERF_CHANNELS ?? 'chrome,msedge').split(',')) {
    const browser = await chromium.launch({ channel });
    try {
      for (const backend of (process.env.PERF_BACKENDS ?? 'WebGPU,WebGL2').split(','))
        for (const mode of (process.env.CONTENT_AWARE_MODES ?? 'current,all').split(',')) {
          const context = await browser.newContext({
            viewport: { width: 1280, height: 850 },
            deviceScaleFactor: 1,
          });
          try {
            const page = await context.newPage();
            page.setDefaultTimeout(180000);
            const errors = [];
            page.on('pageerror', (e) => errors.push(e.message));
            await page.goto(
              base + '/?diagnostic=1&document=1' + (backend === 'WebGL2' ? '&gpu=webgl2' : ''),
            );
            await expect(page.getByTestId('engine-status')).toHaveText('Engine ready');
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
              !gpu ||
              gpu.isFallbackAdapter ||
              ['google', 'microsoft'].includes(gpu.vendor) ||
              /swiftshader|warp|software|llvmpipe|basic render/i.test(adapter ?? '') ||
              (backend === 'WebGL2' && !adapter)
            )
              throw Error('Hardware required: ' + JSON.stringify({ gpu, adapter }));
            await expect(page.getByTestId('gpu-backend')).toHaveText(backend);
            await page
              .getByTestId('open-image')
              .setInputFiles(resolve('bench/results/fixtures/large.png'));
            await expect(page.getByTestId('operation-status')).toHaveText(/Opened in/);
            await page.getByRole('button', { name: 'Actual pixels', exact: true }).click();
            if (mode === 'all')
              await page.getByRole('button', { name: '+ Layer', exact: true }).click();
            await page.waitForFunction((mode) => {
              const s = JSON.parse(
                document.querySelector('[data-testid="gpu-canvas"]').dataset.snapshot,
              );
              return (
                s.document.width === 6000 &&
                s.visibleTiles > 0 &&
                s.refining === 0 &&
                !s.operation &&
                s.layers.nodes.length === (mode === 'all' ? 2 : 1)
              );
            }, mode);
            for (const size of (process.env.CONTENT_AWARE_SIZES ?? '48,200')
              .split(',')
              .map(Number)) {
              const prepared = await page.evaluate(
                async (size) =>
                  window.__fotoeditor.selectionAction({
                    kind: 'shape',
                    shape: {
                      kind: 'rectangle',
                      x0: 3000 - size / 2,
                      y0: 2000 - size / 2,
                      x1: 3000 + size / 2,
                      y1: 2000 + size / 2,
                    },
                    mode: 'new',
                    antialias: false,
                  }),
                size,
              );
              await page.waitForFunction(
                (rev) =>
                  JSON.parse(document.querySelector('[data-testid="gpu-canvas"]').dataset.snapshot)
                    .revision === rev,
                prepared.revision,
              );
              const trials = [];
              const exportHash = () =>
                page.evaluate(async () => {
                  const bytes = await window.__fotoeditor.exportPng();
                  return Array.from(
                    new Uint8Array(await crypto.subtle.digest('SHA-256', bytes)),
                    (n) => n.toString(16).padStart(2, '0'),
                  ).join('');
                });
              const beforeHash = await exportHash();
              for (let trial = 0; trial < 4; trial++) {
                await page.waitForFunction(() => {
                  const s = JSON.parse(
                    document.querySelector('[data-testid="gpu-canvas"]').dataset.snapshot,
                  );
                  return !s.operation && !s.painting && s.refining === 0;
                });
                const monitor = await page.evaluateHandle(() => {
                  const canvas = document.querySelector('[data-testid="gpu-canvas"]');
                  const s = JSON.parse(canvas.dataset.snapshot);
                  const data = {
                    baseline: s.history.undo,
                    periods: [],
                    cpu: [s.memory.document + s.memory.history + s.memory.temporary],
                    gpu: [s.memory.gpu],
                    last: performance.now(),
                    frame: Number(canvas.dataset.frames),
                  };
                  const observer = new MutationObserver(() => {
                    const f = Number(canvas.dataset.frames);
                    if (f === data.frame) return;
                    const now = performance.now(),
                      s = JSON.parse(canvas.dataset.snapshot);
                    data.periods.push(now - data.last);
                    data.last = now;
                    data.frame = f;
                    data.cpu.push(s.memory.document + s.memory.history + s.memory.temporary);
                    data.gpu.push(s.memory.gpu);
                  });
                  observer.observe(canvas, { attributes: true, attributeFilter: ['data-frames'] });
                  return { data, observer };
                });
                const started = performance.now();
                const result = await page.evaluate(
                  async (mode) =>
                    window.__fotoeditor.selectionAction({
                      kind: 'content-aware',
                      allLayers: mode === 'all',
                    }),
                  mode,
                );
                await page.waitForFunction(
                  ({ count, revision }) => {
                    const s = JSON.parse(
                      document.querySelector('[data-testid="gpu-canvas"]').dataset.snapshot,
                    );
                    if (s.editError) throw Error(s.editError);
                    return (
                      s.history.undo === count + 1 &&
                      s.revision === revision &&
                      !s.operation &&
                      !s.selection.busy
                    );
                  },
                  { count: prepared.history.undo, revision: result.revision },
                );
                const elapsedMs = performance.now() - started;
                const raw = await monitor.evaluate(({ data, observer }) => {
                  observer.disconnect();
                  return data;
                });
                await monitor.dispose();
                const peakLogicalCpuBytes = Math.max(
                  ...raw.cpu,
                  result.selection.timing.cpuPeakBytes,
                );
                const peakLogicalGpuBytes = Math.max(
                  ...raw.gpu,
                  result.selection.timing.gpuPeakBytes,
                );
                if (
                  errors.length ||
                  !Number.isFinite(peakLogicalCpuBytes) ||
                  !Number.isFinite(peakLogicalGpuBytes) ||
                  peakLogicalCpuBytes > 768 * 1024 ** 2 ||
                  peakLogicalGpuBytes > 128 * 1024 ** 2
                )
                  throw Error('Invalid resource result: ' + JSON.stringify({ raw, errors }));
                trials.push({
                  trial,
                  elapsedMs,
                  kernelMs: result.selection.timing.kernelMs,
                  workerTotalMs: result.selection.timing.totalMs,
                  cadenceP50: percentile(raw.periods, 0.5),
                  cadenceP95: percentile(raw.periods, 0.95),
                  cadenceP99: percentile(raw.periods, 0.99),
                  maxSnapshotGapMs: Math.max(...raw.periods),
                  peakLogicalCpuBytes,
                  peakLogicalGpuBytes,
                  maxStepMs: result.selection.timing.maxStepMs,
                });
                if (trial === 3)
                  await page.screenshot({
                    path: `bench/results/p9-m3-bench-${channel}-${backend}-${mode}-${size}.png`,
                  });
                await page.getByRole('button', { name: 'Undo', exact: true }).click();
                await page.waitForFunction((count) => {
                  const s = JSON.parse(
                    document.querySelector('[data-testid="gpu-canvas"]').dataset.snapshot,
                  );
                  if (s.editError) throw Error(s.editError);
                  return s.history.undo === count && !s.operation;
                }, prepared.history.undo);
              }
              const warm = trials.slice(1).map((t) => t.elapsedMs);
              const undoHash = await exportHash();
              if (undoHash !== beforeHash)
                throw Error('Benchmark undo did not restore exact exported samples');
              const row = {
                channel,
                browser: browser.version(),
                backend,
                mode,
                size,
                gpu,
                adapter,
                firstMs: trials[0].elapsedMs,
                warmP50: percentile(warm, 0.5),
                warmP95: percentile(warm, 0.95),
                warmP99: percentile(warm, 0.99),
                trials,
                beforeHash,
                undoHash,
                exactUndo: true,
                errors,
              };
              report.results.push(row);
              await save();
              console.log(JSON.stringify(row));
            }
          } finally {
            await context.close();
          }
        }
    } finally {
      await browser.close();
    }
  }
} finally {
  server?.kill();
  await save();
}
