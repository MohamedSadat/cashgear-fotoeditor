import { test, expect } from '@playwright/test';
import type { Page } from '@playwright/test';
import type { EngineApi } from '../../apps/web/src/engine/types';
import { resolve } from 'node:path';
import { readFile } from 'node:fs/promises';
import { PNG } from 'pngjs';
const state = async (page: Page) =>
  JSON.parse((await page.getByTestId('gpu-canvas').getAttribute('data-snapshot'))!);
async function image(page: Page) {
  const download = page.waitForEvent('download');
  await page.getByRole('button', { name: 'Export PNG', exact: true }).click();
  return PNG.sync.read(await readFile((await (await download).path())!));
}
for (const backend of ['WebGPU', 'WebGL2']) {
  test(
    backend + ' edits layers and masks with exact undo and streamed project roundtrip',
    async ({ page }) => {
      test.setTimeout(180000);
      await page.goto('/?diagnostic=1&document=1' + (backend === 'WebGL2' ? '&gpu=webgl2' : ''));
      await expect(page.getByTestId('engine-status')).toHaveText('Engine ready');
      await page
        .getByTestId('open-image')
        .setInputFiles(resolve('bench/results/fixtures/layers-small.cgfoto'));
      await expect(page.getByTestId('operation-status')).toHaveText(/Opened in/);
      await expect.poll(async () => (await state(page)).layers.nodes.length).toBe(3);
      await expect.poll(async () => (await state(page)).operation).toBeUndefined();
      const before = await image(page);
      const target = (await state(page)).layers.nodes[2].id;
      await page.evaluate(async (target) => {
        const api = Reflect.get(window, '__fotoeditor') as EngineApi;
        await api.selectLayer(target);
        await api.layerAction({ kind: 'mask-add', target });
        await api.selectLayer(target, true);
      }, target);
      await page.getByRole('textbox', { name: 'Color hex' }).fill('#000000');
      const box = (await page.getByTestId('gpu-canvas').boundingBox())!;
      await page.mouse.click(box.x + box.width / 2, box.y + box.height / 2);
      await expect.poll(async () => (await state(page)).history.undo).toBe(2);
      await expect.poll(async () => (await state(page)).painting).toBe(false);
      await expect.poll(async () => (await state(page)).operation).toBeUndefined();
      const masked = await image(page);
      expect(masked.data.equals(before.data)).toBe(false);
      await page.getByRole('button', { name: 'Undo', exact: true }).click();
      await expect.poll(async () => (await state(page)).history.undo).toBe(1);
      expect((await image(page)).data.equals(before.data)).toBe(true);
      await page.getByRole('button', { name: 'Redo', exact: true }).click();
      await expect.poll(async () => (await state(page)).history.undo).toBe(2);
      expect((await image(page)).data.equals(masked.data)).toBe(true);
      const roundtrip = await page.evaluate(async () => {
        const api = Reflect.get(window, '__fotoeditor') as EngineApi;
        const directory = await navigator.storage.getDirectory();
        const handle = await directory.getFileHandle('manual-test.cgfoto', { create: true });
        await api.saveProjectFile(handle);
        const first = new Uint8Array(await (await handle.getFile()).arrayBuffer());
        await api.openProjectFile(await handle.getFile());
        await api.saveProjectFile(handle);
        const second = new Uint8Array(await (await handle.getFile()).arrayBuffer());
        return {
          same: first.length === second.length && first.every((b, i) => b === second[i]),
          size: first.length,
        };
      });
      expect(roundtrip.same).toBe(true);
      expect(roundtrip.size).toBeGreaterThan(786432);
      await expect.poll(async () => (await state(page)).history.undo).toBe(0);
      expect((await image(page)).data.equals(masked.data)).toBe(true);
      await expect(page.getByTestId('engine-status')).toHaveText('Engine ready');
    },
  );
}
