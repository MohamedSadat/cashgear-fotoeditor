import { test, expect } from '@playwright/test';
import type { Page } from '@playwright/test';
import type { DocumentSnapshot, EngineApi, GeometryAction } from '../../apps/web/src/engine/types';
import { PNG } from 'pngjs';
import { readFile } from 'node:fs/promises';
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
const action = (page: Page, a: GeometryAction) =>
  whenIdle(page, () =>
    page.evaluate((a) => (Reflect.get(window, '__fotoeditor') as EngineApi).geometryAction(a), a),
  );
const jump = (page: Page, n: number) =>
  whenIdle(page, () =>
    page.evaluate((n) => (Reflect.get(window, '__fotoeditor') as EngineApi).historyJump(n), n),
  );
for (const backend of ['WebGPU', 'WebGL2']) {
  const query = '/?diagnostic=1&document=1' + (backend === 'WebGL2' ? '&gpu=webgl2' : '');
  test(`${backend} rotates exact alpha samples and resizes through canvas controls`, async ({
    page,
  }, info) => {
    await page.goto(query);
    await expect(page.getByTestId('engine-status')).toHaveText('Engine ready');
    await expect(page.getByTestId('gpu-backend')).toHaveText(backend);
    await page.getByTestId('open-image').setInputFiles(resolve('bench/results/fixtures/alpha.png'));
    await expect(page.getByTestId('operation-status')).toHaveText(/Opened in/);
    const original = PNG.sync.read(await readFile(resolve('bench/results/fixtures/alpha.png')));
    await page.getByRole('button', { name: 'Show canvas section' }).click();
    await page.getByText('Rotate and flip canvas', { exact: true }).click();
    await page.getByRole('button', { name: '90°', exact: true }).click();
    await expect.poll(async () => (await state(page)).document?.width).toBe(original.height);
    const rotated = await exported(page);
    expect([rotated.width, rotated.height]).toEqual([original.height, original.width]);
    const expected = Buffer.alloc(rotated.data.length);
    for (let y = 0; y < original.height; y++)
      for (let x = 0; x < original.width; x++)
        original.data.copy(
          expected,
          (x * rotated.width + original.height - 1 - y) * 4,
          (y * original.width + x) * 4,
          (y * original.width + x + 1) * 4,
        );
    expect(rotated.data).toEqual(expected);
    await jump(page, 0);
    expect((await exported(page)).data).toEqual(original.data);
    await page.getByText('Image and canvas size', { exact: true }).click();
    await expect(page.getByLabel('Resize interpolation')).toHaveValue('lanczos3');
    await page.getByLabel('Resize interpolation').selectOption('nearest');
    await page.getByLabel('New width', { exact: true }).fill(String(original.width * 2));
    await expect(page.getByLabel('New height', { exact: true })).toHaveValue(
      String(original.height * 2),
    );
    await page.getByLabel('Resize DPI').fill('300');
    await page.getByRole('button', { name: 'Resize image', exact: true }).click();
    await expect.poll(async () => (await state(page)).document?.width).toBe(original.width * 2);
    expect((await state(page)).document?.dpi).toBe(300);
    await action(page, {
      kind: 'image-size',
      width: original.width,
      height: original.height,
      dpi: 72,
      interpolation: 'nearest',
    });
    expect((await exported(page)).data).toEqual(original.data);
    await info.attach('geometry-timing', {
      body: JSON.stringify((await state(page)).geometryTiming),
      contentType: 'application/json',
    });
    await page.screenshot({
      path: resolve(`bench/results/p7-size-${info.project.name}-${backend}.png`),
    });
  });
  test(`${backend} grows cropped cold projects and preserves outside pixels with exact undo`, async ({
    page,
  }) => {
    await page.goto(query);
    await expect(page.getByTestId('engine-status')).toHaveText('Engine ready');
    await page.getByTestId('open-image').setInputFiles(resolve('bench/results/fixtures/alpha.png'));
    await expect(page.getByTestId('operation-status')).toHaveText(/Opened in/);
    const original = await exported(page);
    await whenIdle(page, () =>
      page.evaluate(async () => {
        const api = Reflect.get(window, '__fotoeditor') as EngineApi;
        await api.geometryMetadata({
          kind: 'crop',
          rect: { x: 100, y: 40, width: 200, height: 150 },
        });
        await api.geometryMetadata({
          kind: 'guides',
          guides: [{ axis: 'vertical', position: 125 }],
        });
        const root = await navigator.storage.getDirectory();
        const h = await root.getFileHandle('p7-size.cgfoto', { create: true });
        await api.saveProjectFile(h);
        await api.openProjectFile(await h.getFile());
      }),
    );
    const resized = await action(page, {
      kind: 'canvas-size',
      width: 613,
      height: 359,
      anchor: [0.5, 0.5],
    });
    expect(resized.document).toMatchObject({
      width: 613,
      height: 359,
      originX: 0,
      originY: 0,
      retainedWidth: 620,
      retainedHeight: 359,
      guides: [{ axis: 'vertical', position: 232 }],
    });
    const grown = await exported(page);
    await whenIdle(page, () =>
      page.evaluate(() =>
        (Reflect.get(window, '__fotoeditor') as EngineApi).geometryMetadata({
          kind: 'crop',
          rect: null,
        }),
      ),
    );
    const retained = await exported(page);
    expect([retained.width, retained.height]).toEqual([620, 359]);
    for (let y = 0; y < original.height; y++)
      expect(
        retained.data.subarray(
          ((y + 65) * retained.width + 107) * 4,
          ((y + 65) * retained.width + 107 + original.width) * 4,
        ),
      ).toEqual(original.data.subarray(y * original.width * 4, (y + 1) * original.width * 4));
    await jump(page, 0);
    expect((await state(page)).document).toMatchObject({
      width: 200,
      height: 150,
      originX: 100,
      originY: 40,
      retainedWidth: 513,
      retainedHeight: 259,
    });
    await jump(page, 1);
    expect((await exported(page)).data).toEqual(grown.data);
    await action(page, { kind: 'canvas-flip', horizontal: true });
    await action(page, { kind: 'canvas-flip', horizontal: true });
    expect((await exported(page)).data).toEqual(grown.data);
  });
  test(`${backend} refuses invalid or over-budget geometry and cancels without losing redo`, async ({
    page,
  }) => {
    await page.goto(query);
    await expect(page.getByTestId('engine-status')).toHaveText('Engine ready');
    await whenIdle(page, () =>
      page.evaluate(async () => {
        const api = Reflect.get(window, '__fotoeditor') as EngineApi;
        await api.newDocument(520, 259, 72);
        await api.selectionAction({ kind: 'fill', color: [19, 87, 33, 1] });
        await api.selectionAction({ kind: 'all' });
        await api.historyJump(1);
      }),
    );
    const before = await exported(page);
    const errors = await whenIdle(page, () =>
      page.evaluate(async () => {
        const api = Reflect.get(window, '__fotoeditor') as EngineApi;
        const errors: string[] = [];
        for (const fault of ['invalid', 'budget', 'budget-after-step'] as const) {
          if (fault !== 'invalid') await api.diagnosticAdjustmentFault(fault);
          try {
            await api.geometryAction({
              kind: 'image-size',
              width: fault === 'invalid' ? 0 : 1040,
              height: 518,
              dpi: 72,
              interpolation: 'lanczos3',
            });
          } catch (e) {
            errors.push(String(e));
          }
        }
        await api.diagnosticAdjustmentFault('pause-before-commit');
        Reflect.set(
          window,
          'pendingGeometry',
          api
            .geometryAction({ kind: 'canvas-rotate', degrees: 17, interpolation: 'lanczos3' })
            .then(
              () => '',
              (e) => String(e),
            ),
        );
        return errors;
      }),
    );
    expect(errors).toHaveLength(3);
    expect(errors[0]).toMatch(/Canvas sides/);
    for (const e of errors.slice(1)) expect(e).toMatch(/budget|768 MiB/);
    await expect(page.getByTestId('operation-status')).toHaveText(/Geometry ready/);
    const cancelled = await page.evaluate(async () => ({
      cancelled: await (Reflect.get(window, '__fotoeditor') as EngineApi).cancelJob(),
      error: await Reflect.get(window, 'pendingGeometry'),
    }));
    expect(cancelled.cancelled).toBe(true);
    expect(cancelled.error).toMatch(/cancelled/);
    expect((await exported(page)).data).toEqual(before.data);
    expect((await state(page)).history).toMatchObject({ undo: 1, redo: 1 });
  });
}
