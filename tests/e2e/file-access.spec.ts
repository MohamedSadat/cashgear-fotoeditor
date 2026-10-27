import { test, expect } from '@playwright/test';
import { PNG } from 'pngjs';
import type { EngineApi, DocumentSnapshot } from '../../apps/web/src/engine/types';
import { whenIdle } from './idle';
const snapshot = async (page: import('@playwright/test').Page) =>
  JSON.parse(
    (await page.getByTestId('gpu-canvas').getAttribute('data-snapshot'))!,
  ) as DocumentSnapshot;
for (const backend of ['WebGPU', 'WebGL2']) {
  test(
    backend +
      ' aborts cancelled and failed project writes before releasing the file-job reservation',
    async ({ page }) => {
      await page.goto('/?diagnostic=1&document=1' + (backend === 'WebGL2' ? '&gpu=webgl2' : ''));
      await expect(page.getByTestId('engine-status')).toHaveText('Engine ready');
      const result = await whenIdle(page, () =>
        page.evaluate(async () => {
          const api = Reflect.get(window, '__fotoeditor') as EngineApi;
          await api.newDocument(4, 4, 72);
          await api.selectionAction({ kind: 'fill', color: [201, 33, 77, 128] });
          await api.selectionAction({ kind: 'fill', color: [11, 93, 57, 255] });
          await api.interact([{ kind: 'undo', x: 0, y: 0, amount: 0 }]);
          const before = Array.from(await api.exportPng());
          const handle = await (
            await navigator.storage.getDirectory()
          ).getFileHandle('atomic-target.cgfoto', { create: true });
          const original = await handle.createWritable();
          await original.write(new Uint8Array([53, 77, 91]));
          await original.close();
          await api.diagnosticProjectSaveFault('pause-after-write');
          const saving = api.saveProjectFile(handle).then(
            () => 'unexpected success',
            (error) => String(error),
          );
          const deadline = performance.now() + 30_000;
          let paused = false;
          while (performance.now() < deadline) {
            const state = JSON.parse(
              document.querySelector('[data-testid="gpu-canvas"]')!.getAttribute('data-snapshot')!,
            );
            if (state?.projectSavePaused) {
              paused = true;
              break;
            }
            await new Promise((resolve) => setTimeout(resolve, 5));
          }
          if (!paused) throw new Error('Project write did not pause');
          const cancelled = await api.cancelJob();
          const cancellation = await saving;
          const afterCancel = Array.from(
            new Uint8Array(await (await handle.getFile()).arrayBuffer()),
          );
          await api.diagnosticProjectSaveFault('quota');
          let failure = '';
          try {
            await api.saveProjectFile(handle);
          } catch (error) {
            failure = String(error);
          }
          const afterFailure = Array.from(
            new Uint8Array(await (await handle.getFile()).arrayBuffer()),
          );
          const after = Array.from(await api.exportPng());
          await api.interact([{ kind: 'redo', x: 0, y: 0, amount: 0 }]);
          const redone = Array.from(await api.exportPng());
          return {
            before,
            after,
            redone,
            cancelled,
            cancellation,
            failure,
            afterCancel,
            afterFailure,
          };
        }),
      );
      expect(result.cancelled).toBe(true);
      expect(result.cancellation).toMatch(/cancelled/);
      expect(result.failure).toMatch(/write failure/);
      expect(result.after).toEqual(result.before);
      expect(result.afterCancel).toEqual([53, 77, 91]);
      expect(result.afterFailure).toEqual([53, 77, 91]);
      expect(Array.from(PNG.sync.read(Buffer.from(result.redone)).data.subarray(0, 4))).toEqual([
        11, 93, 57, 255,
      ]);
    },
  );
  test(
    backend +
      ' image import clears the project handle and rejects a wrong save extension without overwriting files',
    async ({ page }) => {
      await page.goto('/?diagnostic=1&document=1' + (backend === 'WebGL2' ? '&gpu=webgl2' : ''));
      await expect(page.getByTestId('engine-status')).toHaveText('Engine ready');
      await whenIdle(page, () =>
        page.evaluate(async () => {
          const api = Reflect.get(window, '__fotoeditor') as EngineApi;
          const root = await navigator.storage.getDirectory();
          await api.newDocument(4, 4, 72);
          await api.selectionAction({ kind: 'fill', color: [201, 3, 77, 255] });
          const project = await root.getFileHandle('source-project.cgfoto', { create: true });
          await api.saveProjectFile(project);
          await api.selectionAction({ kind: 'fill', color: [31, 51, 71, 255] });
          const image = await root.getFileHandle('source-image.png', { create: true });
          const writer = await image.createWritable();
          await writer.write((await api.exportPng()) as Uint8Array<ArrayBuffer>);
          await writer.close();
          const hash = async (file: File) =>
            Array.from(
              new Uint8Array(await crypto.subtle.digest('SHA-256', await file.arrayBuffer())),
            ).join(',');
          Reflect.set(window, '__sourceHashes', [
            await hash(await project.getFile()),
            await hash(await image.getFile()),
          ]);
          let opens = 0,
            saves = 0;
          Reflect.set(window, 'showOpenFilePicker', async () => [opens++ === 0 ? project : image]);
          Reflect.set(window, 'showSaveFilePicker', async () =>
            saves++ === 0 ? image : root.getFileHandle('new-project.cgfoto', { create: true }),
          );
        }),
      );
      await page.getByRole('button', { name: 'Open project', exact: true }).click();
      await expect(page.getByTestId('operation-status')).toHaveText(/Opened in/);
      await page.getByRole('button', { name: 'Open image', exact: true }).click();
      await expect(page.getByTestId('operation-status')).toHaveText(/Opened in/);
      await page.getByRole('button', { name: 'Save project', exact: true }).click();
      await expect(page.getByTestId('operation-status')).toHaveText(/Choose a \.cgfoto file/);
      await page.getByRole('button', { name: 'Save project as', exact: true }).click();
      await expect(page.getByTestId('operation-status')).toHaveText(/Project saved/);
      expect(
        await page.evaluate(async () => {
          const root = await navigator.storage.getDirectory();
          const hash = async (name: string) =>
            Array.from(
              new Uint8Array(
                await crypto.subtle.digest(
                  'SHA-256',
                  await (await (await root.getFileHandle(name)).getFile()).arrayBuffer(),
                ),
              ),
            ).join(',');
          const after = [await hash('source-project.cgfoto'), await hash('source-image.png')];
          const before = Reflect.get(window, '__sourceHashes') as string[];
          return after.every((value, index) => value === before[index]);
        }),
      ).toBe(true);
    },
  );
  test(
    backend + ' opens handles, reuses saves, saves as and persists permission-aware recents',
    async ({ page }) => {
      await page.goto('/?diagnostic=1&document=1' + (backend === 'WebGL2' ? '&gpu=webgl2' : ''));
      await expect(page.getByTestId('engine-status')).toHaveText('Engine ready');
      await whenIdle(page, () =>
        page.evaluate(async () => {
          const api = Reflect.get(window, '__fotoeditor') as EngineApi;
          await api.newDocument(4, 4, 72);
          await api.selectionAction({ kind: 'fill', color: [201, 3, 77, 255] });
          const root = await navigator.storage.getDirectory();
          const source = await root.getFileHandle('recent-source.cgfoto', { create: true });
          await api.saveProjectFile(source);
          Reflect.set(window, '__openPicks', 0);
          Reflect.set(window, '__savePicks', 0);
          Reflect.set(window, 'showOpenFilePicker', async () => {
            Reflect.set(window, '__openPicks', Reflect.get(window, '__openPicks') + 1);
            return [source];
          });
          Reflect.set(window, 'showSaveFilePicker', async () => {
            Reflect.set(window, '__savePicks', Reflect.get(window, '__savePicks') + 1);
            return root.getFileHandle('recent-saved-as.cgfoto', { create: true });
          });
        }),
      );
      await expect(page.getByRole('button', { name: 'Open project', exact: true })).toBeEnabled();
      await page.getByRole('button', { name: 'Open project', exact: true }).click();
      await expect(page.getByTestId('operation-status')).toHaveText(/Opened in/);
      expect((await snapshot(page)).history.undo).toBe(0);
      await page.getByRole('button', { name: 'Save project', exact: true }).click();
      await expect(page.getByTestId('operation-status')).toHaveText(/Project saved/);
      expect(await page.evaluate(() => Reflect.get(window, '__savePicks'))).toBe(0);
      await page.getByRole('button', { name: 'Save project as', exact: true }).click();
      await expect(page.getByTestId('operation-status')).toHaveText(/Project saved/);
      expect(await page.evaluate(() => Reflect.get(window, '__savePicks'))).toBe(1);
      expect(
        await page.evaluate(async () => {
          const root = await navigator.storage.getDirectory();
          const a = new Uint8Array(
            await (
              await (await root.getFileHandle('recent-source.cgfoto')).getFile()
            ).arrayBuffer(),
          );
          const b = new Uint8Array(
            await (
              await (await root.getFileHandle('recent-saved-as.cgfoto')).getFile()
            ).arrayBuffer(),
          );
          return a.length === b.length && a.every((v, i) => v === b[i]);
        }),
      ).toBe(true);
      await page.getByText('Recent files', { exact: true }).click();
      await expect(
        page.getByRole('button', { name: 'recent-saved-as.cgfoto', exact: true }),
      ).toBeVisible();
      await expect(
        page.getByRole('button', { name: 'recent-source.cgfoto', exact: true }),
      ).toBeVisible();
      await page.reload();
      await expect(page.getByTestId('engine-status')).toHaveText('Engine ready');
      await expect
        .poll(async () => (await snapshot(page))?.recovery?.state ?? 'starting')
        .not.toBe('starting');
      if (await page.getByRole('button', { name: 'Later', exact: true }).isVisible())
        await page.getByRole('button', { name: 'Later', exact: true }).click();
      await page.getByText('Recent files', { exact: true }).click();
      await page.getByRole('button', { name: 'recent-saved-as.cgfoto', exact: true }).click();
      await expect(page.getByTestId('operation-status')).toHaveText(/Opened in/);
      const before = await whenIdle(page, () =>
        page.evaluate(async () =>
          Array.from(await (Reflect.get(window, '__fotoeditor') as EngineApi).exportPng()),
        ),
      );
      await page.evaluate(() => {
        Object.defineProperty(FileSystemFileHandle.prototype, 'queryPermission', {
          configurable: true,
          value: async () => 'denied',
        });
      });
      await page.getByRole('button', { name: 'Save project', exact: true }).click();
      await expect(page.getByTestId('operation-status')).toHaveText(/Write access was denied/);
      await page.getByText('Recent files', { exact: true }).click();
      await page.getByRole('button', { name: 'recent-source.cgfoto', exact: true }).click();
      await expect(page.getByTestId('operation-status')).toHaveText(/File access was denied/);
      await page.evaluate(async () => {
        Object.defineProperty(FileSystemFileHandle.prototype, 'queryPermission', {
          configurable: true,
          value: async () => 'granted',
        });
        const root = await navigator.storage.getDirectory();
        await root.removeEntry('recent-source.cgfoto');
      });
      await page.getByText('Recent files', { exact: true }).click();
      await page.getByRole('button', { name: 'recent-source.cgfoto', exact: true }).click();
      await expect(page.getByTestId('operation-status')).toHaveText(/missing or moved/);
      const after = await whenIdle(page, () =>
        page.evaluate(async () =>
          Array.from(await (Reflect.get(window, '__fotoeditor') as EngineApi).exportPng()),
        ),
      );
      expect(Buffer.from(after).equals(Buffer.from(before))).toBe(true);
      await page.getByText('Recent files', { exact: true }).click();
      await page
        .getByRole('button', { name: 'Remove recent-source.cgfoto from recent files', exact: true })
        .click();
      await expect(
        page.getByRole('button', { name: 'recent-source.cgfoto', exact: true }),
      ).toHaveCount(0);
    },
  );
  test(
    backend + ' cancels pending file pickers without writing or replacing a document',
    async ({ page }) => {
      await page.goto('/?diagnostic=1&document=1' + (backend === 'WebGL2' ? '&gpu=webgl2' : ''));
      await expect(page.getByTestId('engine-status')).toHaveText('Engine ready');
      await whenIdle(page, () =>
        page.evaluate(async () => {
          await (Reflect.get(window, '__fotoeditor') as EngineApi).newDocument(4, 4, 72);
        }),
      );
      await expect(page.getByTestId('document-size')).toHaveText('4 × 4');
      const before = (await snapshot(page)).document;
      await page.evaluate(async () => {
        const root = await navigator.storage.getDirectory();
        const handle = await root.getFileHandle('picker-cancel.cgfoto', { create: true });
        const writable = await handle.createWritable();
        await writable.write(new Uint8Array([11, 22, 33]));
        await writable.close();
        Reflect.set(
          window,
          'showSaveFilePicker',
          () =>
            new Promise((resolve) => {
              Reflect.set(window, '__finishPicker', () => resolve(handle));
            }),
        );
      });
      await page.getByRole('button', { name: 'Save project as', exact: true }).click();
      await expect(
        page.getByRole('button', { name: 'Cancel operation', exact: true }),
      ).toBeVisible();
      await page.getByRole('button', { name: 'Cancel operation', exact: true }).click();
      await expect(page.getByTestId('operation-status')).toHaveText(/cancelled/);
      await page.evaluate(() => Reflect.get(window, '__finishPicker')());
      await expect(page.getByRole('button', { name: 'Save project', exact: true })).toBeEnabled();
      expect((await snapshot(page)).document).toEqual(before);
      expect(
        await page.evaluate(async () => {
          const root = await navigator.storage.getDirectory();
          return Array.from(
            new Uint8Array(
              await (
                await (await root.getFileHandle('picker-cancel.cgfoto')).getFile()
              ).arrayBuffer(),
            ),
          );
        }),
      ).toEqual([11, 22, 33]);
    },
  );
}
