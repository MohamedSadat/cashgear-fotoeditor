import { chromium, expect } from '@playwright/test';
import { spawn } from 'node:child_process';
import { readFile, writeFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { createHash } from 'node:crypto';
import os from 'node:os';
import { setTimeout as delay } from 'node:timers/promises';
const phase = process.env.PERF_PHASE ?? 'p6-m4';
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
        await page.getByRole('button', { name: 'Show adjustments section' }).click();
        const controls = [];
        for (const scenario of [
          { kind: 'exposure', field: 'stops', label: 'Exposure (stops) slider', step: 0.1 },
          { kind: 'hue-saturation', field: 'hue', label: 'Hue (degrees) slider', step: 7 },
          { kind: 'vibrance', field: 'amount', label: 'Vibrance (%) slider', step: 4, scale: 100 },
          {
            kind: 'color-balance',
            field: 'midtones.0',
            label: 'midtones Cyan / Red (%) slider',
            step: 2,
            scale: 100,
          },
          {
            kind: 'black-white',
            field: 'weights.0',
            label: 'Reds (%) slider',
            step: 3,
            scale: 100,
          },
          { kind: 'threshold', field: 'level', label: 'Threshold level slider', step: 10 },
          {
            kind: 'posterize',
            field: 'levels',
            label: 'Posterize levels slider',
            step: 1,
            offset: 2,
          },
          {
            kind: 'gradient-map',
            field: 'stops.0.1',
            label: 'Gradient red 1',
            step: 4,
            scale: 100,
          },
        ]) {
          await page.getByRole('checkbox', { name: 'Live viewport preview' }).uncheck();
          await page.getByLabel('Adjustment', { exact: true }).selectOption(scenario.kind);
          await page.getByRole('checkbox', { name: 'Live viewport preview' }).check();
          await expect(page.getByTestId('adjustment-preview-status')).toContainText(
            'Preview ready',
          );
          const preparationMs = Number(
            await page.getByTestId('adjustment-preview-status').getAttribute('data-preparation-ms'),
          );
          const initialUpdateMs = Number(
            await page.getByTestId('adjustment-preview-status').getAttribute('data-update-ms'),
          );
          const trials = [];
          for (let i = 1; i <= 20; i++)
            trials.push(
              await page.evaluate(
                async ({ scenario, value }) => {
                  const status = document.querySelector(
                    '[data-testid="adjustment-preview-status"]',
                  );
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
                        !status.textContent.includes('Preview ready')
                      )
                        return;
                      const settings = JSON.parse(status.dataset.settings ?? 'null');
                      if (
                        settings?.kind !== scenario.kind ||
                        Math.abs(
                          scenario.field.split('.').reduce((v, k) => v?.[k], settings) -
                            value / (scenario.scale ?? 1),
                        ) > 1e-8
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
                { scenario, value: i * scenario.step + (scenario.offset ?? 0) },
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
          path: `bench/results/${phase}-${channel}-${backend.toLowerCase()}-adjustment-preview.png`,
        });
        const api = await page.evaluate(async () => {
          const api = Reflect.get(window, '__fotoeditor');
          const canvas = document.querySelector('[data-testid="gpu-canvas"]');
          const state = JSON.parse(canvas.dataset.snapshot);
          if (state.document.width !== 6000 || state.document.height !== 4000)
            throw new Error('24 MP source required');
          const target = {
            kind: 'pixels',
            target: state.layers.active ?? state.layers.nodes[0].id,
          };
          const configurations = [
            { kind: 'brightness-contrast', brightness: 0.17, contrast: -0.3 },
            {
              kind: 'levels',
              black: 0.12,
              white: 0.85,
              gamma: 1.7,
              outputBlack: 0.04,
              outputWhite: 0.94,
            },
            {
              kind: 'curves',
              points: [
                [0, 0.9],
                [0.3, 0.2],
                [0.8, 0.7],
                [1, 0.1],
              ],
            },
            { kind: 'exposure', stops: 0.75, offset: 0, gamma: 1 },
            { kind: 'hue-saturation', hue: 20, saturation: 0.15, lightness: 0 },
            { kind: 'vibrance', amount: 0.7 },
            {
              kind: 'color-balance',
              shadows: [-0.3, 0.1, 0.4],
              midtones: [0.2, -0.4, 0.1],
              highlights: [0.3, 0.1, -0.2],
              preserveLuminosity: true,
            },
            { kind: 'black-white', weights: [0.4, 0.8, 0.3, 0.7, -0.2, 1.1] },
            { kind: 'invert' },
            { kind: 'threshold', level: 128 },
            { kind: 'posterize', levels: 4 },
            {
              kind: 'gradient-map',
              stops: [
                [0, 0.2, 0, 0.4],
                [0.37, 1, 0.1, 0.2],
                [1, 0.8, 1, 0.2],
              ],
            },
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
                const timing = await api.previewAdjustment(settings, target);
                trials.push({ ...timing, apiMs: performance.now() - started });
              }
              scenarios.push({ settings, trials });
            }
          } finally {
            clearInterval(timer);
            await api.endAdjustmentPreview();
          }
          return { scenarios, memories, history: JSON.parse(canvas.dataset.snapshot).history };
        });
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
  const wasm = (await readdir(assets)).find((p) => p.endsWith('.wasm'));
  await writeFile(
    process.env.PERF_OUTPUT ?? `bench/results/${phase}-adjustment-preview-performance.json`,
    JSON.stringify(
      {
        measuredAt: new Date().toISOString(),
        machine: { os: os.version(), cpu: os.cpus()[0].model, ram: os.totalmem() },
        viewport: { width: 1280, height: 850, dpr: 1 },
        wasm: { file: wasm, sha256: hash(await readFile(resolve(assets, wasm))) },
        dataset: {
          path: fixture,
          sha256: hash(await readFile(fixture)),
          source: 'https://science.nasa.gov/resource/setting-the-wind-and-thermal-shield/',
          credit: 'NASA/JPL-Caltech/IPGP',
        },
        method:
          'Hardware only. Resident 6000×4000 source, fitted view. Eight representative controls run first (including gradient stop numeric input): initial preparation and first render/GPU initialization are recorded separately, then 20 DOM slider input events per control are measured through completed React status and the following UI frame. Twelve settings then each have one first and ten warm API/present trials, reusing the visible source cache. Rust nearest-samples the visible source at bounded display resolution; final Apply uses full document resolution. Logical residency excludes allocator/browser/driver overhead.',
        results,
      },
      null,
      2,
    ) + '\n',
  );
}
