import { test, expect } from '@playwright/test';
import { readFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { PNG } from 'pngjs';
import type { EngineApi, DocumentSnapshot } from '../../apps/web/src/engine/types';
import { whenIdle } from './idle';

const snapshot = async (page: import('@playwright/test').Page) =>
  JSON.parse(
    (await page.getByTestId('gpu-canvas').getAttribute('data-snapshot'))!,
  ) as DocumentSnapshot;

for (const backend of ['WebGPU', 'WebGL2']) {
  for (const name of [
    'p8-input.bmp',
    'p8-input.gif',
    'p8-input.tiff',
    'p8-icc-linear.png',
    'p8-icc-linear.bmp',
  ]) {
    test(
      backend + ' imports and exports exact supported samples from ' + name,
      async ({ page }) => {
        await page.goto('/?diagnostic=1&document=1' + (backend === 'WebGL2' ? '&gpu=webgl2' : ''));
        await expect(page.getByTestId('engine-status')).toHaveText('Engine ready');
        await page.getByTestId('open-image').setInputFiles(resolve('bench/results/fixtures', name));
        await expect(page.getByTestId('operation-status')).toHaveText(/Opened in/);
        const bytes = await whenIdle(page, () =>
          page.evaluate(async () =>
            Array.from(await (Reflect.get(window, '__fotoeditor') as EngineApi).exportPng()),
          ),
        );
        const expected = PNG.sync.read(
          await readFile(resolve('bench/results/fixtures', name + '.expected.png')),
        );
        const actual = PNG.sync.read(Buffer.from(bytes));
        expect([actual.width, actual.height]).toEqual([expected.width, expected.height]);
        expect(actual.data.equals(expected.data)).toBe(true);
        expect(Buffer.from(bytes).includes(Buffer.from('iCCP'))).toBe(true);
      },
    );
  }
  test(
    backend + ' rejects bad ICC and depth without replacing document or redo',
    async ({ page }) => {
      await page.goto('/?diagnostic=1&document=1' + (backend === 'WebGL2' ? '&gpu=webgl2' : ''));
      await expect(page.getByTestId('engine-status')).toHaveText('Engine ready');
      await page
        .getByTestId('open-image')
        .setInputFiles(resolve('bench/results/fixtures/p8-input.tiff'));
      await expect(page.getByTestId('operation-status')).toHaveText(/Opened in/);
      const base = (await snapshot(page)).history.undo;
      await page.getByRole('textbox', { name: 'Color hex' }).fill('#573921ff');
      const rect = (await page.getByTestId('gpu-canvas').boundingBox())!;
      await page.mouse.click(rect.x + rect.width / 2, rect.y + rect.height / 2);
      await expect.poll(async () => (await snapshot(page)).history.undo).toBe(base + 1);
      await page.getByRole('button', { name: 'Undo', exact: true }).click();
      await expect.poll(async () => (await snapshot(page)).history.redo).toBe(1);
      const before = await whenIdle(page, () =>
        page.evaluate(async () =>
          Array.from(await (Reflect.get(window, '__fotoeditor') as EngineApi).exportPng()),
        ),
      );
      for (const [file, error] of [
        ['p8-invalid-icc.png', /ICC/],
        ['p8-invalid-icc.bmp', /ICC/],
        ['p8-16bit.png', /16-bit/],
      ] as const) {
        await page.getByTestId('open-image').setInputFiles(resolve('bench/results/fixtures', file));
        await expect(page.getByTestId('operation-status')).toHaveText(error);
        expect((await snapshot(page)).history.redo).toBe(1);
        const after = await whenIdle(page, () =>
          page.evaluate(async () =>
            Array.from(await (Reflect.get(window, '__fotoeditor') as EngineApi).exportPng()),
          ),
        );
        expect(Buffer.from(after).equals(Buffer.from(before))).toBe(true);
      }
    },
  );
  test(backend + ' writes ZIP LZ4 projects and preserves full layered state', async ({ page }) => {
    await page.goto('/?diagnostic=1&document=1' + (backend === 'WebGL2' ? '&gpu=webgl2' : ''));
    await expect(page.getByTestId('engine-status')).toHaveText('Engine ready');
    await page
      .getByTestId('open-image')
      .setInputFiles(resolve('bench/results/fixtures/layers-small.cgfoto'));
    await expect(page.getByTestId('operation-status')).toHaveText(/Opened in/);
    const result = await whenIdle(page, () =>
      page.evaluate(async () => {
        const api = Reflect.get(window, '__fotoeditor') as EngineApi;
        const directory = await navigator.storage.getDirectory();
        const handle = await directory.getFileHandle('p8-roundtrip.cgfoto', { create: true });
        const size = await api.saveProjectFile(handle);
        const first = new Uint8Array(await (await handle.getFile()).arrayBuffer());
        const state = await api.openProjectFile(await handle.getFile());
        await api.saveProjectFile(handle);
        const second = new Uint8Array(await (await handle.getFile()).arrayBuffer());
        return {
          size,
          magic: Array.from(first.subarray(0, 4)),
          layers: state.layers.nodes.length,
          undo: state.history.undo,
          exact: first.length === second.length && first.every((b, i) => b === second[i]),
          names:
            new TextDecoder().decode(first).includes('thumbnail.png') &&
            new TextDecoder().decode(first).includes('composite.png'),
        };
      }),
    );
    expect(result).toMatchObject({
      magic: [0x50, 0x4b, 3, 4],
      layers: 3,
      undo: 0,
      exact: true,
      names: true,
    });
    expect(result.size).toBeGreaterThan(0);
  });
}
