import { test, expect } from '@playwright/test';
import type { EngineApi } from '../../apps/web/src/engine/types';
import { readFile } from 'node:fs/promises';
import { PNG } from 'pngjs';
import type { Page } from '@playwright/test';
const snap = async (page: Page) =>
  JSON.parse((await page.getByTestId('gpu-canvas').getAttribute('data-snapshot'))!);
async function exported(page: Page) {
  const download = page.waitForEvent('download');
  await page.getByRole('button', { name: 'Export PNG', exact: true }).click();
  return PNG.sync.read(await readFile((await (await download).path())!));
}
for (const backend of ['WebGPU', 'WebGL2']) {
  test(
    backend +
      ' layer controls reorder group lock rasterize masks and undo the complete edit sequence',
    async ({ page }) => {
      test.setTimeout(180000);
      await page.goto('/?diagnostic=1&document=1' + (backend === 'WebGL2' ? '&gpu=webgl2' : ''));
      await expect(page.getByTestId('engine-status')).toHaveText('Engine ready');
      await page.getByRole('button', { name: 'New canvas', exact: true }).click();
      await page.getByRole('spinbutton', { name: 'Width', exact: true }).fill('128');
      await page.getByRole('spinbutton', { name: 'Height', exact: true }).fill('128');
      await page.getByRole('button', { name: 'Create document' }).click();
      await page.getByRole('button', { name: '+ Layer', exact: true }).click();
      await expect.poll(async () => (await snap(page)).layers.nodes.length).toBe(2);
      await page.getByRole('textbox', { name: 'Layer name', exact: true }).fill('Ink');
      await page.getByRole('textbox', { name: 'Layer name', exact: true }).press('Enter');
      await expect.poll(async () => (await snap(page)).layers.nodes[1].name).toBe('Ink');
      await page.getByRole('button', { name: 'Actual pixels', exact: true }).click();
      await page.getByRole('textbox', { name: 'Color hex' }).fill('#ff0000');
      const canvas = page.getByTestId('gpu-canvas');
      const box = (await canvas.boundingBox())!;
      await page.mouse.click(box.x + box.width / 2, box.y + box.height / 2);
      await expect.poll(async () => (await snap(page)).painting).toBe(false);
      await expect.poll(async () => (await snap(page)).visibleTiles).toBeGreaterThan(0);
      const red = await exported(page);
      await page.getByRole('checkbox', { name: 'Lock pixels', exact: true }).click();
      await expect
        .poll(async () => (await snap(page)).layers.nodes[1].properties.locks.pixels)
        .toBe(true);
      // Metadata can be published before recomposition releases the operation.
      // Wait until the canvas accepts input before testing lock rejection.
      await expect.poll(async () => (await snap(page)).operation).toBeUndefined();
      const history = (await snap(page)).history.undo;
      await page.mouse.click(box.x + box.width / 2 + 30, box.y + box.height / 2);
      await expect.poll(async () => (await snap(page)).editError).toMatch(/locked|Unlock/i);
      expect((await snap(page)).history.undo).toBe(history);
      await page.getByRole('checkbox', { name: 'Lock pixels', exact: true }).click();
      await expect
        .poll(async () => (await snap(page)).layers.nodes[1].properties.locks.pixels)
        .toBe(false);
      await expect.poll(async () => (await snap(page)).operation).toBeUndefined();
      // Both rows must remain visible while a native drag starts. Scrolling the
      // destination after pointer-down can move the source out from under it.
      await page
        .getByRole('group', { name: 'Layer stack', exact: true })
        .evaluate((stack) => stack.scrollIntoView({ block: 'center' }));
      await page
        .locator('.layer-name')
        .filter({ hasText: 'Ink' })
        .dragTo(page.locator('.layer-name').filter({ hasText: 'Layer 1' }));
      await expect.poll(async () => (await snap(page)).layers.nodes[0].name).toBe('Ink');
      await page.locator('.layer-name').filter({ hasText: 'Ink' }).click();
      await page
        .locator('.layer-name')
        .filter({ hasText: 'Layer 1' })
        .click({ modifiers: ['Control'] });
      await page.getByRole('button', { name: 'Group', exact: true }).click();
      await expect.poll(async () => (await snap(page)).layers.nodes.length).toBe(1);
      await expect
        .poll(async () => (await snap(page)).layers.nodes[0].properties.passThrough)
        .toBe(true);
      await page.getByRole('button', { name: 'Add mask', exact: true }).click();
      await page
        .getByRole('button', { name: 'Mask of Group', exact: true })
        .click({ modifiers: ['Alt'] });
      await expect.poll(async () => (await snap(page)).layers.maskView).toBe(true);
      expect((await exported(page)).data.equals(red.data)).toBe(true);
      await page.getByRole('combobox', { name: 'Layer blend mode' }).selectOption('normal');
      await page.getByRole('button', { name: 'Apply mask', exact: true }).click();
      await expect.poll(async () => (await snap(page)).layers.nodes[0].children).toBeNull();
      expect((await exported(page)).data.equals(red.data)).toBe(true);
      await page.evaluate(async () => {
        await (Reflect.get(window, '__fotoeditor') as EngineApi).historyJump(0);
      });
      await expect.poll(async () => (await snap(page)).history.undo).toBe(0);
      expect((await exported(page)).data.equals(Buffer.alloc(128 * 128 * 4))).toBe(true);
    },
  );
}
