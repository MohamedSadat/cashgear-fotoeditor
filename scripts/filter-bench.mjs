import { chromium, expect } from '@playwright/test';
import { spawn } from 'node:child_process';
import { readFile, writeFile, stat, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { createHash } from 'node:crypto';
import os from 'node:os';
import { setTimeout as delay } from 'node:timers/promises';
const base = process.env.PERF_BASE_URL ?? 'http://127.0.0.1:4188';
const fixture = resolve('bench/results/fixtures/p5-photo.png');
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
const results = [];
const trialCount = Number(process.env.PERF_TRIALS ?? 6);
if (!Number.isInteger(trialCount) || trialCount < 2 || trialCount > 20)
  throw new Error('PERF_TRIALS must be 2-20');
function percentiles(values) {
  const sorted = [...values].sort((a, b) => a - b);
  return Object.fromEntries(
    [
      ['p50', 0.5],
      ['p95', 0.95],
      ['p99', 0.99],
    ].map(([name, p]) => [name, sorted[Math.ceil(sorted.length * p) - 1]]),
  );
}
try {
  for (let i = 0; i < 100; i++) {
    try {
      if ((await fetch(base)).ok) break;
    } catch {
      /* Server starting. */
    }
    await delay(100);
  }
  for (const channel of process.env.PERF_BROWSER
    ? [process.env.PERF_BROWSER]
    : ['chrome', 'msedge']) {
    const browser = await chromium.launch({ channel });
    try {
      for (const backend of process.env.PERF_BACKEND
        ? [process.env.PERF_BACKEND]
        : ['WebGPU', 'WebGL2']) {
        const context = await browser.newContext({
          viewport: { width: 1280, height: 850 },
          deviceScaleFactor: 1,
        });
        const page = await context.newPage();
        await page.goto(
          base + '/?diagnostic=1&document=1' + (backend === 'WebGL2' ? '&gpu=webgl2' : ''),
        );
        await expect(page.getByTestId('engine-status')).toHaveText('Engine ready');
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
          /swiftshader|warp|software|llvmpipe|basic render/i.test(adapter ?? '') ||
          ['google', 'microsoft'].includes(gpu.vendor)
        )
          throw new Error('Hardware GPU required');
        await page.getByTestId('open-image').setInputFiles(fixture);
        await expect(page.getByTestId('operation-status')).toHaveText(/Opened in/);
        const scenarios = [];
        const filters =
          process.env.PERF_FILTERS === 'remaining'
            ? [
                { kind: 'motion-blur', radius: 20, angle: 31 },
                { kind: 'high-pass', radius: 20 },
                {
                  kind: 'add-noise',
                  amount: 0.1,
                  seed: 123,
                  monochrome: false,
                  distribution: 'gaussian',
                },
                { kind: 'emboss', radius: 3, angle: 135, amount: 1 },
                { kind: 'pixelate', size: 32 },
              ]
            : [{ kind: 'gaussian', radius: 20 }];
        for (const filter of filters) {
          const trials = [];
          for (let trial = 0; trial < trialCount; trial++) {
            trials.push(
              await page.evaluate(
                async ({ filter, trial }) => {
                  const api = Reflect.get(window, '__fotoeditor');
                  const canvas = document.querySelector('[data-testid="gpu-canvas"]');
                  const baselineStarted = performance.now();
                  await api.historyJump(0);
                  let ready = false;
                  for (let i = 0; i < 300; i++) {
                    const shown = JSON.parse(canvas.dataset.snapshot ?? 'null');
                    if (
                      shown?.history.undo === 0 &&
                      shown.visibleTiles > 0 &&
                      shown.refining === 0 &&
                      !shown.operation
                    ) {
                      ready = true;
                      break;
                    }
                    await new Promise((r) => requestAnimationFrame(r));
                  }
                  if (!ready) throw new Error('Undo baseline was not presented and refined');
                  const baselineReadyMs = performance.now() - baselineStarted;
                  const initial = JSON.parse(canvas.dataset.snapshot);
                  if (initial.document.width !== 6000 || initial.document.height !== 4000)
                    throw new Error('6000×4000 fixture required');
                  const samples = [];
                  const timer = setInterval(() => {
                    const s = JSON.parse(canvas.dataset.snapshot ?? 'null');
                    if (s) samples.push(s.memory);
                  }, 2);
                  const start = performance.now();
                  try {
                    const state = await api.applyFilter(filter);
                    const returnMs = performance.now() - start;
                    samples.push(state.memory);
                    let visible = false;
                    for (let i = 0; i < 240; i++) {
                      const shown = JSON.parse(canvas.dataset.snapshot ?? 'null');
                      if (
                        shown?.contentRevision === state.contentRevision &&
                        shown.visibleTiles > 0
                      ) {
                        visible = true;
                        break;
                      }
                      await new Promise((r) => requestAnimationFrame(r));
                    }
                    if (!visible) throw new Error('Committed filter was not presented');
                    const visibleMs = performance.now() - start;
                    const categoryMaxima = { document: 0, history: 0, temporary: 0, gpu: 0 };
                    let sampledCpuPeak = 0;
                    for (const s of samples) {
                      for (const k of Object.keys(categoryMaxima))
                        categoryMaxima[k] = Math.max(categoryMaxima[k], s[k]);
                      sampledCpuPeak = Math.max(
                        sampledCpuPeak,
                        s.document + s.history + s.temporary,
                      );
                    }
                    const histogram = await api.histogram(
                      state.layers.active ?? state.layers.nodes[0].id,
                    );
                    if (histogram.pixels !== 24000000 || histogram.weight !== 6120000000)
                      throw new Error('Histogram did not visit every photo sample');
                    if (
                      Math.max(sampledCpuPeak, state.filterTiming.cpuPeakBytes) > 768 * 1048576 ||
                      categoryMaxima.gpu > 128 * 1048576
                    )
                      throw new Error('Residency budget exceeded');
                    return {
                      trial,
                      baselineReadyMs,
                      returnMs,
                      visibleMs,
                      ...state.filterTiming,
                      histogramMs: histogram.milliseconds,
                      categoryMaxima,
                      sampledCpuPeak,
                    };
                  } finally {
                    clearInterval(timer);
                  }
                },
                { filter, trial },
              ),
            );
          }
          scenarios.push({
            filter,
            trials,
            warmReturnMs: percentiles(trials.slice(1).map((t) => t.returnMs)),
            warmVisibleMs: percentiles(trials.slice(1).map((t) => t.visibleMs)),
            warmHistogramMs: percentiles(trials.slice(1).map((t) => t.histogramMs)),
          });
        }
        results.push({ channel, backend, browser: browser.version(), adapter, gpu, scenarios });
        console.log(
          JSON.stringify({
            channel,
            backend,
            scenarios: scenarios.map((s) => ({
              kind: s.filter.kind,
              firstMs: s.trials[0].returnMs,
              warm: s.warmReturnMs,
              histogram: s.warmHistogramMs,
            })),
          }),
        );
        await context.close();
      }
    } finally {
      await browser.close();
    }
  }
} finally {
  server?.kill();
  const assets = resolve('apps/web/dist/assets');
  const files = await readdir(assets);
  const wasm = files.find((p) => p.endsWith('.wasm'));
  const worker = files.find((p) => p.startsWith('worker-') && p.endsWith('.js'));
  await writeFile(
    process.env.PERF_OUTPUT ?? 'bench/results/p6-m4-filter-performance.json',
    JSON.stringify(
      {
        measuredAt: new Date().toISOString(),
        machine: { os: os.version(), cpu: os.cpus()[0].model, ram: os.totalmem() },
        viewport: { width: 1280, height: 850, dpr: 1 },
        wasm: {
          file: wasm,
          sha256: createHash('sha256')
            .update(await readFile(resolve(assets, wasm)))
            .digest('hex'),
        },
        worker: {
          file: worker,
          sha256: createHash('sha256')
            .update(await readFile(resolve(assets, worker)))
            .digest('hex'),
        },
        dataset: {
          path: fixture,
          bytes: (await stat(fixture)).size,
          sha256: createHash('sha256')
            .update(await readFile(fixture))
            .digest('hex'),
          source: 'https://science.nasa.gov/resource/setting-the-wind-and-thermal-shield/',
          credit: 'NASA/JPL-Caltech/IPGP',
          preparation:
            'Rust crops [1,0,6000,4000] from 6002×4000 official JPEG, without resampling.',
        },
        trialCount,
        method:
          'One first application and trialCount minus one warm trials per scenario, from the original unselected raster. After Undo, the original must be presented and refinement idle before starting the timer; baseline readiness is recorded separately. Single worker thread. API return includes the command, visible time also waits for frame acknowledgement; histogram is separate. Logical buffer residency excludes allocator/driver overhead. All source samples are already resident; this is not a disk-cold paging measurement.',
        results,
      },
      null,
      2,
    ) + '\n',
  );
}
