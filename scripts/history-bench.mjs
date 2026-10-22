import { chromium, expect } from '@playwright/test';
import { spawn } from 'node:child_process';
import { readFile, writeFile, mkdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { createHash } from 'node:crypto';
import { setTimeout as delay } from 'node:timers/promises';
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
const results = [];
const hash = (bytes) => createHash('sha256').update(bytes).digest('hex');
try {
  for (let i = 0; i < 100; i++) {
    try {
      if ((await fetch(base)).ok) break;
    } catch {
      /* Starting preview. */
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
        const snapshot = async () =>
          JSON.parse(await page.getByTestId('gpu-canvas').getAttribute('data-snapshot'));
        const idle = async () => {
          await expect
            .poll(async () => {
              const s = await snapshot();
              return !s.painting && !s.operation;
            })
            .toBe(true);
        };
        await page.goto(base + (backend === 'WebGL2' ? '/?gpu=webgl2' : '/'));
        await expect(page.getByTestId('engine-status')).toHaveText('Engine ready');
        const gpu = await page.evaluate(async () => {
          const a = await navigator.gpu.requestAdapter();
          return {
            vendor: a?.info.vendor,
            architecture: a?.info.architecture,
            isFallbackAdapter: a?.info.isFallbackAdapter,
          };
        });
        const adapter = await page.getByTestId('gpu-backend').getAttribute('title');
        if (
          !gpu.vendor ||
          gpu.isFallbackAdapter ||
          /swiftshader|warp|software|llvmpipe|basic render/i.test(adapter) ||
          (backend === 'WebGL2' && !adapter)
        )
          throw new Error('Hardware GPU required');
        await page
          .getByTestId('open-image')
          .setInputFiles(resolve('bench/results/fixtures/large.png'));
        await expect(page.getByTestId('operation-status')).toHaveText(/Opened in/);
        await page.getByRole('button', { name: 'Actual pixels', exact: true }).click();
        await expect(page.getByTestId('zoom')).toHaveText('100.0%');
        await page.getByRole('combobox', { name: 'Brush preset' }).selectOption('Soft round');
        await page.getByRole('slider', { name: 'Brush size', exact: true }).evaluate((e) => {
          Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set.call(e, '300');
          e.dispatchEvent(new Event('input', { bubbles: true }));
        });
        const box = await page.getByTestId('gpu-canvas').boundingBox();
        let peakCpu = 0,
          peakHistory = 0;
        const started = performance.now();
        for (let i = 0; i < 200; i++) {
          const before = (await snapshot()).revision;
          await page
            .getByRole('textbox', { name: 'Color hex' })
            .fill('#' + (i + 0x502010).toString(16));
          await page.mouse.click(
            box.x + box.width / 2 - 180 + (i % 10) * 40,
            box.y + box.height / 2 - 90 + (Math.floor(i / 10) % 5) * 45,
          );
          await page.waitForFunction((before) => {
            const s = JSON.parse(
              document.querySelector('[data-testid="gpu-canvas"]').dataset.snapshot,
            );
            return s.revision > before && !s.painting && !s.operation;
          }, before);
          await idle();
          const s = await snapshot(),
            cpu = s.memory.document + s.memory.history + s.memory.temporary;
          peakCpu = Math.max(peakCpu, cpu);
          peakHistory = Math.max(peakHistory, s.memory.history);
          if (
            cpu > 768 * 1048576 ||
            s.memory.history > s.history.bytesLimit ||
            s.history.undo + s.history.redo > 200
          )
            throw new Error('Memory/step budget exceeded');
        }
        const strokeMs = performance.now() - started,
          afterStrokes = await snapshot();
        const cycles = [];
        for (let i = 0; i < 2; i++) {
          const start = performance.now(),
            download = page.waitForEvent('download');
          await page.getByRole('button', { name: 'Save project', exact: true }).click();
          const bytes = await readFile(await (await download).path());
          await idle();
          const saveMs = performance.now() - start;
          // Playwright caps in-memory file inputs at 50 MB; exercise the normal
          // disk-backed input path for this 96 MiB project instead.
          const projectPath = resolve(`bench/results/p3-${channel}-${backend}.cgfoto`);
          await writeFile(projectPath, bytes);
          const openStart = performance.now();
          await page.getByTestId('open-project').setInputFiles(projectPath);
          await expect(page.getByTestId('operation-status')).toHaveText(/Opened in/);
          await idle();
          const openMs = performance.now() - openStart;
          if ((await snapshot()).history.undo !== 0)
            throw new Error('Project history was not reset');
          cycles.push({ saveMs, openMs, bytes: bytes.length, sha256: hash(bytes) });
        }
        if (cycles[0].sha256 !== cycles[1].sha256)
          throw new Error('Project roundtrip changed samples/metadata');
        const autosaveMs = [];
        const autosaveWorkerMs = [];
        for (let i = 0; i < 4; i++) {
          const previous = (await snapshot()).recovery.lastSavedAt ?? 0;
          await page
            .getByRole('slider', { name: 'Document resolution DPI' })
            .evaluate((e, value) => {
              Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set.call(
                e,
                String(value),
              );
              e.dispatchEvent(new Event('input', { bubbles: true }));
            }, 300 + i);
          await expect.poll(async () => (await snapshot()).document.dpi).toBe(300 + i);
          const start = performance.now();
          await page.getByRole('button', { name: 'Save recovery now', exact: true }).click();
          await expect
            .poll(async () => (await snapshot()).recovery.lastSavedAt ?? 0)
            .toBeGreaterThan(previous);
          await idle();
          autosaveMs.push(performance.now() - start);
          autosaveWorkerMs.push((await snapshot()).recovery.lastSaveMs);
        }
        const generations = await page.evaluate(async () => {
          const d = await (
            await navigator.storage.getDirectory()
          ).getDirectoryHandle('cashgear-recovery-v1');
          let n = 0;
          for await (const [name] of d.entries()) if (name.endsWith('.json')) n++;
          return n;
        });
        if (generations !== 3) throw new Error('Expected three retained completed generations');
        const result = {
          channel,
          backend,
          browser: browser.version(),
          gpu,
          adapter,
          viewport: { width: 1280, height: 850, dpr: 1 },
          strokeMs,
          peakCpu,
          peakHistory,
          afterStrokes,
          cycles,
          autosaveMs,
          autosaveWorkerMs,
          generations,
          final: await snapshot(),
        };
        results.push(result);
        console.log(
          JSON.stringify({
            channel,
            backend,
            strokeMs,
            peakCpu,
            peakHistory,
            cycles,
            autosaveMs,
            autosaveWorkerMs,
            generations,
          }),
        );
        await page.screenshot({ path: `bench/results/p3-history-${channel}-${backend}.png` });
        await context.close();
      }
    } finally {
      await browser.close();
    }
  }
  await mkdir('bench/results', { recursive: true });
  await writeFile(
    'bench/results/p3-history-performance.json',
    JSON.stringify(
      {
        measuredAt: new Date().toISOString(),
        dataset:
          'Existing deterministic 6000x4000 RGBA8 PNG; hardware only; 200 native mouse dabs with 300 px soft round at 100%; two warmed project save/reopen cycles; four flushed/readback-validated autosaves. Times include UI automation, bridge/file and presentation acknowledgements; autosave blocks editing until completion.',
        results,
      },
      null,
      2,
    ) + '\n',
  );
} finally {
  server?.kill();
}
