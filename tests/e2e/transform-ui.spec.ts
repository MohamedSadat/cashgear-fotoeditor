import { test, expect } from '@playwright/test';
import type { Page } from '@playwright/test';
import type { DocumentSnapshot, EngineApi } from '../../apps/web/src/engine/types';
import { PNG } from 'pngjs';
import { resolve } from 'node:path';
import { whenIdle } from './idle';
const state = async (page: Page) =>
  JSON.parse(
    (await page.getByTestId('gpu-canvas').getAttribute('data-snapshot'))!,
  ) as DocumentSnapshot;
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
const pixel = (p: PNG, x: number, y: number) =>
  Array.from(p.data.subarray((y * p.width + x) * 4, (y * p.width + x + 1) * 4));
async function ready(page: Page, backend: string) {
  await page.goto('/?diagnostic=1&document=1' + (backend === 'WebGL2' ? '&gpu=webgl2' : ''));
  await expect(page.getByTestId('engine-status')).toHaveText('Engine ready');
  await expect(page.getByTestId('gpu-backend')).toHaveText(backend);
  const baseline = await whenIdle(page, () =>
    page.evaluate(async () => {
      const api = Reflect.get(window, '__fotoeditor') as EngineApi;
      await api.newDocument(64, 48, 72);
      return api.selectionAction({ kind: 'fill', color: [200, 40, 80, 128] });
    }),
  );
  await expect
    .poll(async () => (await state(page))?.contentRevision)
    .toBe(baseline.contentRevision);
  await expect.poll(async () => (await state(page))?.document?.width).toBe(64);
  return baseline;
}
for (const backend of ['WebGPU', 'WebGL2']) {
  test(`${backend} commits transform controls with Enter and nudges by one or ten pixels`, async ({
    page,
  }) => {
    const baseline = await ready(page, backend);
    const before = await exported(page);
    const canvas = page.getByTestId('gpu-canvas');
    await canvas.focus();
    await page.keyboard.press('Control+t');
    await expect(page.getByTestId('transform-status')).toHaveText('Transform preview ready');
    await expect(page.locator('[data-testid="transform-overlay"] circle')).toHaveCount(9);
    await page.getByLabel('Rotate by (degrees)').fill('10');
    await page.getByRole('button', { name: 'Rotate transform', exact: true }).click();
    await expect(page.getByTestId('transform-status')).toHaveText('Transform preview ready');
    await page.getByLabel('Rotate by (degrees)').fill('-10');
    await page.getByRole('button', { name: 'Rotate transform', exact: true }).click();
    await expect(page.getByTestId('transform-status')).toHaveText('Transform preview ready');
    expect((await exported(page)).data).toEqual(before.data);
    await canvas.focus();
    await page.keyboard.press('Enter');
    await expect.poll(async () => (await state(page)).transform).toBeNull();
    expect((await state(page)).history.undo).toBe(baseline.history.undo);
    expect((await exported(page)).data).toEqual(before.data);
    await canvas.focus();
    await page.keyboard.press('v');
    await page.keyboard.press('ArrowRight');
    await expect.poll(async () => (await state(page)).history.undo).toBe(baseline.history.undo + 1);
    await page.keyboard.press('Shift+ArrowDown');
    await expect.poll(async () => (await state(page)).history.undo).toBe(baseline.history.undo + 2);
    const moved = await exported(page);
    expect(pixel(moved, 0, 10)).toEqual([0, 0, 0, 0]);
    expect(pixel(moved, 1, 9)).toEqual([0, 0, 0, 0]);
    expect(pixel(moved, 1, 10)).toEqual([200, 40, 80, 128]);
  });
  test(`${backend} drags transform handles with Shift and cancels without editing pixels`, async ({
    page,
  }, info) => {
    const baseline = await ready(page, backend);
    const before = await exported(page);
    const canvas = page.getByTestId('gpu-canvas');
    await canvas.focus();
    await page.keyboard.press('Control+t');
    await expect(page.getByTestId('transform-status')).toHaveText('Transform preview ready');
    const handle = page.locator('[data-testid="transform-overlay"] [data-handle="2"]');
    const box = await handle.boundingBox();
    expect(box).not.toBeNull();
    await page.keyboard.down('Shift');
    await page.mouse.move(box!.x + box!.width / 2, box!.y + box!.height / 2);
    await page.mouse.down();
    await page.mouse.move(box!.x + box!.width / 2 + 60, box!.y + box!.height / 2 + 20, {
      steps: 5,
    });
    await page.mouse.up();
    await page.keyboard.up('Shift');
    await expect(page.getByTestId('transform-status')).toHaveText('Transform preview ready');
    const q = (await state(page)).transform!.quad;
    const width = Math.hypot(q[1][0] - q[0][0], q[1][1] - q[0][1]),
      height = Math.hypot(q[3][0] - q[0][0], q[3][1] - q[0][1]);
    expect(Math.abs(width / 64 - height / 48)).toBeLessThan(1e-6);
    expect(width).toBeGreaterThan(64);
    expect((await exported(page)).data).toEqual(before.data);
    expect((await state(page)).history.undo).toBe(baseline.history.undo);
    await page.screenshot({
      path: resolve(`bench/results/p7-transform-handles-${info.project.name}-${backend}.png`),
    });
    await canvas.focus();
    await page.keyboard.press('Escape');
    await expect.poll(async () => (await state(page)).transform).toBeNull();
    expect((await exported(page)).data).toEqual(before.data);
  });
  test(`${backend} move dragging applies one command and preserves the current view`, async ({
    page,
  }) => {
    const baseline = await ready(page, backend),
      s = await state(page);
    await page.getByRole('button', { name: 'Move', exact: true }).click();
    const box = await page.getByTestId('gpu-canvas').boundingBox();
    expect(box).not.toBeNull();
    const x = box!.x + box!.width / 2,
      y = box!.y + box!.height / 2;
    await page.mouse.move(x, y);
    await page.mouse.down();
    await page.mouse.move(x + 5 * s.view.zoom, y + 3 * s.view.zoom, { steps: 3 });
    await page.mouse.up();
    await expect.poll(async () => (await state(page)).history.undo).toBe(baseline.history.undo + 1);
    await expect.poll(async () => (await state(page)).transform).toBeNull();
    const moved = await exported(page);
    expect(pixel(moved, 4, 3)).toEqual([0, 0, 0, 0]);
    expect(pixel(moved, 5, 2)).toEqual([0, 0, 0, 0]);
    expect(pixel(moved, 5, 3)).toEqual([200, 40, 80, 128]);
    expect((await state(page)).view.zoom).toBe(s.view.zoom);
  });
  test(`${backend} auto-selects rendered layers, snaps movement and edits skew and perspective handles`, async ({
    page,
  }) => {
    await ready(page, backend);
    const ids = await whenIdle(page, () =>
      page.evaluate(async () => {
        const api = Reflect.get(window, '__fotoeditor') as EngineApi;
        const added = await api.layerAction({
            kind: 'add',
            name: 'Top',
            parent: null,
            index: 1,
            group: false,
          }),
          bottom = added.layers.nodes[0].id,
          top = added.layers.nodes[1].id;
        await api.selectLayer(top);
        await api.selectionAction({
          kind: 'shape',
          shape: { kind: 'rectangle', x0: 20, y0: 8, x1: 44, y1: 40 },
          mode: 'new',
          antialias: false,
        });
        await api.selectionAction({ kind: 'fill', color: [10, 220, 50, 255] });
        await api.selectionAction({ kind: 'none' });
        await api.geometryMetadata({
          kind: 'guides',
          guides: [{ axis: 'vertical', position: 22 }],
        });
        await api.selectLayer(bottom);
        return { bottom, top };
      }),
    );
    const hits = await whenIdle(page, () =>
      page.evaluate(async () => {
        const api = Reflect.get(window, '__fotoeditor') as EngineApi;
        return [await api.hitTest(30, 20), await api.hitTest(5, 5), await api.hitTest(-1, 0)];
      }),
    );
    expect(hits).toEqual([ids.top, ids.bottom, null]);
    await page.getByRole('checkbox', { name: 'Auto-select layer', exact: true }).check();
    await page.getByRole('button', { name: 'Move', exact: true }).click();
    await expect.poll(async () => (await state(page)).layers.active).toBe(ids.bottom);
    const s = await state(page),
      box = (await page.getByTestId('gpu-canvas').boundingBox())!;
    const x = box.x + s.view.screenWidth / 2 + (30 - 32) * s.view.zoom,
      y = box.y + s.view.screenHeight / 2 + (20 - 24) * s.view.zoom;
    await page.mouse.move(x, y);
    await page.mouse.down();
    await page.mouse.move(x + 2 * s.view.zoom, y, { steps: 3 });
    await page.mouse.up();
    await expect.poll(async () => (await state(page)).layers.active).toBe(ids.top);
    await expect.poll(async () => (await state(page)).transform).toBeNull();
    const p = await exported(page);
    expect(pixel(p, 21, 20)).toEqual([200, 40, 80, 128]);
    expect(pixel(p, 22, 20)).toEqual([10, 220, 50, 255]);
    await page.getByTestId('gpu-canvas').focus();
    await page.keyboard.press('Control+t');
    await expect(page.getByTestId('transform-status')).toHaveText('Transform preview ready');
    await expect.poll(async () => (await state(page)).transform?.bounds.width).toBe(24);
    await page.getByLabel('Transform handle mode').selectOption('skew');
    const h = page.locator('[data-testid="transform-overlay"] [data-handle="1"]'),
      hb = (await h.boundingBox())!;
    await page.mouse.move(hb.x + hb.width / 2, hb.y + hb.height / 2);
    await page.mouse.down();
    await page.mouse.move(hb.x + hb.width / 2 + 30, hb.y + hb.height / 2, { steps: 3 });
    await page.mouse.up();
    await expect(page.getByTestId('transform-status')).toHaveText('Transform preview ready');
    const skew = (await state(page)).transform!.matrix.values;
    expect(Math.abs(skew[1])).toBeGreaterThan(0.01);
    await page.getByLabel('Transform handle mode').selectOption('perspective');
    const corner = page.locator('[data-testid="transform-overlay"] [data-handle="0"]'),
      cb = (await corner.boundingBox())!;
    await page.mouse.move(cb.x + cb.width / 2, cb.y + cb.height / 2);
    await page.mouse.down();
    await page.mouse.move(cb.x + cb.width / 2 + 20, cb.y + cb.height / 2, { steps: 3 });
    await page.mouse.up();
    await expect(page.getByTestId('transform-status')).toHaveText('Transform preview ready');
    const perspective = (await state(page)).transform!.matrix.values;
    expect(Math.abs(perspective[6]) + Math.abs(perspective[7])).toBeGreaterThan(1e-6);
    await page.getByTestId('gpu-canvas').focus();
    await page.keyboard.press('Escape');
  });
}
