import { test, expect } from '@playwright/test';
import { readFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { PNG } from 'pngjs';
import type { DocumentSnapshot } from '../../apps/web/src/engine/types';
const fixtures = resolve('bench/results/fixtures');
const snap = async (page: import('@playwright/test').Page) =>
  JSON.parse(
    (await page.getByTestId('gpu-canvas').getAttribute('data-snapshot'))!,
  ) as DocumentSnapshot;

for (const backend of ['WebGPU', 'WebGL2']) {
  const query = backend === 'WebGL2' ? '?gpu=webgl2' : '';
  test(
    backend + ' opens exact PNG samples, navigates and exports unchanged pixels',
    async ({ page }) => {
      await page.goto('/' + query);
      await expect(page.getByTestId('gpu-backend')).toHaveText(backend);
      await expect(page.getByTestId('engine-status')).toHaveText('Engine ready');
      const sizes = await page.getByTestId('gpu-canvas').evaluateHandle((canvas) => {
        const heights = new Set([canvas.getAttribute('data-height')]);
        const observer = new MutationObserver(() =>
          heights.add(canvas.getAttribute('data-height')),
        );
        observer.observe(canvas, { attributes: true, attributeFilter: ['data-height'] });
        return { heights, observer };
      });
      await page.getByTestId('open-image').setInputFiles(resolve(fixtures, 'alpha.png'));
      await expect(page.getByTestId('operation-status')).toHaveText(/Opened in/);
      await expect(page.getByTestId('document-size')).toHaveText('513 × 259');
      const canvas = page.getByTestId('gpu-canvas');
      await page.getByRole('button', { name: 'Actual pixels', exact: true }).click();
      await expect(page.getByTestId('zoom')).toHaveText('100.0%');
      await expect.poll(async () => (await snap(page)).refining).toBe(0);
      const frameCount = Number(await canvas.getAttribute('data-frames'));
      await expect
        .poll(async () => Number(await canvas.getAttribute('data-frames')))
        .toBeGreaterThan(frameCount + 2);
      expect(
        await sizes.evaluate(({ heights, observer }) => {
          observer.disconnect();
          return heights.size;
        }),
      ).toBeLessThanOrEqual(2);
      await sizes.dispose();
      const image = PNG.sync.read(await canvas.screenshot());
      // Opaque samples straddle tile boundaries; half-alpha green shows the checker.
      for (const [x, y, rgb] of [
        [-50, -50, [255, 0, 0]],
        [-50, 50, [0, 0, 255]],
        [50, 50, [255, 255, 255]],
      ] as const) {
        const i =
          (Math.floor(image.height / 2 + y) * image.width + Math.floor(image.width / 2 + x)) * 4;
        rgb.forEach((c, channel) =>
          expect(Math.abs(image.data[i + channel] - c)).toBeLessThanOrEqual(1),
        );
      }
      const before = await snap(page);
      const rect = (await canvas.boundingBox())!;
      await page.mouse.move(rect.x + rect.width / 2, rect.y + rect.height / 2);
      await page.keyboard.down('Space');
      await page.mouse.down();
      await page.mouse.move(rect.x + rect.width / 2 + 40, rect.y + rect.height / 2);
      await page.mouse.up();
      await page.keyboard.up('Space');
      await expect.poll(async () => (await snap(page)).view.panX).toBe(before.view.panX + 40);
      await page.mouse.down({ button: 'middle' });
      await page.mouse.move(rect.x + rect.width / 2 + 60, rect.y + rect.height / 2);
      await page.mouse.up({ button: 'middle' });
      await expect.poll(async () => (await snap(page)).view.panX).toBe(before.view.panX + 60);
      await page.keyboard.down('r');
      await page.mouse.down();
      await page.mouse.move(rect.x + rect.width / 2 + 90, rect.y + rect.height / 2);
      await page.mouse.up();
      await page.keyboard.up('r');
      await expect.poll(async () => (await snap(page)).view.rotation).toBeCloseTo(0.3, 7);
      await page.getByRole('button', { name: 'Reset view rotation' }).click();
      await expect.poll(async () => (await snap(page)).view.rotation).toBe(0);
      await page.mouse.move(rect.x + rect.width / 2, rect.y + rect.height / 2);
      await page.mouse.wheel(0, 80);
      await expect.poll(async () => (await snap(page)).view.panY).toBe(before.view.panY - 80);
      await page.keyboard.down('Control');
      await page.mouse.wheel(0, -10000);
      await page.keyboard.up('Control');
      await expect(page.getByTestId('zoom')).toHaveText('3200.0%');
      await page.keyboard.down('Control');
      await page.mouse.wheel(0, 10000);
      await page.keyboard.up('Control');
      await expect(page.getByTestId('zoom')).toHaveText('1.0%');
      await page.getByRole('button', { name: 'Fit canvas', exact: true }).click();
      await page.getByRole('button', { name: 'Rotate view 90 degrees' }).click();
      await page.getByRole('button', { name: 'Flip view horizontally' }).click();
      await expect.poll(async () => (await snap(page)).view.flipped).toBe(true);
      expect((await snap(page)).view.rotation).toBeCloseTo(Math.PI / 2, 7);
      const downloadEvent = page.waitForEvent('download');
      await page.getByRole('button', { name: 'Export PNG' }).click();
      const download = await downloadEvent;
      const exported = PNG.sync.read(await readFile((await download.path())!));
      const original = PNG.sync.read(await readFile(resolve(fixtures, 'alpha.png')));
      expect(exported.width).toBe(original.width);
      expect(exported.height).toBe(original.height);
      expect(exported.data.equals(original.data)).toBe(true);
      await page.getByTestId('open-image').setInputFiles({
        name: 'broken.png',
        mimeType: 'image/png',
        buffer: Buffer.from('not an image'),
      });
      await expect(page.getByTestId('operation-status')).toContainText('Unrecognized image');
      await expect(page.getByTestId('document-size')).toHaveText('513 × 259');
      expect((await snap(page)).memory.temporary).toBe(0);
      // Cancel while tiled construction is pending; the old document must survive.
      await page.getByTestId('open-image').setInputFiles(resolve(fixtures, 'large.png'));
      await page.waitForFunction(() => {
        const canvas = document.querySelector<HTMLElement>('[data-testid="gpu-canvas"]');
        return canvas?.dataset.snapshot && JSON.parse(canvas.dataset.snapshot).memory.temporary > 0;
      });
      await page.getByRole('button', { name: 'Cancel operation' }).click();
      await expect(page.getByTestId('operation-status')).toHaveText('Operation cancelled');
      await expect.poll(async () => (await snap(page)).memory.temporary).toBe(0);
      await expect(page.getByTestId('document-size')).toHaveText('513 × 259');
    },
  );
  test(backend + ' creates sparse maximum canvas and validates presets', async ({ page }) => {
    await page.goto('/' + query);
    await expect(page.getByTestId('engine-status')).toHaveText('Engine ready');
    await page.getByRole('button', { name: 'New canvas', exact: true }).click();
    await expect(page.getByRole('spinbutton', { name: 'Width', exact: true })).toHaveValue('1920');
    await expect(page.getByRole('spinbutton', { name: 'Resolution DPI' })).toHaveValue('72');
    await page.getByRole('combobox').selectOption('A4 · Print');
    await expect(page.getByRole('spinbutton', { name: 'Width', exact: true })).toHaveValue('2480');
    await page.getByRole('spinbutton', { name: 'Width', exact: true }).fill('16385');
    await page.getByRole('button', { name: 'Create document' }).click();
    await expect(page.getByRole('dialog')).toBeVisible();
    await page.getByRole('spinbutton', { name: 'Width', exact: true }).fill('16384');
    await page.getByRole('spinbutton', { name: 'Height', exact: true }).fill('16384');
    await page.getByRole('button', { name: 'Create document' }).click();
    await expect(page.getByTestId('document-size')).toHaveText('16384 × 16384');
    await expect.poll(async () => (await snap(page)).memory.document).toBe(0);
    await expect.poll(async () => (await snap(page)).document?.tiles).toBe(0);
  });
  test(backend + ' opens JPEG and WebP, drops files and cancels export', async ({ page }) => {
    await page.goto('/' + query);
    await expect(page.getByTestId('engine-status')).toHaveText('Engine ready');
    await page.getByTestId('open-image').setInputFiles(resolve(fixtures, 'sample.jpg'));
    await expect(page.getByTestId('operation-status')).toHaveText(/Opened in/);
    await expect(page.getByTestId('document-name')).toHaveText('sample.jpg');
    const bytes = [...(await readFile(resolve(fixtures, 'sample.webp')))];
    const data = await page.evaluateHandle((bytes) => {
      const dt = new DataTransfer();
      dt.items.add(new File([new Uint8Array(bytes)], 'dropped.webp', { type: 'image/webp' }));
      return dt;
    }, bytes);
    await page.locator('.canvas-area').dispatchEvent('drop', { dataTransfer: data });
    await data.dispose();
    await expect(page.getByTestId('document-name')).toHaveText('dropped.webp');
    await page.getByRole('button', { name: 'New canvas', exact: true }).click();
    await page.getByRole('combobox').selectOption('Photo · 24 MP');
    await page.getByRole('button', { name: 'Create document' }).click();
    await expect(page.getByTestId('document-size')).toHaveText('6000 × 4000');
    await page.getByRole('button', { name: 'Export PNG' }).click();
    await page.getByRole('button', { name: 'Cancel operation' }).click();
    await expect(page.getByTestId('operation-status')).toHaveText('Operation cancelled');
    await expect.poll(async () => (await snap(page)).memory.temporary).toBe(0);
    await expect(page.getByTestId('engine-status')).toHaveText('Engine ready');
  });
  test(
    backend + ' renders a large image, culls tiles and frees replaced residency',
    async ({ page }, info) => {
      await page.goto('/' + query);
      await expect(page.getByTestId('engine-status')).toHaveText('Engine ready');
      await page.getByTestId('open-image').setInputFiles(resolve(fixtures, 'large.png'));
      await expect(page.getByTestId('operation-status')).toHaveText(/Opened in/);
      await expect(page.getByTestId('document-size')).toHaveText('6000 × 4000');
      await expect.poll(async () => (await snap(page)).visibleTiles).toBe(384);
      const fit = await snap(page);
      expect(fit.document?.tiles).toBe(384);
      expect(fit.memory.document).toBe(384 * 256 * 256 * 4);
      await page.getByRole('button', { name: 'Actual pixels', exact: true }).click();
      await expect.poll(async () => (await snap(page)).visibleTiles).toBeLessThan(30);
      expect((await snap(page)).memory.gpu).toBeLessThanOrEqual(128 * 1048576);
      await info.attach('large-image', {
        body: JSON.stringify({
          status: await page.getByTestId('operation-status').textContent(),
          fit,
          actual: await snap(page),
        }),
        contentType: 'application/json',
      });
      await page.getByRole('button', { name: 'New canvas', exact: true }).click();
      await page.getByRole('button', { name: 'Create document' }).click();
      await expect.poll(async () => (await snap(page)).memory.document).toBe(0);
      await expect.poll(async () => (await snap(page)).memory.gpu).toBeLessThan(1024);
    },
  );
}
