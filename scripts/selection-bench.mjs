import { chromium, expect } from '@playwright/test';
import { spawn } from 'node:child_process';
import { readFile, writeFile, stat, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { createHash } from 'node:crypto';
import os from 'node:os';
import { setTimeout as delay } from 'node:timers/promises';
const base = process.env.PERF_BASE_URL ?? 'http://127.0.0.1:4188';
const fixture = resolve(process.env.SELECT_FIXTURE ?? 'bench/results/fixtures/p5-photo.png');
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
const percentile = (values, p) =>
  [...values].sort((a, b) => a - b)[Math.min(values.length - 1, Math.floor(values.length * p))];
try {
  for (let i = 0; i < 100; i++) {
    try {
      if ((await fetch(base)).ok) break;
    } catch {
      /* Preview starting. */
    }
    await delay(100);
  }
  for (const channel of process.env.PERF_CHANNEL
    ? [process.env.PERF_CHANNEL]
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
          (backend === 'WebGL2' && !adapter) ||
          !gpu.vendor ||
          gpu.isFallbackAdapter ||
          /swiftshader|warp|software|llvmpipe|basic render/i.test(adapter ?? '') ||
          ['google', 'microsoft'].includes(gpu.vendor)
        )
          throw new Error('Hardware GPU required');
        await page.getByTestId('open-image').setInputFiles(fixture);
        await expect(page.getByTestId('operation-status')).toHaveText(/Opened in/);
        const document = await page.evaluate(
          () =>
            JSON.parse(document.querySelector('[data-testid="gpu-canvas"]').dataset.snapshot)
              .document,
        );
        if (document.width !== 6000 || document.height !== 4000)
          throw new Error('Selection benchmark requires exactly 6000×4000 samples');
        const scenarios = [];
        for (const config of [
          { name: 'contiguous-32', tolerance: 32, contiguous: true },
          { name: 'global-32', tolerance: 32, contiguous: false },
          { name: 'full-connected-255', tolerance: 255, contiguous: true },
        ]) {
          const trials = [];
          for (let trial = 0; trial < 6; trial++) {
            const measured = await page.evaluate(
              async ({ config, trial }) => {
                const api = Reflect.get(window, '__fotoeditor');
                await api.selectionAction({ kind: 'none' });
                const samples = [];
                const canvas = document.querySelector('[data-testid="gpu-canvas"]');
                const timer = setInterval(() => {
                  const s = JSON.parse(canvas.dataset.snapshot ?? 'null');
                  if (s) samples.push(s.memory);
                }, 2);
                const start = performance.now();
                try {
                  const state = await api.selectionAction({
                    kind: 'wand',
                    allLayers: false,
                    options: {
                      seed: [3000, 2000],
                      tolerance: config.tolerance,
                      contiguous: config.contiguous,
                      antialias: true,
                      mode: 'new',
                    },
                  });
                  const returned = performance.now();
                  samples.push(state.memory);
                  let observed = false;
                  for (let wait = 0; wait < 100; wait++) {
                    const shown = JSON.parse(canvas.dataset.snapshot ?? 'null');
                    if (shown?.revision === state.revision) {
                      observed = true;
                      break;
                    }
                    await new Promise((r) => requestAnimationFrame(r));
                  }
                  if (!observed) throw new Error('The committed selection was not presented');
                  const maxima = { document: 0, history: 0, temporary: 0, gpu: 0 };
                  let cpu = 0;
                  for (const s of samples) {
                    for (const k of Object.keys(maxima)) maxima[k] = Math.max(maxima[k], s[k]);
                    cpu = Math.max(cpu, s.document + s.history + s.temporary);
                  }
                  return {
                    trial,
                    returnMs: returned - start,
                    visibleMs: performance.now() - start,
                    ...state.selection.timing,
                    selectedPixels: state.selection.lastWandPixels,
                    categoryMaxima: maxima,
                    sampledCpuPeak: cpu,
                  };
                } finally {
                  clearInterval(timer);
                }
              },
              { config, trial },
            );
            trials.push(measured);
          }
          const warm = trials.slice(1).map((t) => t.returnMs);
          scenarios.push({
            ...config,
            seed: [3000, 2000],
            antialias: true,
            allLayers: false,
            trials,
            warmReturnMs: {
              p50: percentile(warm, 0.5),
              p95: percentile(warm, 0.95),
              p99: percentile(warm, 0.99),
            },
            allReturnsUnder200: trials.every((t) => t.returnMs < 200),
          });
        }
        const result = { channel, backend, browser: browser.version(), adapter, gpu, scenarios };
        results.push(result);
        console.log(
          JSON.stringify({
            channel,
            backend,
            gpu,
            scenarios: scenarios.map((s) => ({
              name: s.name,
              firstMs: s.trials[0].returnMs,
              ...s.warmReturnMs,
              allUnder200: s.allReturnsUnder200,
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
  if (server) server.kill();
  const assets = resolve('apps/web/dist/assets');
  const wasm = (await readdir(assets)).find((p) => p.endsWith('.wasm'));
  await writeFile(
    process.env.SELECT_REPORT ?? 'bench/results/p5-selection-performance.json',
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
        dataset: {
          path: fixture,
          bytes: (await stat(fixture)).size,
          sha256: createHash('sha256')
            .update(await readFile(fixture))
            .digest('hex'),
          source: 'https://science.nasa.gov/resource/setting-the-wind-and-thermal-shield/',
          credit: 'NASA/JPL-Caltech/IPGP',
          preparation:
            'Official JPEG is 6002×4000. Rust crops [1,0,6000,4000] without resampling to an RGB8 PNG.',
        },
        results,
      },
      null,
      2,
    ) + '\n',
  );
}
if (results.some((r) => r.scenarios.some((s) => !s.allReturnsUnder200))) process.exitCode = 1;
