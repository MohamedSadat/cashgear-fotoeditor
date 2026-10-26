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
const percentile = (values) => {
  const a = [...values].sort((a, b) => a - b);
  return Object.fromEntries(
    [
      ['p50', 0.5],
      ['p95', 0.95],
      ['p99', 0.99],
    ].map(([k, p]) => [k, a[Math.ceil(a.length * p) - 1]]),
  );
};
const hash = (bytes) => createHash('sha256').update(bytes).digest('hex');
const assets = resolve('apps/web/dist/assets'),
  wasm = (await readdir(assets)).find((p) => p.endsWith('.wasm'));
const report = {
  measuredAt: new Date().toISOString(),
  machine: { os: os.version(), cpu: os.cpus()[0].model, ram: os.totalmem() },
  viewport: { width: 1280, height: 850, dpr: 1 },
  wasm: { file: wasm, sha256: hash(await readFile(resolve(assets, wasm))) },
  dataset: {
    path: fixture,
    bytes: (await stat(fixture)).size,
    sha256: hash(await readFile(fixture)),
    width: 6000,
    height: 4000,
    source: 'https://science.nasa.gov/resource/setting-the-wind-and-thermal-shield/',
    credit: 'NASA/JPL-Caltech/IPGP',
    preparation: 'Rust crops [1,0,6000,4000] from the 6002×4000 official JPEG, without resampling.',
  },
  method:
    'Hardware only, sequential browser/API pairs. Single WASM worker thread. Original resident 24 MP source restored by undo before every Apply. First operation and three warm repeats per scenario; nearest-rank warm p50/p95/p99 (only three samples, p95/p99 are maxima). Total API return includes persistence/hydration, command and composite; presented time additionally waits for captured frame content revision. Preview uses a 1536-side source cache and fitted viewport, with first-use preparation separate from 20 serial warm updates per kernel. Preview API measurements exclude pointer/UI event coalescing, browser compositor and scanout. Logical CPU/GPU buffers exclude allocator/driver overhead; no disk-cold performance claim or invented P7 time threshold.',
  results,
};
const save = () =>
  writeFile('bench/results/p7-geometry-performance.json', JSON.stringify(report, null, 2) + '\n');
try {
  for (let i = 0; i < 100; i++) {
    try {
      if ((await fetch(base)).ok) break;
    } catch {
      // Wait for the local preview server to accept connections.
    }
    await delay(100);
  }
  for (const channel of ['chrome', 'msedge']) {
    const browser = await chromium.launch({ channel });
    try {
      for (const backend of ['WebGPU', 'WebGL2']) {
        const context = await browser.newContext({
            viewport: { width: 1280, height: 850 },
            deviceScaleFactor: 1,
          }),
          page = await context.newPage();
        await page.goto(
          base + '/?diagnostic=1&document=1' + (backend === 'WebGL2' ? '&gpu=webgl2' : ''),
        );
        await expect(page.getByTestId('engine-status')).toHaveText('Engine ready');
        await expect(page.getByTestId('gpu-backend')).toHaveText(backend);
        await page.evaluate(() => {
          window.__geometryIdle = async (action) => {
            const deadline = performance.now() + 60000;
            for (;;) {
              try {
                return await action();
              } catch (error) {
                if (
                  !/Finish or cancel the current operation first/.test(String(error)) ||
                  performance.now() >= deadline
                )
                  throw error;
                await new Promise((r) => setTimeout(r, 50));
              }
            }
          };
        });
        const gpu = await page.evaluate(async () => {
            const a = await navigator.gpu?.requestAdapter();
            return a
              ? {
                  vendor: a.info.vendor,
                  architecture: a.info.architecture,
                  isFallbackAdapter: a.info.isFallbackAdapter,
                }
              : null;
          }),
          adapter = await page.getByTestId('gpu-backend').getAttribute('title');
        if (
          !gpu?.vendor ||
          gpu.isFallbackAdapter ||
          /swiftshader|warp|software|llvmpipe|basic render/i.test(adapter ?? '') ||
          ['google', 'microsoft'].includes(gpu.vendor)
        )
          throw Error('Hardware GPU required');
        await page.getByTestId('open-image').setInputFiles(fixture);
        await expect(page.getByTestId('operation-status')).toHaveText(/Opened in/);
        const result = {
          channel,
          backend,
          browser: browser.version(),
          adapter,
          gpu,
          apply: [],
          preview: [],
        };
        results.push(result);
        await save();
        for (const scenario of [
          'image-half-lanczos3',
          'layer-move-exact',
          'layer-rotate-10-lanczos3',
          'selected-move-exact',
        ]) {
          const group = { scenario, trials: [] };
          result.apply.push(group);
          for (let trial = 0; trial < 4; trial++) {
            console.log(JSON.stringify({ channel, backend, scenario, trial, status: 'starting' }));
            const measured = await page.evaluate(
              async ({ scenario, trial }) => {
                const api = window.__fotoeditor,
                  canvas = document.querySelector('[data-testid="gpu-canvas"]');
                await window.__geometryIdle(() => api.historyJump(0));
                let source = JSON.parse(canvas.dataset.snapshot ?? 'null');
                for (let i = 0; i < 240 && source?.document?.width !== 6000; i++) {
                  await new Promise((r) => requestAnimationFrame(r));
                  source = JSON.parse(canvas.dataset.snapshot ?? 'null');
                }
                if (source.document.width !== 6000 || source.document.height !== 4000)
                  throw Error('24 MP source required');
                if (scenario === 'selected-move-exact')
                  await window.__geometryIdle(() =>
                    api.selectionAction({
                      kind: 'shape',
                      shape: { kind: 'rectangle', x0: 1500, y0: 1000, x1: 4500, y1: 3000 },
                      mode: 'new',
                      antialias: false,
                    }),
                  );
                const target = source.layers.active ?? source.layers.nodes[0].id;
                const a = (10 * Math.PI) / 180,
                  c = Math.cos(a),
                  s = Math.sin(a);
                const matrix =
                  scenario === 'layer-rotate-10-lanczos3'
                    ? {
                        values: [
                          c,
                          -s,
                          3000 - 3000 * c + 2000 * s,
                          s,
                          c,
                          2000 - 3000 * s - 2000 * c,
                          0,
                          0,
                          1,
                        ],
                      }
                    : { values: [1, 0, 20, 0, 1, 12, 0, 0, 1] };
                const action =
                  scenario === 'image-half-lanczos3'
                    ? {
                        kind: 'image-size',
                        width: 3000,
                        height: 2000,
                        dpi: source.document.dpi,
                        interpolation: 'lanczos3',
                      }
                    : {
                        kind: 'layer-transform',
                        target,
                        matrix,
                        interpolation: 'lanczos3',
                        selection: scenario === 'selected-move-exact',
                      };
                const samples = [],
                  timer = setInterval(() => {
                    const s = JSON.parse(canvas.dataset.snapshot ?? 'null');
                    if (s) samples.push(s.memory);
                  }, 2),
                  start = performance.now();
                try {
                  const applied = await window.__geometryIdle(() => api.geometryAction(action)),
                    returnMs = performance.now() - start;
                  samples.push(applied.memory);
                  let visible = false;
                  for (let i = 0; i < 600; i++) {
                    const s = JSON.parse(canvas.dataset.snapshot ?? 'null');
                    if (s?.contentRevision === applied.contentRevision && s.visibleTiles > 0) {
                      visible = true;
                      break;
                    }
                    await new Promise((r) => requestAnimationFrame(r));
                  }
                  if (!visible) throw Error('Committed geometry was not presented');
                  const visibleMs = performance.now() - start,
                    categoryMaxima = { document: 0, history: 0, temporary: 0, gpu: 0 };
                  let sampledCpuPeak = 0;
                  for (const s of samples) {
                    for (const k of Object.keys(categoryMaxima))
                      categoryMaxima[k] = Math.max(categoryMaxima[k], s[k]);
                    sampledCpuPeak = Math.max(sampledCpuPeak, s.document + s.history + s.temporary);
                  }
                  if (sampledCpuPeak > 768 * 1048576 || categoryMaxima.gpu > 128 * 1048576)
                    throw Error('Residency budget exceeded');
                  if (
                    applied.document.width !== (scenario === 'image-half-lanczos3' ? 3000 : 6000) ||
                    applied.document.height !== (scenario === 'image-half-lanczos3' ? 2000 : 4000)
                  )
                    throw Error('Unexpected visible dimensions');
                  return {
                    trial,
                    returnMs,
                    visibleMs,
                    ...applied.geometryTiming,
                    categoryMaxima,
                    sampledCpuPeak,
                    output: applied.document,
                  };
                } finally {
                  clearInterval(timer);
                }
              },
              { scenario, trial },
            );
            group.trials.push(measured);
            group.warmReturnMs = percentile(group.trials.slice(1).map((t) => t.returnMs));
            group.warmVisibleMs = percentile(group.trials.slice(1).map((t) => t.visibleMs));
            await save();
            console.log(JSON.stringify({ channel, backend, scenario, ...measured }));
          }
        }
        await page.evaluate(async () => {
          const api = window.__fotoeditor;
          await window.__geometryIdle(() => api.historyJump(0));
          await window.__geometryIdle(() => api.selectionAction({ kind: 'none' }));
        });
        await expect
          .poll(() =>
            page
              .getByTestId('gpu-canvas')
              .evaluate((c) => JSON.parse(c.dataset.snapshot).document.width),
          )
          .toBe(6000);
        const prepared = await page.evaluate(async () => {
          const api = window.__fotoeditor,
            c = document.querySelector('[data-testid="gpu-canvas"]'),
            s = JSON.parse(c.dataset.snapshot);
          const start = performance.now();
          await window.__geometryIdle(() =>
            api.beginTransform(s.layers.active ?? s.layers.nodes[0].id, false),
          );
          const beginMs = performance.now() - start;
          return {
            beginMs,
            first: await window.__geometryIdle(() =>
              api.previewTransform({
                kind: 'matrix',
                matrix: { values: [1, 0, 0, 0, 1, 0, 0, 0, 1] },
              }),
            ),
          };
        });
        result.preparation = prepared;
        await save();
        for (const interpolation of ['nearest', 'bilinear', 'bicubic', 'lanczos3']) {
          await page.evaluate(
            (i) =>
              window.__geometryIdle(() =>
                window.__fotoeditor.previewTransform({ kind: 'interpolation', interpolation: i }),
              ),
            interpolation,
          );
          const trials = await page.evaluate(async () => {
            const api = window.__fotoeditor,
              out = [];
            for (let i = 0; i < 20; i++) {
              const a = (((i % 7) - 3) * Math.PI) / 180,
                c = Math.cos(a),
                s = Math.sin(a),
                start = performance.now();
              const timing = await window.__geometryIdle(() =>
                api.previewTransform({
                  kind: 'matrix',
                  matrix: {
                    values: [
                      c,
                      -s,
                      3000 - 3000 * c + 2000 * s,
                      s,
                      c,
                      2000 - 3000 * s - 2000 * c,
                      0,
                      0,
                      1,
                    ],
                  },
                }),
              );
              const memory = JSON.parse(
                document.querySelector('[data-testid="gpu-canvas"]').dataset.snapshot,
              ).memory;
              if (
                memory.document + memory.history + memory.temporary > 768 * 1048576 ||
                memory.gpu > 128 * 1048576
              )
                throw Error('Preview residency exceeded');
              out.push({ event: i, totalMs: performance.now() - start, ...timing, memory });
            }
            return out;
          });
          result.preview.push({
            interpolation,
            trials,
            totalMs: percentile(trials.map((t) => t.totalMs)),
            updateMs: percentile(trials.map((t) => t.updateMs)),
          });
          await save();
          console.log(
            JSON.stringify({
              channel,
              backend,
              interpolation,
              preview: result.preview.at(-1).totalMs,
            }),
          );
        }
        await page.evaluate(() =>
          window.__geometryIdle(() => window.__fotoeditor.cancelTransform()),
        );
        await context.close();
      }
    } finally {
      await browser.close();
    }
  }
} finally {
  server?.kill();
  await save();
}
