import { test, expect } from '@playwright/test';
import type { Page } from '@playwright/test';
import type { EngineApi, DocumentSnapshot } from '../../apps/web/src/engine/types';
import { PNG } from 'pngjs';
import { whenIdle } from './idle';
import { resolve } from 'node:path';
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
for (const backend of ['WebGPU', 'WebGL2']) {
  const query = '/?diagnostic=1&document=1' + (backend === 'WebGL2' ? '&gpu=webgl2' : '');
  test(`${backend} masked layer GPU transform matches committed pixels after a rotated flipped view rebuild`, async ({
    page,
  }, info) => {
    await page.goto(query);
    await expect(page.getByTestId('engine-status')).toHaveText('Engine ready');
    // Canvas screenshots include overlapping DOM handles; compare GPU pixels only.
    await page.addStyleTag({ content: '.transform-overlay { visibility: hidden; }' });
    const target = await whenIdle(page, () =>
      page.evaluate(async () => {
        const api = Reflect.get(window, '__fotoeditor') as EngineApi;
        const s = await api.newDocument(96, 64, 72),
          target = s.layers.nodes[0].id;
        await api.selectionAction({ kind: 'fill', color: [200, 40, 80, 255] });
        await api.layerAction({ kind: 'mask-add', target });
        await api.selectLayer(target, true);
        await api.selectionAction({
          kind: 'shape',
          shape: { kind: 'rectangle', x0: 32, y0: 0, x1: 64, y1: 64 },
          mode: 'new',
          antialias: false,
        });
        await api.selectionAction({ kind: 'fill', color: [128, 128, 128, 255] });
        await api.selectionAction({ kind: 'none' });
        await api.selectLayer(target, false);
        await api.layerAction({ kind: 'group', ids: [target], name: 'Transformed child' });
        await api.interact([{ kind: 'actual', x: 0, y: 0, amount: 0 }]);
        await api.beginTransform(target, false);
        return target;
      }),
    );
    expect(target).toBeGreaterThan(0);
    const update = { kind: 'matrix', matrix: { values: [1, 0, 7, 0, 1, 3, 0, 0, 1] } } as const;
    await whenIdle(page, () =>
      page.evaluate(
        (u) => (Reflect.get(window, '__fotoeditor') as EngineApi).previewTransform(u),
        update,
      ),
    );
    await whenIdle(page, () =>
      page.evaluate(async () => {
        const api = Reflect.get(window, '__fotoeditor') as EngineApi;
        await api.interact([
          { kind: 'rotate', x: 0, y: 0, amount: 0.37 },
          { kind: 'flip', x: 0, y: 0, amount: 0 },
        ]);
      }),
    );
    await whenIdle(page, () =>
      page.evaluate(
        (u) => (Reflect.get(window, '__fotoeditor') as EngineApi).previewTransform(u),
        update,
      ),
    );
    await page.evaluate(
      () => new Promise<void>((r) => requestAnimationFrame(() => requestAnimationFrame(() => r()))),
    );
    const before = PNG.sync.read(
      await page.getByTestId('gpu-canvas').screenshot({
        path: resolve(`bench/results/p7-masked-${info.project.name}-${backend}-before.png`),
      }),
    );
    const applied = await whenIdle(page, () =>
      page.evaluate(() => (Reflect.get(window, '__fotoeditor') as EngineApi).applyTransform()),
    );
    await expect
      .poll(async () => (await state(page)).contentRevision)
      .toBe(applied.contentRevision);
    await expect.poll(async () => (await state(page)).refining).toBe(0);
    await page.evaluate(
      () => new Promise<void>((r) => requestAnimationFrame(() => requestAnimationFrame(() => r()))),
    );
    const after = PNG.sync.read(
      await page.getByTestId('gpu-canvas').screenshot({
        path: resolve(`bench/results/p7-masked-${info.project.name}-${backend}-after.png`),
      }),
    );
    expect(after.data.equals(before.data)).toBe(true);
  });
  test(`${backend} transform cancellation, budget refusal and invalid matrices preserve redo and samples`, async ({
    page,
  }) => {
    await page.goto(query);
    await expect(page.getByTestId('engine-status')).toHaveText('Engine ready');
    const baseline = await whenIdle(page, () =>
      page.evaluate(async () => {
        const api = Reflect.get(window, '__fotoeditor') as EngineApi;
        const s = await api.newDocument(513, 259, 72),
          target = s.layers.nodes[0].id;
        const filled = await api.selectionAction({ kind: 'fill', color: [200, 40, 80, 128] });
        await api.geometryAction({ kind: 'canvas-flip', horizontal: true });
        await api.historyJump(filled.history.undo);
        return { target, undo: filled.history.undo };
      }),
    );
    const before = await exported(page);
    const errors = await whenIdle(page, () =>
      page.evaluate(async ({ target }) => {
        const api = Reflect.get(window, '__fotoeditor') as EngineApi,
          errors: string[] = [];
        await api.beginTransform(target, false);
        for (const values of [
          [0, 0, 0, 0, 0, 0, 0, 0, 1],
          [1, 0, 0, 0, 1, 0, -0.1, 0, 1],
        ]) {
          try {
            await api.previewTransform({ kind: 'matrix', matrix: { values } });
          } catch (e) {
            errors.push(String(e));
          }
        }
        await api.diagnosticPreviewFault('budget');
        try {
          await api.previewTransform({
            kind: 'matrix',
            matrix: { values: [1, 0, 4, 0, 1, 0, 0, 0, 1] },
          });
        } catch (e) {
          errors.push(String(e));
        }
        await api.cancelTransform();
        await api.beginTransform(target, false);
        await api.previewTransform({
          kind: 'matrix',
          matrix: { values: [1, 0, 4, 0, 1, 0, 0, 0, 1] },
        });
        await api.diagnosticAdjustmentFault('budget-after-step');
        try {
          await api.applyTransform();
        } catch (e) {
          errors.push(String(e));
        }
        return errors;
      }, baseline),
    );
    expect(errors[0]).toMatch(/singular/);
    expect(errors[1]).toMatch(/horizon/);
    expect(errors[2]).toMatch(/budget/i);
    expect(errors[3]).toMatch(/budget/i);
    await whenIdle(page, () =>
      page.evaluate(async ({ target }) => {
        const api = Reflect.get(window, '__fotoeditor') as EngineApi;
        await api.beginTransform(target, false);
        await api.diagnosticPreviewFault('pause-before-render');
        Reflect.set(
          window,
          'pausedTransform',
          api
            .previewTransform({ kind: 'matrix', matrix: { values: [1, 0, 4, 0, 1, 0, 0, 0, 1] } })
            .then(
              () => '',
              (e) => String(e),
            ),
        );
      }, baseline),
    );
    await expect
      .poll(async () => (await state(page)).operation)
      .toContain('Transform preview ready');
    await page.evaluate(async () => {
      await (Reflect.get(window, '__fotoeditor') as EngineApi).cancelTransform();
    });
    expect(await page.evaluate(async () => Reflect.get(window, 'pausedTransform'))).toContain(
      'cancelled',
    );
    expect((await exported(page)).data).toEqual(before.data);
    expect((await state(page)).history.undo).toBe(baseline.undo);
    expect((await state(page)).history.redo).toBe(1);
  });
  test(`${backend} previews immutable transform sessions and commits inverse rotations without resampling`, async ({
    page,
  }, info) => {
    await page.goto(query);
    await expect(page.getByTestId('engine-status')).toHaveText('Engine ready');
    await expect(page.getByTestId('gpu-backend')).toHaveText(backend);
    const baseline = await whenIdle(page, () =>
      page.evaluate(async () => {
        const api = Reflect.get(window, '__fotoeditor') as EngineApi;
        await api.newDocument(64, 48, 72);
        return api.selectionAction({ kind: 'fill', color: [200, 40, 80, 128] });
      }),
    );
    const before = await exported(page);
    const target = baseline.layers.nodes[0].id;
    await whenIdle(page, () =>
      page.evaluate(
        (target) =>
          (Reflect.get(window, '__fotoeditor') as EngineApi).beginTransform(target, false),
        target,
      ),
    );
    const timings = await whenIdle(page, () =>
      page.evaluate(async () => {
        const api = Reflect.get(window, '__fotoeditor') as EngineApi;
        const cold = await api.previewTransform({
          kind: 'matrix',
          matrix: { values: [1, 0, 0, 0, 1, 0, 0, 0, 1] },
        });
        const a = (10 * Math.PI) / 180,
          c = Math.cos(a),
          s = Math.sin(a);
        const pivot = [32, 24];
        const matrix = (c: number, s: number) => ({
          values: [
            c,
            -s,
            pivot[0] - c * pivot[0] + s * pivot[1],
            s,
            c,
            pivot[1] - s * pivot[0] - c * pivot[1],
            0,
            0,
            1,
          ],
        });
        const rotated = await api.previewTransform({ kind: 'compose', matrix: matrix(c, s) });
        const inverse = await api.previewTransform({ kind: 'compose', matrix: matrix(c, -s) });
        return { cold, rotated, inverse };
      }),
    );
    expect((await exported(page)).data).toEqual(before.data);
    expect((await state(page)).history.undo).toBe(baseline.history.undo);
    expect((await state(page)).transform).toMatchObject({
      target,
      interpolation: 'lanczos3',
      selected: false,
    });
    const applied = await whenIdle(page, () =>
      page.evaluate(() => (Reflect.get(window, '__fotoeditor') as EngineApi).applyTransform()),
    );
    expect(applied.history.undo).toBe(baseline.history.undo);
    expect((await exported(page)).data).toEqual(before.data);
    await expect.poll(async () => (await state(page)).transform).toBeNull();
    await info.attach('transform-preview-timing', {
      body: JSON.stringify(timings),
      contentType: 'application/json',
    });
  });
  test(`${backend} transforms selected fractional coverage from cold projects and undoes exactly`, async ({
    page,
  }) => {
    await page.goto(query);
    await expect(page.getByTestId('engine-status')).toHaveText('Engine ready');
    const baseline = await whenIdle(page, () =>
      page.evaluate(async () => {
        const api = Reflect.get(window, '__fotoeditor') as EngineApi;
        await api.newDocument(64, 32, 72);
        await api.selectionAction({ kind: 'fill', color: [200, 40, 80, 128] });
        await api.selectionAction({
          kind: 'shape',
          shape: { kind: 'rectangle', x0: 8.5, y0: 0, x1: 16.5, y1: 32 },
          mode: 'new',
          antialias: true,
        });
        const dir = await navigator.storage.getDirectory(),
          h = await dir.getFileHandle('p7-transform.cgfoto', { create: true });
        await api.saveProjectFile(h);
        await api.newDocument(1, 1, 72);
        await api.openProjectFile(await h.getFile());
        return api.selectionAction({
          kind: 'shape',
          shape: { kind: 'rectangle', x0: 8.5, y0: 0, x1: 16.5, y1: 32 },
          mode: 'new',
          antialias: true,
        });
      }),
    );
    const before = await exported(page);
    const target = baseline.layers.nodes[0].id;
    await whenIdle(page, () =>
      page.evaluate(
        (target) => (Reflect.get(window, '__fotoeditor') as EngineApi).beginTransform(target, true),
        target,
      ),
    );
    await whenIdle(page, () =>
      page.evaluate(() =>
        (Reflect.get(window, '__fotoeditor') as EngineApi).previewTransform({
          kind: 'matrix',
          matrix: { values: [1, 0, 8, 0, 1, 0, 0, 0, 1] },
        }),
      ),
    );
    expect((await exported(page)).data).toEqual(before.data);
    const applied = await whenIdle(page, () =>
      page.evaluate(() => (Reflect.get(window, '__fotoeditor') as EngineApi).applyTransform()),
    );
    expect(applied.history.undo).toBe(baseline.history.undo + 1);
    const final = await exported(page);
    const pixel = (x: number) =>
      Array.from(final.data.subarray((10 * final.width + x) * 4, (10 * final.width + x + 1) * 4));
    expect(pixel(8)).toEqual([200, 40, 80, 64]);
    expect(pixel(9)).toEqual([200, 40, 80, 0]);
    expect(pixel(16)).toEqual([200, 40, 80, 112]);
    expect(pixel(24)).toEqual([200, 40, 80, 160]);
    await whenIdle(page, () =>
      page.evaluate(
        (n) => (Reflect.get(window, '__fotoeditor') as EngineApi).historyJump(n),
        baseline.history.undo,
      ),
    );
    expect((await exported(page)).data).toEqual(before.data);
    await whenIdle(page, () =>
      page.evaluate(
        (n) => (Reflect.get(window, '__fotoeditor') as EngineApi).historyJump(n),
        baseline.history.undo + 1,
      ),
    );
    expect((await exported(page)).data).toEqual(final.data);
  });
}
