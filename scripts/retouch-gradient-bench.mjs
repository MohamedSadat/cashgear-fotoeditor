import { chromium, expect } from '@playwright/test';
import { spawn, execFileSync } from 'node:child_process';
import { readFile, readdir, writeFile, mkdir } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { resolve } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import os from 'node:os';
const base = process.env.PERF_BASE_URL ?? 'http://127.0.0.1:4188';
const out = process.env.RETOUCH_REPORT ?? 'bench/results/p9-m4-performance.json';
const hash = (b) => createHash('sha256').update(b).digest('hex');
const fixturePath = 'bench/results/fixtures/retouch-photo-6000x4000.png';
const fixture = await readFile(fixturePath);
const p = (a, q) =>
  a.length ? [...a].sort((x, y) => x - y)[Math.min(a.length - 1, Math.floor(a.length * q))] : null;
const summary = (a) => ({ count: a.length, p50: p(a, 0.5), p95: p(a, 0.95), p99: p(a, 0.99) });
const report = {
  measuredAt: new Date().toISOString(),
  head: execFileSync('git', ['rev-parse', 'HEAD'], { encoding: 'utf8', windowsHide: true }).trim(),
  dirty: Boolean(
    execFileSync('git', ['status', '--porcelain'], { encoding: 'utf8', windowsHide: true }).trim(),
  ),
  machine: { os: os.version(), cpu: os.cpus()[0].model, ram: os.totalmem() },
  fixture: {
    file: fixturePath,
    source: 'tests/fixtures/retouch/melissa-diaz-usaf.jpg',
    sourceSha256: hash(await readFile('tests/fixtures/retouch/melissa-diaz-usaf.jpg')),
    preparation:
      'Rust image 0.25.10; crop [0,600,1905,1270], Lanczos3 resize to 6000x4000 without aspect distortion.',
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
    'Serial installed hardware Chrome/Edge, actual non-fallback WebGPU/WebGL2; normal populated 6000x4000 document at 100%, 1280x850 DPR1. Fresh context per browser/API, first plus three warm calls per tool/shape; shared kernels/OS/file caches may be warm. Current raster, no selection/mask/crop/locks. Retouch: 200px soft round, flow/opacity1, spacing0.1, strength0.5, pressure size enabled with mouse pressure1, stabilizer off; native Playwright mouse path 240px in 12 moves, requested 20ms between moves, actual delivery recorded. API-to-observed-commit includes input/worker/presentation/persistence. Snapshot observer measures presentation gaps and distinct changed/completed input acknowledgements, not per-pixel throughput or a 60fps claim. Retouch CPU/GPU peaks are observed frame snapshots, not exhaustive intra-step peaks; conservative reservation bounds and unobserved COW work are reported separately. Gradients: full canvas, endpoints [2900.5,2000.5]..[3100.5,2000.5], three RGBA stops, opacity0.8; worker probes cover cooperative slices/before commit/composition, plus at most 2.25MiB separately admitted synchronous compositor scratch. Exact exported SHA-256 before/after all four undo-restored calls. No builds/tests run during timing; shared machine, no dedicated-host claim. No M4 speed threshold exists.',
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
async function idle(page, count) {
  await page.waitForFunction((count) => {
    const s = JSON.parse(document.querySelector('[data-testid="gpu-canvas"]').dataset.snapshot);
    return (
      s &&
      !s.painting &&
      !s.operation &&
      !s.selection.busy &&
      s.refining === 0 &&
      (count === null || s.history.undo === count)
    );
  }, count ?? null);
}
async function digest(page) {
  return page.evaluate(async () =>
    Array.from(
      new Uint8Array(await crypto.subtle.digest('SHA-256', await window.__fotoeditor.exportPng())),
    )
      .map((v) => v.toString(16).padStart(2, '0'))
      .join(''),
  );
}
async function range(page, name, value) {
  await page.getByRole('slider', { name, exact: true }).evaluate((e, v) => {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set.call(e, String(v));
    e.dispatchEvent(new Event('input', { bubbles: true }));
    e.dispatchEvent(new Event('change', { bubbles: true }));
  }, value);
}
try {
  for (let i = 0; i < 100; i++) {
    try {
      if ((await fetch(base)).ok) break;
    } catch {
      /* Wait for the preview server to start. */
    }
    await delay(100);
  }
  for (const channel of (process.env.PERF_CHANNELS ?? 'chrome,msedge').split(',')) {
    const browser = await chromium.launch({ channel });
    try {
      for (const backend of (process.env.PERF_BACKENDS ?? 'WebGPU,WebGL2').split(',')) {
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
          await expect(page.getByTestId('gpu-backend')).toHaveText(backend);
          const gpu = await page.evaluate(async () => {
            const a = await navigator.gpu?.requestAdapter();
            return a
              ? {
                  vendor: a.info.vendor,
                  architecture: a.info.architecture,
                  fallback: a.info.isFallbackAdapter,
                }
              : null;
          });
          const adapter = await page.getByTestId('gpu-backend').getAttribute('title');
          if (
            !gpu ||
            gpu.fallback ||
            ['google', 'microsoft'].includes(gpu.vendor) ||
            /swiftshader|warp|software|llvmpipe|basic render/i.test(adapter ?? '') ||
            (backend === 'WebGL2' && !adapter)
          )
            throw Error('Hardware required: ' + JSON.stringify({ gpu, adapter }));
          await page.getByTestId('open-image').setInputFiles(resolve(fixturePath));
          await expect(page.getByTestId('operation-status')).toHaveText(/Opened in/);
          await page.getByRole('button', { name: 'Actual pixels', exact: true }).click();
          await idle(page);
          await page.waitForFunction(() => {
            const s = JSON.parse(
              document.querySelector('[data-testid="gpu-canvas"]').dataset.snapshot,
            );
            return s.document.width === 6000 && s.document.height === 4000 && s.visibleTiles > 0;
          });
          for (const tool of (
            process.env.RETOUCH_TOOLS ??
            'smudge,blur,sharpen,dodge,burn,sponge,linear,radial,angle,reflected,diamond'
          ).split(',')) {
            const gradient = ['linear', 'radial', 'angle', 'reflected', 'diamond'].includes(tool);
            if (!gradient) {
              await page.getByRole('combobox', { name: 'Retouch tool' }).selectOption(tool);
              await page.getByRole('combobox', { name: 'Brush preset' }).selectOption('Soft round');
              await range(page, 'Brush size', 200);
              await range(page, 'Retouch strength', 0.5);
            } else {
              await page.getByRole('button', { name: 'Gradient', exact: true }).click();
              await page
                .getByRole('combobox', { name: 'Gradient preset' })
                .selectOption({ label: 'Sunset' });
              await page.getByRole('combobox', { name: 'Gradient shape' }).selectOption(tool);
              await range(page, 'Gradient opacity', 0.8);
            }
            const baselineHash = await digest(page);
            const baseline = await page.evaluate(
              () =>
                JSON.parse(document.querySelector('[data-testid="gpu-canvas"]').dataset.snapshot)
                  .history.undo,
            );
            const trials = [];
            const box = await page.getByTestId('gpu-canvas').boundingBox(),
              cx = box.x + box.width / 2,
              cy = box.y + box.height / 2;
            for (let trial = 0; trial < 4; trial++) {
              const observer = await page.evaluateHandle(() => {
                const c = document.querySelector('[data-testid="gpu-canvas"]');
                const s = JSON.parse(c.dataset.snapshot);
                const data = {
                  periods: [],
                  latencies: [],
                  completeLatencies: [],
                  cpu: [],
                  gpu: [],
                  pending: [],
                  inputs: [],
                  steps: [],
                  start: performance.now(),
                  frames: 0,
                };
                let last = performance.now(),
                  frame = Number(c.dataset.frames),
                  paint = s.paintTime,
                  complete = s.paintCompleteTime;
                const input = (e) => {
                  if (e.buttons & 1)
                    data.inputs.push({
                      x: e.clientX,
                      y: e.clientY,
                      time: e.timeStamp,
                      buttons: e.buttons,
                      pointerType: e.pointerType,
                    });
                };
                const kind = 'onpointerrawupdate' in window ? 'pointerrawupdate' : 'pointermove';
                c.addEventListener(kind, input);
                const o = new MutationObserver(() => {
                  const n = Number(c.dataset.frames);
                  if (n === frame) return;
                  frame = n;
                  const now = performance.now(),
                    s = JSON.parse(c.dataset.snapshot);
                  data.periods.push(now - last);
                  last = now;
                  data.frames++;
                  data.cpu.push(s.memory.document + s.memory.history + s.memory.temporary);
                  data.gpu.push(s.memory.gpu);
                  data.pending.push(s.pendingSamples);
                  data.steps.push(JSON.parse(c.dataset.timing).paintMs);
                  if (s.painting && s.paintTime > paint) {
                    paint = s.paintTime;
                    data.latencies.push(now - paint);
                  }
                  if (s.painting && s.paintCompleteTime > complete) {
                    complete = s.paintCompleteTime;
                    data.completeLatencies.push(now - complete);
                  }
                });
                o.observe(c, { attributes: true, attributeFilter: ['data-frames'] });
                return { data, o, input, kind };
              });
              const start = performance.now();
              let timing;
              if (gradient)
                timing = await page.evaluate(
                  async (shape) =>
                    (
                      await window.__fotoeditor.selectionAction({
                        kind: 'gradient',
                        gradient: {
                          shape,
                          start: [2900.5, 2000.5],
                          end: [3100.5, 2000.5],
                          stops: [
                            { position: 0, color: [42, 25, 91, 255] },
                            { position: 0.5, color: [222, 67, 105, 255] },
                            { position: 1, color: [255, 211, 136, 255] },
                          ],
                          opacity: 0.8,
                          reverse: false,
                        },
                      })
                    ).selection.timing,
                  tool,
                );
              else {
                await page.mouse.move(cx - 120, cy);
                await page.mouse.down();
                for (let i = 1; i <= 12; i++) {
                  await page.mouse.move(cx - 120 + i * 20, cy + Math.sin((i * Math.PI) / 6) * 20);
                  await delay(20);
                }
                await page.mouse.up();
              }
              await idle(page, baseline + 1);
              const totalMs = performance.now() - start;
              const raw = await observer.evaluate(({ data, o, input, kind }) => {
                o.disconnect();
                document
                  .querySelector('[data-testid="gpu-canvas"]')
                  .removeEventListener(kind, input);
                return {
                  ...data,
                  elapsedMs: performance.now() - data.start,
                  snapshot: JSON.parse(
                    document.querySelector('[data-testid="gpu-canvas"]').dataset.snapshot,
                  ),
                };
              });
              await observer.dispose();
              if (
                raw.snapshot.editError ||
                !raw.periods.length ||
                raw.snapshot.visibleTiles <= 0 ||
                Math.max(...raw.cpu) > 768 * 1024 ** 2 ||
                Math.max(...raw.gpu) > 128 * 1024 ** 2 ||
                (!gradient && (!raw.latencies.length || !raw.inputs.length))
              )
                throw Error(
                  'Invalid timing: ' + JSON.stringify({ tool, error: raw.snapshot.editError }),
                );
              const entry = {
                trial,
                first: trial === 0,
                totalMs,
                timing,
                presentationGapMs: summary(raw.periods),
                changedAckLatencyMs: summary(raw.latencies),
                completedAckLatencyMs: summary(raw.completeLatencies),
                cpuObservedPeak: Math.max(...raw.cpu),
                gpuObservedPeak: Math.max(...raw.gpu),
                inputCount: raw.inputs.length,
                inputElapsedMs: raw.inputs.at(-1)?.time - raw.inputs[0]?.time,
                changedAcknowledgements: raw.latencies.length,
                completedAcknowledgements: raw.completeLatencies.length,
                maxFramePaintMs: Math.max(...raw.steps),
                maxPending: Math.max(...raw.pending),
                raw,
              };
              trials.push(entry);
              if (trial === 3)
                await page.screenshot({
                  path: `bench/results/p9-m4-performance-${channel}-${backend}-${tool}.png`,
                });
              await page.evaluate((n) => window.__fotoeditor.historyJump(n), baseline);
              await idle(page, baseline);
            }
            const restoredHash = await digest(page);
            if (restoredHash !== baselineHash || errors.length)
              throw Error('Undo restoration or browser errors');
            const item = {
              channel,
              backend,
              tool,
              browser: browser.version(),
              gpu,
              adapter,
              baselineHash,
              restoredHash,
              firstMs: trials[0].totalMs,
              warmMs: summary(trials.slice(1).map((t) => t.totalMs)),
              trials,
            };
            report.results.push(item);
            await save();
            console.log(
              JSON.stringify({ channel, backend, tool, firstMs: item.firstMs, warm: item.warmMs }),
            );
          }
        } finally {
          await context.close();
        }
      }
    } finally {
      await browser.close();
    }
  }
  report.complete = true;
  await save();
} finally {
  server?.kill();
}
