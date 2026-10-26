import { test, expect } from '@playwright/test';
import type { Page } from '@playwright/test';
import type { EngineApi, DocumentSnapshot } from '../../apps/web/src/engine/types';
import { PNG } from 'pngjs';
import { resolve } from 'node:path';
import { whenIdle } from './idle';
const state = async (page: Page) => {
  const canvas = page.getByTestId('gpu-canvas');
  await expect(canvas).toHaveAttribute('data-snapshot', /"view":/);
  return JSON.parse((await canvas.getAttribute('data-snapshot'))!) as DocumentSnapshot;
};
const exported = async (page: Page) =>
  PNG.sync.read(
    Buffer.from(
      await whenIdle(page, () =>
        page.evaluate(async () =>
          Array.from(await (Reflect.get(window, '__fotoeditor') as EngineApi).exportPng()),
        ),
      ),
    ),
  );
async function ready(page: Page, backend: string) {
  await page.goto('/?diagnostic=1&document=1' + (backend === 'WebGL2' ? '&gpu=webgl2' : ''));
  await expect(page.getByTestId('engine-status')).toHaveText('Engine ready');
  await expect(page.getByTestId('gpu-backend')).toHaveText(backend);
  const s = await whenIdle(page, () =>
    page.evaluate(async () => {
      const api = Reflect.get(window, '__fotoeditor') as EngineApi;
      await api.newDocument(64, 48, 72);
      return api.selectionAction({ kind: 'fill', color: [200, 40, 80, 128] });
    }),
  );
  await expect.poll(async () => (await state(page)).contentRevision).toBe(s.contentRevision);
  return s;
}
async function screen(page: Page, x: number, y: number) {
  const s = await state(page),
    d = s.document!,
    v = s.view,
    b = (await page.getByTestId('gpu-canvas').boundingBox())!;
  let px = x - (d.originX ?? 0) - d.width / 2;
  const py = y - (d.originY ?? 0) - d.height / 2;
  if (v.flipped) px = -px;
  return [
    b.x +
      v.screenWidth / 2 +
      v.panX +
      (px * Math.cos(v.rotation) - py * Math.sin(v.rotation)) * v.zoom,
    b.y +
      v.screenHeight / 2 +
      v.panY +
      (px * Math.sin(v.rotation) + py * Math.cos(v.rotation)) * v.zoom,
  ] as const;
}
async function draw(page: Page, from: [number, number], to: [number, number]) {
  const a = await screen(page, ...from),
    b = await screen(page, ...to);
  await page.mouse.move(...a);
  await page.mouse.down();
  await page.mouse.move(...b, { steps: 5 });
  await page.mouse.up();
}
for (const backend of ['WebGPU', 'WebGL2']) {
  test(`${backend} crops by ratio with thirds, cancels drafts and reveals exact retained samples`, async ({
    page,
  }, info) => {
    const baseline = await ready(page, backend),
      before = await exported(page);
    await page.getByTestId('gpu-canvas').focus();
    await page.keyboard.press('c');
    await page.getByLabel('Crop ratio', { exact: true }).selectOption('1');
    await draw(page, [10, 8], [38, 32]);
    await expect(page.getByLabel('Crop draft width', { exact: true })).toHaveValue('28');
    await expect(page.getByLabel('Crop draft height', { exact: true })).toHaveValue('28');
    await expect(page.locator('[data-testid="crop-overlay"] .crop-thirds')).toHaveCount(2);
    expect((await state(page)).history.undo).toBe(baseline.history.undo);
    await page.screenshot({
      path: resolve(`bench/results/p7-crop-${info.project.name}-${backend}.png`),
    });
    await page.getByTestId('gpu-canvas').focus();
    await page.keyboard.press('Escape');
    expect((await exported(page)).data).toEqual(before.data);
    await page.keyboard.press('c');
    await page.getByLabel('Crop ratio', { exact: true }).selectOption('free');
    await draw(page, [8, 6], [40, 30]);
    await page.getByTestId('gpu-canvas').focus();
    await page.keyboard.press('Enter');
    await expect.poll(async () => (await state(page)).document?.width).toBe(32);
    await expect.poll(async () => (await state(page)).document?.height).toBe(24);
    expect((await state(page)).history.undo).toBe(baseline.history.undo + 1);
    const p = await exported(page);
    expect([p.width, p.height]).toEqual([32, 24]);
    await whenIdle(page, () =>
      page.evaluate(() =>
        (Reflect.get(window, '__fotoeditor') as EngineApi).geometryMetadata({
          kind: 'crop',
          rect: null,
        }),
      ),
    );
    expect((await exported(page)).data).toEqual(before.data);
  });
  test(`${backend} crop handles and guide snapping follow rotated flipped document coordinates`, async ({
    page,
  }) => {
    await ready(page, backend);
    await whenIdle(page, () =>
      page.evaluate(async () => {
        const api = Reflect.get(window, '__fotoeditor') as EngineApi;
        await api.geometryMetadata({
          kind: 'guides',
          guides: [
            { axis: 'vertical', position: 20 },
            { axis: 'horizontal', position: 14 },
          ],
        });
        await api.interact([
          { kind: 'rotate', x: 0, y: 0, amount: 0.23 },
          { kind: 'flip', x: 0, y: 0, amount: 0 },
        ]);
      }),
    );
    await expect.poll(async () => (await state(page)).view.flipped).toBe(true);
    await expect.poll(async () => (await state(page)).view.rotation).toBe(0.23);
    await page.getByRole('button', { name: 'Crop', exact: true }).click();
    await page.getByLabel('Crop ratio', { exact: true }).selectOption('free');
    await draw(page, [38, 30], [19.8, 14.2]);
    for (const [name, value] of Object.entries({ x: 20, y: 14, width: 18, height: 16 }))
      await expect(page.getByLabel('Crop draft ' + name, { exact: true })).toHaveValue(
        String(value),
      );
    await draw(page, [38, 30], [42, 32]);
    await expect(page.getByLabel('Crop draft width', { exact: true })).toHaveValue('22');
    await expect(page.getByLabel('Crop draft height', { exact: true })).toHaveValue('18');
    await page.keyboard.down('Alt');
    await draw(page, [30, 24], [32, 25]);
    await page.keyboard.up('Alt');
    await expect(page.getByLabel('Crop draft x', { exact: true })).toHaveValue('22');
    await expect(page.getByLabel('Crop draft y', { exact: true })).toHaveValue('15');
    await page.keyboard.press('Escape');
  });
  test(`${backend} straightens from a drawn line, commits one crop command and undoes exactly`, async ({
    page,
  }) => {
    const baseline = await ready(page, backend),
      before = await exported(page);
    await page.getByRole('button', { name: 'Crop', exact: true }).click();
    await draw(page, [8, 8], [56, 40]);
    await page.getByRole('button', { name: 'Draw straighten line', exact: true }).click();
    await draw(page, [10, 20], [50, 24]);
    const angle = Number(
      await page.getByLabel('Crop straighten degrees', { exact: true }).inputValue(),
    );
    expect(Math.abs(angle + (Math.atan2(4, 40) * 180) / Math.PI)).toBeLessThan(0.001);
    await page.getByLabel('Crop interpolation', { exact: true }).selectOption('nearest');
    await page.getByRole('button', { name: 'Apply crop', exact: true }).click();
    await expect.poll(async () => (await state(page)).document?.width).toBe(48);
    await expect.poll(async () => (await state(page)).document?.height).toBe(32);
    expect((await state(page)).history.undo).toBe(baseline.history.undo + 1);
    const after = await exported(page);
    expect([after.width, after.height]).toEqual([48, 32]);
    await whenIdle(page, () =>
      page.evaluate(
        (n) => (Reflect.get(window, '__fotoeditor') as EngineApi).historyJump(n),
        baseline.history.undo,
      ),
    );
    expect((await exported(page)).data).toEqual(before.data);
  });
  test(`${backend} minified crop mips exclude retained red pixels at unaligned green edges`, async ({
    page,
  }) => {
    await ready(page, backend);
    await whenIdle(page, () =>
      page.evaluate(async () => {
        const api = Reflect.get(window, '__fotoeditor') as EngineApi;
        await api.newDocument(1024, 1024, 72);
        await api.selectionAction({ kind: 'fill', color: [255, 0, 0, 255] });
        await api.selectionAction({
          kind: 'shape',
          shape: { kind: 'rectangle', x0: 253, y0: 251, x1: 772, y1: 514 },
          mode: 'new',
          antialias: false,
        });
        await api.selectionAction({ kind: 'fill', color: [0, 255, 0, 255] });
        await api.selectionAction({ kind: 'none' });
        await api.geometryMetadata({
          kind: 'crop',
          rect: { x: 253, y: 251, width: 519, height: 263 },
        });
        await api.interact([{ kind: 'actual', x: 0, y: 0, amount: 0 }]);
      }),
    );
    const box = (await page.getByTestId('gpu-canvas').boundingBox())!;
    await whenIdle(page, () =>
      page.evaluate(
        ({ x, y }) =>
          (Reflect.get(window, '__fotoeditor') as EngineApi).interact([
            { kind: 'zoom', x, y, amount: 0.25 },
          ]),
        { x: box.width / 2, y: box.height / 2 },
      ),
    );
    await expect.poll(async () => (await state(page)).view.zoom).toBe(0.25);
    await expect.poll(async () => (await state(page)).refining).toBe(0);
    const p = PNG.sync.read(await page.getByTestId('gpu-canvas').screenshot()),
      left = p.width / 2 - 519 / 8,
      right = p.width / 2 + 519 / 8,
      top = p.height / 2 - 263 / 8,
      bottom = p.height / 2 + 263 / 8;
    for (const [x, y] of [
      [Math.ceil(left), Math.floor(p.height / 2)],
      [Math.floor(right) - 1, Math.floor(p.height / 2)],
      [Math.floor(p.width / 2), Math.ceil(top)],
      [Math.floor(p.width / 2), Math.floor(bottom) - 1],
    ])
      expect(Array.from(p.data.subarray((y * p.width + x) * 4, (y * p.width + x + 1) * 4))).toEqual(
        [0, 255, 0, 255],
      );
  });
}
