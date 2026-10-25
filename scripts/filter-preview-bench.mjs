import { chromium, expect } from '@playwright/test';
import { spawn } from 'node:child_process';
import { readFile, writeFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { createHash } from 'node:crypto';
import os from 'node:os';
import { setTimeout as delay } from 'node:timers/promises';
const viewMode = process.env.PERF_VIEW ?? 'fit';
if (!['fit', 'actual'].includes(viewMode)) throw new Error('PERF_VIEW must be fit or actual');
const actualPixels = viewMode === 'actual';
const base = process.env.PERF_BASE_URL ?? 'http://127.0.0.1:4188';
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
const fixture = resolve('bench/results/fixtures/p5-photo.png');
const results = [];
const hash = (bytes) => createHash('sha256').update(bytes).digest('hex');
function summary(values) {
  const s = [...values].sort((a, b) => a - b);
  return {
    p50: s[Math.ceil(s.length * 0.5) - 1],
    p95: s[Math.ceil(s.length * 0.95) - 1],
    p99: s[Math.ceil(s.length * 0.99) - 1],
    max: s.at(-1),
  };
}
try {
  for (let i = 0; i < 100; i++) {
    try {
      if ((await fetch(base)).ok) break;
    } catch {
      /* Preview server is starting. */
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
        if (actualPixels) {
          await page.getByRole('button', { name: 'Actual pixels', exact: true }).click();
          await expect(page.getByTestId('zoom')).toHaveText('100.0%');
          await expect
            .poll(() =>
              page.getByTestId('gpu-canvas').evaluate((canvas) => {
                const s = JSON.parse(canvas.dataset.snapshot);
                return s.view.zoom === 1 && s.refining === 0 && s.visibleTiles > 0;
              }),
            )
            .toBe(true);
        }
        await page.getByRole('button', { name: 'Show filters section' }).click();
        const controls = [];
        for (const scenario of [
          { kind: 'gaussian', field: 'radius', label: 'Filter radius (px) slider', step: 1 },
          { kind: 'box-blur', field: 'radius', label: 'Filter radius (px) slider', step: 1 },
          { kind: 'unsharp-mask', field: 'amount', label: 'Sharpen amount slider', step: 0.1 },
          { kind: 'sharpen', field: 'amount', label: 'Sharpen amount slider', step: 0.1 },
          { kind: 'median', field: 'radius', label: 'Filter radius (px) slider', step: 1 },
        ]) {
          await page.getByRole('checkbox', { name: 'Live filter preview' }).uncheck();
          await page.getByLabel('Filter', { exact: true }).selectOption(scenario.kind);
          await page.getByRole('checkbox', { name: 'Live filter preview' }).check();
          await expect(page.getByTestId('filter-preview-status')).toContainText(
            'Filter preview ready',
          );
          const preparationMs = Number(
            await page.getByTestId('filter-preview-status').getAttribute('data-preparation-ms'),
          );
          const initialUpdateMs = Number(
            await page.getByTestId('filter-preview-status').getAttribute('data-update-ms'),
          );
          const trials = [];
          for (let i = 1; i <= 20; i++)
            trials.push(
              await page.evaluate(
                async ({ scenario, value }) => {
                  const status = document.querySelector('[data-testid="filter-preview-status"]');
                  const input = document.querySelector('[aria-label="' + scenario.label + '"]');
                  const prior = status.dataset.serial;
                  const started = performance.now();
                  return new Promise((resolve, reject) => {
                    const timeout = setTimeout(() => {
                      observer.disconnect();
                      reject(new Error('Slider preview did not present'));
                    }, 10000);
                    const observer = new MutationObserver(() => {
                      if (
                        status.dataset.serial === prior ||
                        !status.textContent.includes('Filter preview ready')
                      )
                        return;
                      const settings = JSON.parse(status.dataset.settings ?? 'null');
                      if (
                        settings?.kind !== scenario.kind ||
                        Math.abs(settings[scenario.field] - value) > 1e-8
                      )
                        return;
                      observer.disconnect();
                      clearTimeout(timeout);
                      requestAnimationFrame(() =>
                        resolve({
                          eventToUiFrameMs: performance.now() - started,
                          workerPresentedMs: Number(status.dataset.updateMs),
                          renderSubmitMs: Number(status.dataset.submitMs),
                          frameWaitMs: Number(status.dataset.frameWaitMs),
                          preparationMs: Number(status.dataset.preparationMs),
                          settings,
                          memory: JSON.parse(
                            document.querySelector('[data-testid="gpu-canvas"]').dataset.snapshot,
                          ).memory,
                        }),
                      );
                    });
                    observer.observe(status, {
                      attributes: true,
                      childList: true,
                      subtree: true,
                      characterData: true,
                    });
                    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set.call(
                      input,
                      String(value),
                    );
                    input.dispatchEvent(new Event('input', { bubbles: true }));
                    input.dispatchEvent(new Event('change', { bubbles: true }));
                  });
                },
                { scenario, value: scenario.kind === 'median' ? (i % 5) + 1 : i * scenario.step },
              ),
            );
          controls.push({
            ...scenario,
            preparationMs,
            initialUpdateMs,
            trials,
            warmEventToUiFrameMs: summary(trials.map((t) => t.eventToUiFrameMs)),
            warmWorkerPresentedMs: summary(trials.map((t) => t.workerPresentedMs)),
          });
        }
        await page.screenshot({
          path: `bench/results/p6-m3-${channel}-${backend.toLowerCase()}-${actualPixels ? 'actual-' : ''}preview.png`,
        });
        await page.getByRole('checkbox', { name: 'Live filter preview' }).uncheck();
        const api = await page.evaluate(async (actualPixels) => {
          const api = Reflect.get(window, '__fotoeditor');
          await api.endAdjustmentPreview();
          const canvas = document.querySelector('[data-testid="gpu-canvas"]');
          const state = JSON.parse(canvas.dataset.snapshot);
          if (state.document.width !== 6000 || state.document.height !== 4000)
            throw new Error('24 MP source required');
          const target = state.layers.active ?? state.layers.nodes[0].id;
          const configurations = [
            { kind: 'gaussian', radius: actualPixels ? 64 : 20 },
            { kind: 'box-blur', radius: actualPixels ? 64 : 20 },
            { kind: 'unsharp-mask', radius: actualPixels ? 64 : 3, amount: 1.3, threshold: 7 },
            { kind: 'sharpen', amount: 1.2 },
            { kind: 'median', radius: 5 },
          ];
          const scenarios = [];
          const memories = [];
          const timer = setInterval(() => {
            const s = JSON.parse(canvas.dataset.snapshot ?? 'null');
            if (s) memories.push(s.memory);
          }, 2);
          try {
            for (const settings of configurations) {
              const trials = [];
              for (let i = 0; i < 11; i++) {
                const started = performance.now();
                const timing = await api.previewFilter(settings, target);
                trials.push({ ...timing, apiMs: performance.now() - started });
              }
              scenarios.push({ settings, trials });
            }
          } finally {
            clearInterval(timer);
            await api.endAdjustmentPreview();
          }
          return { scenarios, memories, history: JSON.parse(canvas.dataset.snapshot).history };
        }, actualPixels);
        for (const scenario of api.scenarios) {
          scenario.warmApiMs = summary(scenario.trials.slice(1).map((t) => t.apiMs));
          scenario.warmPresentedMs = summary(scenario.trials.slice(1).map((t) => t.updateMs));
        }
        const record = {
          channel,
          backend,
          browser: browser.version(),
          adapter,
          gpu,
          api,
          controls,
        };
        results.push(record);
        console.log(
          JSON.stringify({
            channel,
            backend,
            api: api.scenarios.map((s) => ({ kind: s.settings.kind, warm: s.warmApiMs })),
            controls: controls.map((s) => ({ kind: s.kind, warm: s.warmEventToUiFrameMs })),
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
    process.env.PERF_OUTPUT ??
      `bench/results/p6-m3-preview-${actualPixels ? 'actual-' : ''}performance.json`,
    JSON.stringify(
      {
        measuredAt: new Date().toISOString(),
        machine: { os: os.version(), cpu: os.cpus()[0].model, ram: os.totalmem() },
        viewport: { width: 1280, height: 850, dpr: 1, viewMode },
        wasm: { file: wasm, sha256: hash(await readFile(resolve(assets, wasm))) },
        worker: { file: worker, sha256: hash(await readFile(resolve(assets, worker))) },
        dataset: {
          path: fixture,
          sha256: hash(await readFile(fixture)),
          source: 'https://science.nasa.gov/resource/setting-the-wind-and-thermal-shield/',
          credit: 'NASA/JPL-Caltech/IPGP',
        },
        method:
          'Hardware only. Resident 6000×4000 source at the recorded view mode. Five filter controls run first: initial preparation and first render/GPU initialization are recorded separately, then 20 DOM slider input events per control are measured through completed React status and the following UI frame. Five filters then each have one first and ten warm API/present trials, reusing the visible source cache. Actual-pixel API cases use maximum convolution/median radii. Rust nearest-samples the visible source at bounded display resolution; final Apply uses full document resolution. Logical residency excludes allocator/browser/driver overhead.',
        results,
      },
      null,
      2,
    ) + '\n',
  );
}
