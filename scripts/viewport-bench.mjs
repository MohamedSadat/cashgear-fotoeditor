import { chromium } from '@playwright/test';
import { spawn } from 'node:child_process';
import { mkdir, writeFile, stat } from 'node:fs/promises';
import { resolve } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
const base = process.env.PERF_BASE_URL ?? 'http://127.0.0.1:4174';
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
        '4174',
        '--strictPort',
      ],
      { cwd: resolve('apps/web'), stdio: 'ignore', windowsHide: true },
    );
const percentile = (a, p) =>
  [...a].sort((a, b) => a - b)[Math.min(a.length - 1, Math.floor(a.length * p))];
const results = [];
try {
  for (let i = 0; i < 100; ++i) {
    try {
      if ((await fetch(base)).ok) break;
    } catch {
      /* Wait for the local preview server. */
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
        await page.goto(base + (backend === 'WebGL2' ? '/?gpu=webgl2' : '/'));
        await page.getByTestId('engine-status').filter({ hasText: 'Engine ready' }).waitFor();
        const gpu = await page.evaluate(async () => {
          const adapter = await navigator.gpu?.requestAdapter();
          return adapter
            ? {
                ...Object.fromEntries(
                  ['vendor', 'architecture', 'device', 'description', 'isFallbackAdapter'].map(
                    (k) => [k, adapter.info[k]],
                  ),
                ),
              }
            : null;
        });
        if (!gpu || gpu.isFallbackAdapter || ['google', 'microsoft'].includes(gpu.vendor))
          throw new Error('Hardware benchmark requires a hardware adapter: ' + JSON.stringify(gpu));
        const opens = [];
        for (let repeat = 0; repeat < 3; repeat++) {
          await page
            .getByTestId('open-image')
            .setInputFiles(resolve('bench/results/fixtures/large.png'));
          await page.getByTestId('operation-status').filter({ hasText: 'Opened in' }).waitFor();
          opens.push(
            Number(
              (await page.getByTestId('operation-status').innerText()).match(/([\d.]+) s/)[1],
            ) * 1000,
          );
        }
        const frames = {};
        for (const mode of ['pan', 'zoom', 'zoom-mips']) {
          // Warm visible atlas at 15% zoom. Zoom stays within one mip interval.
          await page.evaluate(() => {
            const canvas = document.querySelector('[data-testid="gpu-canvas"]');
            const s = JSON.parse(canvas.dataset.snapshot);
            const rect = canvas.getBoundingClientRect();
            canvas.dispatchEvent(
              new WheelEvent('wheel', {
                deltaY: -Math.log(0.15 / s.view.zoom) / 0.002,
                ctrlKey: true,
                clientX: rect.x + rect.width / 2,
                clientY: rect.y + rect.height / 2,
                bubbles: true,
                cancelable: true,
              }),
            );
          });
          await delay(600);
          frames[mode] = await page.evaluate(async (mode) => {
            const canvas = document.querySelector('[data-testid="gpu-canvas"]');
            const rect = canvas.getBoundingClientRect();
            const periods = [];
            const renderPeriods = [];
            const counts = [];
            let begin = 0;
            let last = 0;
            let previousCount = Number(canvas.dataset.frames);
            let previousPresent = 0;
            let initialCount = previousCount;
            await new Promise((resolve) => {
              const tick = (now) => {
                if (!begin) {
                  begin = now;
                  previousPresent = now;
                  last = now;
                  initialCount = Number(canvas.dataset.frames);
                } else periods.push(now - last);
                const count = Number(canvas.dataset.frames);
                if (count !== previousCount) {
                  renderPeriods.push(now - previousPresent);
                  counts.push(count - previousCount);
                  previousPresent = now;
                  previousCount = count;
                }
                last = now;
                canvas.dispatchEvent(
                  new WheelEvent('wheel', {
                    deltaX: mode === 'pan' ? Math.cos((now - begin) / 400) * 3 : 0,
                    deltaY:
                      mode === 'zoom'
                        ? Math.cos((now - begin) / 400) * 1.5
                        : mode === 'zoom-mips'
                          ? Math.cos((now - begin) / 600) * 26.4
                          : 0,
                    ctrlKey: mode !== 'pan',
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
              elapsedMs: last - begin,
              presented: Number(canvas.dataset.frames) - initialCount,
              periods,
              renderPeriods,
              counts,
              snapshot: JSON.parse(canvas.dataset.snapshot),
            };
          }, mode);
          const f = frames[mode];
          frames[mode] = {
            fps: (f.presented * 1000) / f.elapsedMs,
            elapsedMs: f.elapsedMs,
            presented: f.presented,
            rafP50: percentile(f.periods, 0.5),
            rafP95: percentile(f.periods, 0.95),
            rafP99: percentile(f.periods, 0.99),
            presentedPeriodP50: percentile(f.renderPeriods, 0.5),
            presentedPeriodP95: percentile(f.renderPeriods, 0.95),
            presentedPeriodP99: percentile(f.renderPeriods, 0.99),
            snapshot: f.snapshot,
          };
        }
        const refineStart = performance.now();
        await page.waitForFunction(
          () =>
            JSON.parse(document.querySelector('[data-testid="gpu-canvas"]').dataset.snapshot)
              .refining === 0,
        );
        const refinementAfterStopMs = performance.now() - refineStart;
        const result = {
          refinementAfterStopMs,
          channel,
          backend,
          browser: browser.version(),
          adapter: await page.getByTestId('gpu-backend').getAttribute('title'),
          gpu,
          viewport: { width: 1280, height: 850, dpr: 1 },
          dataset: {
            width: 6000,
            height: 4000,
            bytes: (await stat('bench/results/fixtures/large.png')).size,
            generator: 'fotoeditor-io/examples/fixtures.rs',
          },
          opensMs: opens,
          frames,
        };
        results.push(result);
        console.log(JSON.stringify(result));
        await page.screenshot({ path: `bench/results/p1-${channel}-${backend}.png` });
        await context.close();
      }
    } finally {
      await browser.close();
    }
  }
  await mkdir('bench/results', { recursive: true });
  await writeFile(
    'bench/results/p1-performance.json',
    JSON.stringify({ measuredAt: new Date().toISOString(), results }, null, 2) + '\n',
  );
} finally {
  if (server) server.kill();
}
