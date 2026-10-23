import { chromium } from '@playwright/test';
import { spawn } from 'node:child_process';
import { createReadStream } from 'node:fs';
import { writeFile, stat, mkdtemp, rm } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { resolve, join } from 'node:path';
import { tmpdir } from 'node:os';
import { setTimeout as delay } from 'node:timers/promises';
const file = resolve('bench/results/fixtures/layers-50.cgfoto');
const hash = createHash('sha256');
for await (const bytes of createReadStream(file)) hash.update(bytes);
const dataset = {
  width: 4000,
  height: 3000,
  layers: 50,
  tiles: 9600,
  bytes: (await stat(file)).size,
  sha256: hash.digest('hex'),
  generator: 'crates/fotoeditor-io/examples/layers.rs',
  seed: '0x9e3779b9',
};
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
const percentile = (a, p) =>
  [...a].sort((a, b) => a - b)[Math.min(a.length - 1, Math.floor(a.length * p))];
const results = [];
async function removeProfile(profile) {
  if (!resolve(profile).startsWith(join(resolve(tmpdir()), 'fotoeditor-p4-layers-')))
    throw new Error('Unexpected benchmark profile path');
  await rm(profile, { recursive: true, force: true });
}
try {
  for (let i = 0; i < 100; i++) {
    try {
      if ((await fetch(base)).ok) break;
    } catch {
      /* Server startup. */
    }
    await delay(100);
  }
  for (const channel of process.env.PERF_CHANNEL
    ? [process.env.PERF_CHANNEL]
    : ['chrome', 'msedge']) {
    const profiles = [];
    const contexts = [];
    try {
      for (const backend of process.env.PERF_BACKEND
        ? [process.env.PERF_BACKEND]
        : ['WebGPU', 'WebGL2']) {
        const profile = await mkdtemp(join(tmpdir(), 'fotoeditor-p4-layers-'));
        profiles.push(profile);
        const context = await chromium.launchPersistentContext(profile, {
          channel,
          headless: true,
          viewport: { width: 1280, height: 850 },
          deviceScaleFactor: 1,
        });
        contexts.push(context);
        const browser = context.browser();
        const page = await context.newPage();
        page.setDefaultTimeout(360000);
        const errors = [];
        page.on('pageerror', (error) => errors.push(String(error)));
        const progress = setInterval(async () => {
          try {
            console.log(
              JSON.stringify({
                progress: await page.evaluate(() => ({
                  status: document.querySelector('[data-testid="engine-status"]')?.textContent,
                  operation: document.querySelector('[data-testid="operation-status"]')
                    ?.textContent,
                  thumbnails: document.querySelectorAll('.layer-thumbnail').length,
                  snapshot: (() => {
                    const s = JSON.parse(
                      document.querySelector('[data-testid="gpu-canvas"]')?.dataset.snapshot ??
                        '{}',
                    );
                    return {
                      document: s.document,
                      memory: s.memory,
                      remaining: s.layers?.compositing,
                      operation: s.operation,
                      recovery: s.recovery?.state,
                      projectTiming: s.projectTiming,
                    };
                  })(),
                })),
              }),
            );
          } catch {
            /* Context closed. */
          }
        }, 10000);
        progress.unref();
        await page.goto(
          base + '/?diagnostic=1&document=1' + (backend === 'WebGL2' ? '&gpu=webgl2' : ''),
        );
        await page.getByTestId('engine-status').filter({ hasText: 'Engine ready' }).waitFor();
        const gpu = await page.evaluate(async () => {
          const a = await navigator.gpu?.requestAdapter();
          return a
            ? {
                ...a.info,
                isFallbackAdapter: a.info.isFallbackAdapter,
                vendor: a.info.vendor,
                architecture: a.info.architecture,
              }
            : null;
        });
        const adapter = await page.getByTestId('gpu-backend').getAttribute('title');
        if (
          !gpu ||
          gpu.isFallbackAdapter ||
          ['google', 'microsoft'].includes(gpu.vendor) ||
          /swiftshader|warp|software|llvmpipe|basic render/i.test(adapter ?? '')
        )
          throw new Error('Hardware adapter required: ' + JSON.stringify({ gpu, adapter }));
        await page.evaluate(() => {
          window.__p4peak = { document: 0, history: 0, temporary: 0, gpu: 0 };
          window.__p4watch = true;
          const tick = () => {
            const value = document.querySelector('[data-testid="gpu-canvas"]')?.dataset.snapshot;
            if (value) {
              const s = JSON.parse(value);
              for (const key of Object.keys(window.__p4peak))
                window.__p4peak[key] = Math.max(window.__p4peak[key], s.memory[key]);
            }
            if (window.__p4watch) requestAnimationFrame(tick);
          };
          requestAnimationFrame(tick);
        });
        const storageBefore = await page.evaluate(() => navigator.storage.estimate());
        const started = performance.now();
        await page.getByTestId('open-image').setInputFiles(file);
        await page.getByTestId('operation-status').filter({ hasText: 'Opened in' }).waitFor();
        const openAndComposeMs = performance.now() - started;
        console.log(JSON.stringify({ channel, backend, openAndComposeMs }));
        await page.waitForFunction(
          () =>
            JSON.parse(document.querySelector('[data-testid="gpu-canvas"]').dataset.snapshot)
              .refining === 0,
          null,
          { timeout: 360000 },
        );
        const firstRefinementMs = performance.now() - started;
        await page.waitForFunction(
          () => document.querySelectorAll('.layer-thumbnail').length === 50,
          null,
          { timeout: 360000 },
        );
        const allThumbnailsMs = performance.now() - started;
        const normalView = await page.evaluate(() =>
          JSON.parse(document.querySelector('[data-testid="gpu-canvas"]').dataset.snapshot),
        );
        if (
          normalView.visibleTiles === 0 ||
          normalView.uploadedBytes === 0 ||
          normalView.document.tiles !== 9600
        )
          throw new Error('The actual dense composite was not presented');
        clearInterval(progress);
        const autosaveStart = performance.now();
        await page.evaluate(async () => Reflect.get(window, '__fotoeditor').autosave());
        await page.waitForFunction(
          () => {
            const s = JSON.parse(
              document.querySelector('[data-testid="gpu-canvas"]').dataset.snapshot,
            );
            return s.recovery.state !== 'saving' && !s.operation;
          },
          null,
          { timeout: 360000 },
        );
        const recoveryState = await page.evaluate(
          () =>
            JSON.parse(document.querySelector('[data-testid="gpu-canvas"]').dataset.snapshot)
              .recovery,
        );
        if (recoveryState.state !== 'saved')
          throw new Error('Dense recovery did not finish: ' + JSON.stringify(recoveryState));
        // The timer may already have saved while thumbnails were rendering.
        // Actual recovery duration is snapshot.recovery.lastSaveMs; this only
        // measures synchronization with an idle, completed recovery generation.
        const autosaveSynchronizationMs = performance.now() - autosaveStart;
        // Let file and thumbnail jobs finish before measuring the warm cache.
        await delay(1000);
        const trials = [];
        for (let trial = 0; trial < 3; trial++) {
          const value = await page.evaluate(async () => {
            const canvas = document.querySelector('[data-testid="gpu-canvas"]');
            const rect = canvas.getBoundingClientRect();
            const intervals = [];
            let begin = 0,
              last = 0,
              previous = Number(canvas.dataset.frames),
              initial = previous;
            await new Promise((resolve) => {
              const tick = (now) => {
                if (!begin) {
                  begin = now;
                  last = now;
                  initial = Number(canvas.dataset.frames);
                }
                const count = Number(canvas.dataset.frames);
                if (count !== previous) {
                  intervals.push(now - last);
                  last = now;
                  previous = count;
                }
                canvas.dispatchEvent(
                  new WheelEvent('wheel', {
                    deltaX: Math.cos((now - begin) / 400) * 3,
                    deltaY: Math.sin((now - begin) / 500),
                    clientX: rect.x + rect.width / 2,
                    clientY: rect.y + rect.height / 2,
                    bubbles: true,
                    cancelable: true,
                  }),
                );
                if (now - begin < 4000) requestAnimationFrame(tick);
                else resolve();
              };
              requestAnimationFrame(tick);
            });
            return {
              elapsedMs: performance.now() - begin,
              presented: Number(canvas.dataset.frames) - initial,
              intervals,
              snapshot: JSON.parse(canvas.dataset.snapshot),
            };
          });
          trials.push({
            fps: (value.presented * 1000) / value.elapsedMs,
            presented: value.presented,
            elapsedMs: value.elapsedMs,
            p50: percentile(value.intervals, 0.5),
            p95: percentile(value.intervals, 0.95),
            p99: percentile(value.intervals, 0.99),
            snapshot: value.snapshot,
          });
        }
        const peak = await page.evaluate(() => {
          window.__p4watch = false;
          return window.__p4peak;
        });
        const storageAfter = await page.evaluate(() => navigator.storage.estimate());
        const result = {
          channel,
          backend,
          browser: browser.version(),
          gpu,
          adapter,
          dataset,
          viewport: { width: 1280, height: 850, dpr: 1 },
          profileMode: 'fresh persistent profile; OS file cache not flushed',
          openAndComposeMs,
          firstRefinementMs,
          allThumbnailsMs,
          autosaveSynchronizationMs,
          peak,
          storageBefore,
          storageAfter,
          trials,
          errors,
        };
        results.push(result);
        console.log(
          JSON.stringify({
            ...result,
            trials: trials.map(({ fps, p50, p95, p99 }) => ({ fps, p50, p95, p99 })),
          }),
        );
        await writeFile(
          'bench/results/p4-layers-performance.json',
          JSON.stringify({ measuredAt: new Date().toISOString(), results }, null, 2) + '\n',
        );
        if (
          errors.length ||
          peak.document + peak.history + peak.temporary > 768 * 1048576 ||
          peak.gpu > 128 * 1048576
        )
          throw new Error('Workload error or residency budget exceeded');
        await context.close();
      }
    } finally {
      for (const context of contexts) await context.close().catch(() => {});
      for (const profile of profiles) {
        await removeProfile(profile);
      }
    }
  }
} finally {
  server?.kill();
}
