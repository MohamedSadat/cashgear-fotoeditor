import { chromium } from '@playwright/test';
import { spawn } from 'node:child_process';
import { mkdir, writeFile, readFile, readdir } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { resolve } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
const base = process.env.PERF_BASE_URL ?? 'http://127.0.0.1:4188';
const phase = process.env.PERF_PHASE ?? 'p2';
const clone = process.env.CLONE_BENCH === '1';
const cloneAll = process.env.CLONE_ALL === '1';
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
const fixtureBytes = await readFile(resolve('bench/results/fixtures/large.png'));
const fixture = {
  bytes: fixtureBytes.length,
  sha256: createHash('sha256').update(fixtureBytes).digest('hex'),
};
const runtimeAssets = process.env.PERF_BASE_URL
  ? null
  : Object.fromEntries(
      await Promise.all(
        (await readdir('apps/web/dist/assets'))
          .filter((name) => /\.(wasm|js|css)$/.test(name))
          .map(async (name) => [
            name,
            createHash('sha256')
              .update(await readFile(resolve('apps/web/dist/assets', name)))
              .digest('hex'),
          ]),
      ),
    );
try {
  for (let i = 0; i < 100; i++) {
    try {
      if ((await fetch(base)).ok) break;
    } catch {
      /* Wait for preview. */
    }
    await delay(100);
  }
  for (const channel of process.env.PERF_CHANNELS?.split(',') ?? ['chrome', 'msedge']) {
    const browser = await chromium.launch({ channel });
    try {
      for (const backend of process.env.PERF_BACKENDS?.split(',') ?? ['WebGPU', 'WebGL2']) {
        const context = await browser.newContext({
          viewport: { width: 1280, height: 850 },
          deviceScaleFactor: 1,
        });
        const page = await context.newPage();
        const pageErrors = [];
        page.on('pageerror', (error) => pageErrors.push(error.message));
        await page.goto(base + (backend === 'WebGL2' ? '/?gpu=webgl2' : '/'));
        await page.getByTestId('engine-status').filter({ hasText: 'Engine ready' }).waitFor();
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
          /swiftshader|warp|llvmpipe|software|basic render/i.test(adapter ?? '') ||
          (backend === 'WebGL2' && !adapter)
        )
          throw new Error('Hardware adapter required: ' + JSON.stringify({ gpu, adapter }));
        await page
          .getByTestId('open-image')
          .setInputFiles(resolve('bench/results/fixtures/large.png'));
        await page.getByTestId('operation-status').filter({ hasText: 'Opened in' }).waitFor();
        await page.getByRole('button', { name: 'Actual pixels', exact: true }).click();
        await page.getByTestId('zoom').filter({ hasText: '100.0%' }).waitFor();
        await page.getByRole('combobox', { name: 'Brush preset' }).selectOption('Soft round');
        await page.getByRole('textbox', { name: 'Color hex' }).fill('#ff6030');
        if (clone) {
          if (cloneAll) {
            await page.getByRole('button', { name: '+ Layer', exact: true }).click();
            await page.waitForFunction(() => {
              const s = JSON.parse(
                document.querySelector('[data-testid="gpu-canvas"]').dataset.snapshot,
              );
              return s.layers.nodes.length === 2 && !s.operation;
            });
          }
          await page.getByRole('button', { name: 'Clone Stamp', exact: true }).click();
          await page
            .getByRole('slider', { name: 'Brush size', exact: true })
            .evaluate((element) => {
              Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set.call(
                element,
                '200',
              );
              element.dispatchEvent(new Event('input', { bubbles: true }));
              element.dispatchEvent(new Event('change', { bubbles: true }));
            });
          await page
            .getByRole('combobox', { name: 'Clone sample' })
            .selectOption(cloneAll ? 'all' : 'current');
        }
        await page.waitForFunction(
          () =>
            JSON.parse(document.querySelector('[data-testid="gpu-canvas"]').dataset.snapshot)
              .refining === 0,
        );
        // Prime a 300 px dab, including its neighboring atlas gutters, before warm measurements.
        const rect = await page.getByTestId('gpu-canvas').boundingBox();
        const cx = rect.x + rect.width / 2,
          cy = rect.y + rect.height / 2;
        if (clone) {
          await page.keyboard.down('Alt');
          await page.mouse.click(cx - 270, cy - 180);
          await page.keyboard.up('Alt');
          await page.waitForFunction(
            () =>
              JSON.parse(document.querySelector('[data-testid="gpu-canvas"]').dataset.snapshot)
                .cloneStamp.source !== null,
          );
        }
        const initialHistory = await page.evaluate(
          () =>
            JSON.parse(document.querySelector('[data-testid="gpu-canvas"]').dataset.snapshot)
              .history.undo,
        );
        const firstDabStart = performance.now();
        await page.mouse.click(cx - 270, cy);
        await page.waitForFunction(
          (baseline) =>
            JSON.parse(document.querySelector('[data-testid="gpu-canvas"]').dataset.snapshot)
              .history.undo ===
            baseline + 1,
          initialHistory,
        );
        const firstDabCommitMs = performance.now() - firstDabStart;
        const trials = [];
        const nativeInput = clone ? await context.newCDPSession(page) : undefined;
        for (let trial = 0; trial < 2; trial++) {
          const priorComplete = clone
            ? await page.evaluate(
                () =>
                  JSON.parse(document.querySelector('[data-testid="gpu-canvas"]').dataset.snapshot)
                    .paintCompleteTime,
              )
            : 0;
          const strokeStart = performance.now();
          await page.mouse.move(cx - 270, cy);
          await page.mouse.down();
          if (clone)
            await page.waitForFunction((prior) => {
              const s = JSON.parse(
                document.querySelector('[data-testid="gpu-canvas"]').dataset.snapshot,
              );
              return s.painting && s.paintCompleteTime > prior && s.pendingSamples === 0;
            }, priorComplete);
          const strokeStartAckMs = performance.now() - strokeStart;
          const measurement = await page.evaluateHandle(() => {
            const canvas = document.querySelector('[data-testid="gpu-canvas"]');
            const state = {
              periods: [],
              latencies: [],
              completeLatencies: [],
              pending: [],
              paintMs: [],
              renderMs: [],
              snapshotMs: [],
              cpuBytes: [],
              gpuBytes: [],
              start: performance.now(),
              initialFrames: Number(canvas.dataset.frames),
              inputEvents: 0,
              inputTrace: [],
            };
            const input = (event) => {
              state.inputEvents++;
              if (state.inputTrace.length < 8)
                state.inputTrace.push({
                  id: event.pointerId,
                  buttons: event.buttons,
                  time: event.timeStamp,
                  x: event.clientX,
                  y: event.clientY,
                });
            };
            const inputEvent = 'onpointerrawupdate' in window ? 'pointerrawupdate' : 'pointermove';
            canvas.addEventListener(inputEvent, input);
            let lastFrame = performance.now(),
              paintTime = JSON.parse(canvas.dataset.snapshot).paintTime,
              completeTime = JSON.parse(canvas.dataset.snapshot).paintCompleteTime,
              frames = Number(canvas.dataset.frames);
            const observer = new MutationObserver(() => {
              const n = Number(canvas.dataset.frames);
              if (n === frames) return;
              frames = n;
              const now = performance.now();
              state.periods.push(now - lastFrame);
              lastFrame = now;
              const s = JSON.parse(canvas.dataset.snapshot);
              state.cpuBytes.push(s.memory.document + s.memory.history + s.memory.temporary);
              state.gpuBytes.push(s.memory.gpu);
              state.pending.push(s.pendingSamples);
              const timing = JSON.parse(canvas.dataset.timing);
              state.paintMs.push(timing.paintMs);
              state.renderMs.push(timing.renderMs);
              state.snapshotMs.push(timing.snapshotMs);
              if (s.paintTime > paintTime && s.painting) {
                paintTime = s.paintTime;
                state.latencies.push(now - paintTime);
              }
              if (s.paintCompleteTime > completeTime && s.painting) {
                completeTime = s.paintCompleteTime;
                state.completeLatencies.push(now - completeTime);
              }
            });
            observer.observe(canvas, { attributes: true, attributeFilter: ['data-frames'] });
            return { state, observer, input, inputEvent };
          });
          // Native protocol mouse moves exercise Chromium's real input delivery /
          // coalescing. Dispatching synthetic events after the app's RAF callback
          // would manufacture an extra frame of input delay.
          const start = performance.now();
          let tick = 0;
          const pendingInput = new Set();
          let nativeError;
          let maxInputInFlight = 0;
          let lastX = cx - 270,
            lastY = cy;
          while (performance.now() - start < 4000) {
            const elapsed = performance.now() - start,
              phase = (elapsed * 1.8) % 1080,
              x = phase <= 540 ? phase : 1080 - phase;
            lastX = cx - 270 + x;
            lastY = cy + Math.sin(elapsed / 400) * 60;
            if (nativeInput) {
              // Native protocol acknowledgements may take a full display frame.
              // Bound outstanding input to four rather than serializing a
              // requested 120 Hz stream down to 45-50 Hz. The engine's normal
              // native/coalesced pointer path still receives every event.
              if (pendingInput.size >= 4) await Promise.race(pendingInput);
              if (nativeError) throw nativeError;
              const pending = nativeInput
                .send('Input.dispatchMouseEvent', {
                  type: 'mouseMoved',
                  x: lastX,
                  y: lastY,
                  buttons: 1,
                  button: 'left',
                  force: 0.5,
                  modifiers: 0,
                  pointerType: 'mouse',
                })
                .catch((error) => {
                  nativeError = error;
                })
                .finally(() => pendingInput.delete(pending));
              pendingInput.add(pending);
              maxInputInFlight = Math.max(maxInputInFlight, pendingInput.size);
            } else await page.mouse.move(lastX, lastY);
            tick++;
            await delay(Math.max(0, start + (tick * 1000) / 120 - performance.now()));
          }
          await Promise.all(pendingInput);
          if (nativeError) throw nativeError;
          const raw = await measurement.evaluate(({ state, observer, input, inputEvent }) => {
            observer.disconnect();
            const canvas = document.querySelector('[data-testid="gpu-canvas"]');
            canvas.removeEventListener(inputEvent, input);
            return {
              ...state,
              elapsedMs: performance.now() - state.start,
              presented: Number(canvas.dataset.frames) - state.initialFrames,
              maxPending: Math.max(...state.pending),
              snapshot: JSON.parse(canvas.dataset.snapshot),
              engineStatus: document.querySelector('[data-testid="engine-status"]').textContent,
            };
          });
          await measurement.dispose();
          if (
            clone &&
            (raw.snapshot.editError ||
              raw.snapshot.document.width !== 6000 ||
              raw.snapshot.document.height !== 4000 ||
              raw.snapshot.visibleTiles <= 0 ||
              raw.latencies.length === 0 ||
              Math.max(...raw.cpuBytes) > 768 * 1024 ** 2 ||
              Math.max(...raw.gpuBytes) > 128 * 1024 ** 2)
          )
            throw new Error(
              'Clone benchmark failed its document/change/residency checks: ' +
                JSON.stringify({
                  status: raw.engineStatus,
                  pageErrors,
                  input: raw.inputTrace,
                  snapshot: raw.snapshot,
                }),
            );
          if (nativeInput) await page.mouse.move(lastX, lastY);
          await page.mouse.up();
          await page.waitForFunction(
            () =>
              !JSON.parse(document.querySelector('[data-testid="gpu-canvas"]').dataset.snapshot)
                .painting,
          );
          trials.push({
            fps: (raw.presented * 1000) / raw.elapsedMs,
            strokeStartAckMs,
            changedPixelAckFps: (raw.latencies.length * 1000) / raw.elapsedMs,
            peakLogicalCpuBytes: Math.max(...raw.cpuBytes),
            peakLogicalGpuBytes: Math.max(...raw.gpuBytes),
            frameP50: percentile(raw.periods, 0.5),
            frameP95: percentile(raw.periods, 0.95),
            frameP99: percentile(raw.periods, 0.99),
            sampleToFrameAckP50: percentile(raw.latencies, 0.5),
            sampleToFrameAckP95: percentile(raw.latencies, 0.95),
            sampleToFrameAckP99: percentile(raw.latencies, 0.99),
            fullDabToFrameAckP95: percentile(raw.completeLatencies, 0.95),
            paintP95: percentile(raw.paintMs, 0.95),
            renderP95: percentile(raw.renderMs, 0.95),
            snapshotP95: percentile(raw.snapshotMs, 0.95),
            maxPending: raw.maxPending,
            samples: raw.latencies.length,
            presented: raw.presented,
            elapsedMs: raw.elapsedMs,
            requestedNativeMoves: tick,
            deliveredNativeEvents: raw.inputEvents,
            maxInputInFlight,
            snapshot: raw.snapshot,
          });
        }
        const result = {
          channel,
          backend,
          browser: browser.version(),
          gpu,
          adapter,
          inputMode: await page.getByTestId('gpu-canvas').getAttribute('data-pointer-input'),
          firstDabCommitMs,
          viewport: { width: 1280, height: 850, dpr: 1 },
          dataset: `Deterministic 6000x4000 RGBA8 image fixture; normal document view, 100%; ${clone ? '200px soft Clone Stamp, aligned, source offset [0,-180], ' + (cloneAll ? 'all visible layers into a new raster' : 'current raster') : '300px soft round'}; 100% opacity/flow, 10% spacing, native protocol mouse input at 120Hz and 1800px/s; two 4s warm trials after one priming dab`,
          latencyDefinition:
            'Timestamp of the pointer sample first changing a rendered document pixel, to main-thread observation of the worker presentation acknowledgement. Full-dab completion is reported separately. Includes queueing/CPU/upload/submission, excluding compositor/scanout and physical pen latency.',
          trials,
        };
        results.push(result);
        console.log(JSON.stringify(result));
        await page.screenshot({ path: `bench/results/${phase}-brush-${channel}-${backend}.png` });
        await context.close();
      }
    } finally {
      await browser.close();
    }
  }
  await mkdir('bench/results', { recursive: true });
  await writeFile(
    `bench/results/${phase}-performance.json`,
    JSON.stringify(
      { measuredAt: new Date().toISOString(), fixture, runtimeAssets, results },
      null,
      2,
    ) + '\n',
  );
} finally {
  server?.kill();
}
