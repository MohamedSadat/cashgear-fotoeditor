import { chromium, expect } from '@playwright/test';
import { spawn, execFileSync } from 'node:child_process';
import { mkdir, readFile, readdir, writeFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { resolve } from 'node:path';
import os from 'node:os';
import { setTimeout as delay } from 'node:timers/promises';

const base = process.env.PERF_BASE_URL ?? 'http://127.0.0.1:4188';
const out = process.env.HEALING_REPORT ?? 'bench/results/p9-m2-performance.json';
const hash = (bytes) => createHash('sha256').update(bytes).digest('hex');
const fixture = await readFile('bench/results/fixtures/large.png');
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
    'Serial installed hardware browsers, fresh context per API/source mode, ordinary populated 6000x4000 document at 100% / 1280x850 / DPR1. Hard round 48 and 200 px, opacity/flow 100%, pressure 1. Four native mouse click strokes per tool/size, exact undo outside timing restores original between trials. First-use and three warm trials; OS/file caches not cleared. Current raw raster or all visible into a separate empty retouch raster. Healing source [0,-180], aligned; Spot Healing searches automatically. Click-to-committed presentation includes automation/worker/CPU/GPU and persistence. Sample-to-completed-dab GPU acknowledgement excludes scanout/compositor and physical input. Cadence includes unchanged active previews, never presented as changed-pixel throughput. Logical memory excludes allocator/browser/driver overhead. No separate M2 speed threshold exists.',
  results: [],
  pathMethod:
    'After the four click trials at 48px, one 48px horizontal path uses native down, four 12px moves with requested 16ms pauses, and up. Report actual completion/ack latency and queue depth; no fixed delivered input rate is assumed. A click has down/up endpoint samples; completed acknowledgements include unchanged endpoints.',
};
const save = () => writeFile(out, JSON.stringify(report, null, 2) + '\n');
await mkdir('bench/results', { recursive: true });
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
const percentile = (values, p) =>
  values.length
    ? [...values].sort((a, b) => a - b)[Math.min(values.length - 1, Math.floor(values.length * p))]
    : null;
try {
  for (let i = 0; i < 100; i++) {
    try {
      if ((await fetch(base)).ok) break;
    } catch {
      /* server startup */
    }
    await delay(100);
  }
  for (const channel of (process.env.PERF_CHANNELS ?? 'chrome,msedge').split(',')) {
    const browser = await chromium.launch({ channel });
    try {
      for (const backend of (process.env.PERF_BACKENDS ?? 'WebGPU,WebGL2').split(','))
        for (const mode of (process.env.HEALING_MODES ?? 'current,all').split(',')) {
          const context = await browser.newContext({
            viewport: { width: 1280, height: 850 },
            deviceScaleFactor: 1,
          });
          const page = await context.newPage();
          page.setDefaultTimeout(120000);
          const errors = [];
          page.on('pageerror', (e) => errors.push(e.message));
          await page.goto(base + (backend === 'WebGL2' ? '/?gpu=webgl2' : '/'));
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
          await expect(page.getByTestId('zoom')).toHaveText('100.0%');
          if (mode === 'all') {
            await page.getByRole('button', { name: '+ Layer', exact: true }).click();
          }
          await page.waitForFunction((mode) => {
            const s = JSON.parse(
              document.querySelector('[data-testid="gpu-canvas"]').dataset.snapshot,
            );
            return (
              s.document.width === 6000 &&
              s.document.height === 4000 &&
              s.visibleTiles > 0 &&
              s.refining === 0 &&
              !s.operation &&
              s.layers.nodes.length === (mode === 'all' ? 2 : 1)
            );
          }, mode);
          await page.getByRole('combobox', { name: 'Brush preset' }).selectOption('Hard round');
          for (const size of (process.env.HEALING_SIZES ?? '48,200').split(',').map(Number))
            for (const tool of ['Healing Brush', 'Spot Healing']) {
              await page.getByRole('button', { name: tool, exact: true }).click();
              await page.getByRole('combobox', { name: 'Healing sample' }).selectOption(mode);
              await page
                .getByRole('slider', { name: 'Brush size', exact: true })
                .evaluate((el, n) => {
                  Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set.call(
                    el,
                    String(n),
                  );
                  el.dispatchEvent(new Event('input', { bubbles: true }));
                  el.dispatchEvent(new Event('change', { bubbles: true }));
                }, size);
              const box = await page.getByTestId('gpu-canvas').boundingBox(),
                cx = box.x + box.width / 2,
                cy = box.y + box.height / 2;
              if (tool === 'Healing Brush') {
                await page.keyboard.down('Alt');
                await page.mouse.click(cx, cy - 180);
                await page.keyboard.up('Alt');
                await page.waitForFunction(
                  () =>
                    JSON.parse(
                      document.querySelector('[data-testid="gpu-canvas"]').dataset.snapshot,
                    ).cloneStamp.source !== null,
                );
              }
              const trials = [];
              for (let trial = 0; trial < (size === 48 ? 5 : 4); trial++) {
                await page.waitForFunction(() => {
                  const s = JSON.parse(
                    document.querySelector('[data-testid="gpu-canvas"]').dataset.snapshot,
                  );
                  return !s.painting && !s.operation && s.refining === 0;
                });
                const monitor = await page.evaluateHandle(() => {
                  const canvas = document.querySelector('[data-testid="gpu-canvas"]'),
                    initial = JSON.parse(canvas.dataset.snapshot);
                  const data = {
                    baseline: initial.history.undo,
                    previousPaint: initial.paintTime,
                    previousComplete: initial.paintCompleteTime,
                    lastFrame: Number(canvas.dataset.frames),
                    lastTime: performance.now(),
                    periods: [],
                    changedLatencies: [],
                    completeLatencies: [],
                    cpu: [],
                    gpu: [],
                    paintMs: [],
                    pending: [],
                  };
                  const observer = new MutationObserver(() => {
                    const frame = Number(canvas.dataset.frames);
                    if (frame === data.lastFrame) return;
                    const now = performance.now(),
                      s = JSON.parse(canvas.dataset.snapshot);
                    data.periods.push(now - data.lastTime);
                    data.lastFrame = frame;
                    data.lastTime = now;
                    data.cpu.push(s.memory.document + s.memory.history + s.memory.temporary);
                    data.gpu.push(s.memory.gpu);
                    data.pending.push(s.pendingSamples);
                    data.paintMs.push(JSON.parse(canvas.dataset.timing).paintMs);
                    if (s.paintTime > data.previousPaint) {
                      data.changedLatencies.push(now - s.paintTime);
                      data.previousPaint = s.paintTime;
                    }
                    if (s.paintCompleteTime > data.previousComplete) {
                      data.completeLatencies.push(now - s.paintCompleteTime);
                      data.previousComplete = s.paintCompleteTime;
                    }
                  });
                  observer.observe(canvas, { attributes: true, attributeFilter: ['data-frames'] });
                  return { data, observer };
                });
                const baseline = await monitor.evaluate(({ data }) => data.baseline),
                  started = performance.now();
                if (trial === 4) {
                  await page.mouse.move(cx - 24, cy);
                  await page.mouse.down();
                  for (let step = 1; step <= 4; step++) {
                    await page.mouse.move(cx - 24 + step * 12, cy);
                    await delay(16);
                  }
                  await page.mouse.up();
                } else await page.mouse.click(cx, cy);
                await page.waitForFunction((baseline) => {
                  const s = JSON.parse(
                    document.querySelector('[data-testid="gpu-canvas"]').dataset.snapshot,
                  );
                  if (s.editError) throw Error(s.editError);
                  return s.history.undo === baseline + 1 && !s.painting && !s.operation;
                }, baseline);
                const elapsedMs = performance.now() - started;
                const raw = await monitor.evaluate(({ data, observer }) => {
                  observer.disconnect();
                  return data;
                });
                await monitor.dispose();
                if (
                  raw.changedLatencies.length === 0 ||
                  raw.completeLatencies.length === 0 ||
                  errors.length ||
                  Math.max(...raw.cpu) > 768 * 1024 ** 2 ||
                  Math.max(...raw.gpu) > 128 * 1024 ** 2
                )
                  throw Error(
                    'Missing change/completion or invalid resource result: ' +
                      JSON.stringify({ raw, errors }),
                  );
                trials.push({
                  trial,
                  kind: trial === 4 ? '48px path, four native moves, requested 16ms pauses' : 'dab',
                  elapsedMs,
                  changedAcknowledgements: raw.changedLatencies.length,
                  completeAcknowledgements: raw.completeLatencies.length,
                  sampleToChangedAckMs: raw.changedLatencies,
                  sampleToCompletedDabAckMs: raw.completeLatencies,
                  cadenceP50: percentile(raw.periods, 0.5),
                  cadenceP95: percentile(raw.periods, 0.95),
                  cadenceP99: percentile(raw.periods, 0.99),
                  workerPaintP95: percentile(raw.paintMs, 0.95),
                  peakLogicalCpuBytes: Math.max(...raw.cpu),
                  peakLogicalGpuBytes: Math.max(...raw.gpu),
                  maxPendingSamples: Math.max(...raw.pending),
                });
                if (trial === 3)
                  await page.screenshot({
                    path: `bench/results/p9-m2-bench-${channel}-${backend}-${mode}-${size}-${tool === 'Healing Brush' ? 'healing' : 'spot'}.png`,
                  });
                await page.getByRole('button', { name: 'Undo', exact: true }).click();
                try {
                  await page.waitForFunction((baseline) => {
                    const s = JSON.parse(
                      document.querySelector('[data-testid="gpu-canvas"]').dataset.snapshot,
                    );
                    if (s.editError) throw Error(s.editError);
                    return s.history.undo === baseline;
                  }, baseline);
                } catch (error) {
                  const snapshot = await page
                    .getByTestId('gpu-canvas')
                    .getAttribute('data-snapshot');
                  await writeFile(
                    out.replace('.json', '-failure.json'),
                    JSON.stringify(
                      {
                        channel,
                        backend,
                        mode,
                        tool,
                        size,
                        trial,
                        baseline,
                        error: String(error),
                        snapshot: JSON.parse(snapshot),
                      },
                      null,
                      2,
                    ) + '\n',
                  );
                  await page.screenshot({ path: out.replace('.json', '-failure.png') });
                  throw error;
                }
              }
              const warm = trials.slice(1, 4).map((t) => t.elapsedMs);
              const result = {
                channel,
                browser: browser.version(),
                backend,
                mode,
                tool,
                size,
                gpu,
                adapter,
                firstMs: trials[0].elapsedMs,
                warmP50: percentile(warm, 0.5),
                warmP95: percentile(warm, 0.95),
                warmP99: percentile(warm, 0.99),
                solverAllocationUpperBytes: 105 * (size + 3) ** 2 + 192 * 8,
                trials,
                errors,
              };
              report.results.push(result);
              await save();
              console.log(JSON.stringify(result));
            }
          await context.close();
        }
    } finally {
      await browser.close();
    }
  }
} finally {
  server?.kill();
}
