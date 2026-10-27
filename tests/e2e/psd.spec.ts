import { test, expect } from '@playwright/test';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { PNG } from 'pngjs';
import type { EngineApi, DocumentSnapshot, LayerNode } from '../../apps/web/src/engine/types';
import { whenIdle } from './idle';
const root = resolve('tests/fixtures/psd');
const corpus = JSON.parse(readFileSync(resolve(root, 'oracle.json'), 'utf8')) as {
  fixtures: {
    file: string;
    width: number;
    height: number;
    canvas: { x: number; y: number; width: number; height: number };
    compositeReference: string;
    layers: {
      name: string;
      visible: boolean;
      opacity: number;
      blend: string;
      parentIndex: number | null;
      mask?: { enabled: boolean };
    }[];
  }[];
};
const state = async (page: import('@playwright/test').Page) =>
  JSON.parse(
    (await page.getByTestId('gpu-canvas').getAttribute('data-snapshot'))!,
  ) as DocumentSnapshot;
const exported = (page: import('@playwright/test').Page) =>
  whenIdle(page, () =>
    page.evaluate(async () =>
      Array.from(await (Reflect.get(window, '__fotoeditor') as EngineApi).exportPng()),
    ),
  );

for (const backend of ['WebGPU', 'WebGL2']) {
  for (const fixture of corpus.fixtures) {
    test(
      backend + ' imports public PSD channels and Photoshop composite: ' + fixture.file,
      async ({ page }) => {
        await page.goto('/?diagnostic=1&document=1' + (backend === 'WebGL2' ? '&gpu=webgl2' : ''));
        await expect(page.getByTestId('engine-status')).toHaveText('Engine ready');
        await page.getByTestId('open-image').setInputFiles(resolve(root, fixture.file));
        await expect(page.getByTestId('operation-status')).toHaveText(/Opened in/);
        const snapshot = await state(page);
        expect(snapshot.editError).toBeFalsy();
        expect(snapshot.document).toMatchObject({
          width: fixture.canvas.width,
          height: fixture.canvas.height,
        });
        expect(snapshot.history.undo).toBe(0);
        const nodes: { node: LayerNode; parent: number | null }[] = [];
        const flatten = (layers: LayerNode[], parent: number | null = null) => {
          for (const node of layers) {
            const index = nodes.length;
            nodes.push({ node, parent });
            if (node.children) flatten(node.children, index);
          }
        };
        flatten(snapshot.layers.nodes);
        expect(nodes).toHaveLength(fixture.layers.length);
        for (const [index, { node, parent }] of nodes.entries()) {
          const expected = fixture.layers[index];
          expect(node.name).toBe(expected.name);
          expect(parent).toBe(expected.parentIndex);
          expect(node.properties.visible).toBe(expected.visible);
          expect(node.properties.opacity).toBe(expected.opacity / 255);
          expect(node.properties.passThrough).toBe(expected.blend === 'pass');
          expect(node.mask).toEqual(expected.mask ? { enabled: expected.mask.enabled } : null);
        }
        const actual = PNG.sync.read(Buffer.from(await exported(page)));
        const reference = PNG.sync.read(readFileSync(resolve(root, fixture.compositeReference)));
        expect([actual.width, actual.height]).toEqual([reference.width, reference.height]);
        const maximum = [0, 0, 0, 0];
        for (let at = 0; at < actual.data.length; at += 4) {
          const alpha = actual.data[at + 3];
          for (let channel = 0; channel < 3; channel++) {
            const white = Math.floor(
              (actual.data[at + channel] * alpha + 255 * (255 - alpha) + 127) / 255,
            );
            maximum[channel] = Math.max(
              maximum[channel],
              Math.abs(white - reference.data[at + channel]),
            );
          }
          maximum[3] = Math.max(maximum[3], Math.abs(alpha - reference.data[at + 3]));
        }
        expect(
          maximum.slice(0, 3).every((n) => n <= 2),
          JSON.stringify(maximum),
        ).toBe(true);
        expect(maximum[3]).toBeLessThanOrEqual(1);
      },
    );
  }
  test(
    backend + ' cancels ready PSD candidates and refuses malformed features without losing redo',
    async ({ page }) => {
      await page.goto('/?diagnostic=1&document=1' + (backend === 'WebGL2' ? '&gpu=webgl2' : ''));
      await expect(page.getByTestId('engine-status')).toHaveText('Engine ready');
      await whenIdle(page, () =>
        page.evaluate(async () => {
          const api = Reflect.get(window, '__fotoeditor') as EngineApi;
          await api.newDocument(4, 4, 72);
          await api.selectionAction({ kind: 'fill', color: [201, 3, 77, 255] });
          await api.selectionAction({ kind: 'fill', color: [31, 51, 71, 127] });
          await api.historyJump(1);
          await api.diagnosticImportFault();
        }),
      );
      const before = Buffer.from(await exported(page));
      await expect(page.getByRole('button', { name: 'Open image', exact: true })).toBeEnabled();
      await page.getByTestId('open-image').setInputFiles(resolve(root, 'chinedufn__green-1x1.psd'));
      await expect.poll(async () => (await state(page)).importPaused).toBe(true);
      expect((await state(page)).document).toMatchObject({ width: 4, height: 4 });
      expect((await state(page)).history.redo).toBe(1);
      await page.getByRole('button', { name: 'Cancel operation', exact: true }).click();
      await expect(page.getByTestId('operation-status')).toHaveText(/cancelled/);
      expect(Buffer.from(await exported(page)).equals(before)).toBe(true);
      const input = readFileSync(resolve(root, 'chinedufn__green-1x1.psd'));
      const blend = Buffer.from(input);
      const at = blend.indexOf('8BIMnorm');
      expect(at).toBeGreaterThan(0);
      blend.write('diss', at + 4, 'ascii');
      const highDepth = Buffer.from(input);
      highDepth.writeUInt16BE(16, 22);
      for (const [buffer, error] of [
        [input.subarray(0, 43), /Truncated/],
        [blend, /blend mode/],
        [highDepth, /8-bit|16-bit/],
      ] as const) {
        await expect(page.getByRole('button', { name: 'Open image', exact: true })).toBeEnabled();
        await page
          .getByTestId('open-image')
          .setInputFiles({ name: 'refused.psd', mimeType: 'image/vnd.adobe.photoshop', buffer });
        await expect(page.getByTestId('operation-status')).toHaveText(error);
        expect((await state(page)).history.redo).toBe(1);
        expect(Buffer.from(await exported(page)).equals(before)).toBe(true);
      }
    },
  );
  test(
    backend + ' edits an imported Photoshop raster mask with exact undo and ZIP roundtrip',
    async ({ page }) => {
      await page.goto('/?diagnostic=1&document=1' + (backend === 'WebGL2' ? '&gpu=webgl2' : ''));
      await expect(page.getByTestId('engine-status')).toHaveText('Engine ready');
      await page
        .getByTestId('open-image')
        .setInputFiles(resolve(root, 'ag-psd__test__read__layer-mask__src.psd'));
      await expect(page.getByTestId('operation-status')).toHaveText(/Opened in/);
      const before = Buffer.from(await exported(page));
      const target = (await state(page)).layers.nodes[0].id;
      await whenIdle(page, () =>
        page.evaluate(async (target) => {
          await (Reflect.get(window, '__fotoeditor') as EngineApi).selectLayer(target, true);
        }, target),
      );
      await page.getByRole('textbox', { name: 'Color hex' }).fill('#000000');
      const box = (await page.getByTestId('gpu-canvas').boundingBox())!;
      await page.mouse.click(box.x + box.width / 2, box.y + box.height / 2);
      await expect.poll(async () => (await state(page)).history.undo).toBe(1);
      const changed = Buffer.from(await exported(page));
      expect(changed.equals(before)).toBe(false);
      await page.getByRole('button', { name: 'Undo', exact: true }).click();
      await expect.poll(async () => (await state(page)).history.redo).toBe(1);
      expect(Buffer.from(await exported(page)).equals(before)).toBe(true);
      await page.getByRole('button', { name: 'Redo', exact: true }).click();
      await expect.poll(async () => (await state(page)).history.undo).toBe(1);
      const roundtrip = await whenIdle(page, () =>
        page.evaluate(async () => {
          const api = Reflect.get(window, '__fotoeditor') as EngineApi;
          const root = await navigator.storage.getDirectory();
          const handle = await root.getFileHandle('imported-mask.cgfoto', { create: true });
          await api.saveProjectFile(handle);
          const first = new Uint8Array(await (await handle.getFile()).arrayBuffer());
          const reopened = await api.openProjectFile(await handle.getFile());
          await api.saveProjectFile(handle);
          const second = new Uint8Array(await (await handle.getFile()).arrayBuffer());
          return {
            exact: first.length === second.length && first.every((b, i) => b === second[i]),
            mask: reopened.layers.nodes[0].mask,
          };
        }),
      );
      expect(roundtrip).toEqual({ exact: true, mask: { enabled: true } });
      expect(Buffer.from(await exported(page)).equals(changed)).toBe(true);
    },
  );
}
