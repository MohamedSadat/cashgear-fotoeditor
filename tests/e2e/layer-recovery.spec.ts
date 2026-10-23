import { test, expect } from '@playwright/test';
import { resolve } from 'node:path';
import type { EngineApi } from '../../apps/web/src/engine/types';
for (const backend of ['WebGPU', 'WebGL2']) {
  test(
    backend +
      ' retains streamed recovery after quota failure and a renderer kill during a candidate',
    async ({ page, context }) => {
      test.setTimeout(180000);
      const query = '/?diagnostic=1&document=1' + (backend === 'WebGL2' ? '&gpu=webgl2' : '');
      await page.goto(query);
      await expect(page.getByTestId('engine-status')).toHaveText('Engine ready');
      await page
        .getByTestId('open-image')
        .setInputFiles(resolve('bench/results/fixtures/layers-small.cgfoto'));
      await expect(page.getByTestId('operation-status')).toHaveText(/Opened in/);
      await page.evaluate(async () => {
        const api = Reflect.get(window, '__fotoeditor') as EngineApi;
        await api.autosave();
      });
      await expect(page.getByTestId('recovery-status')).toHaveText(/^Autosaved /);
      await page.evaluate(async () => {
        const api = Reflect.get(window, '__fotoeditor') as EngineApi;
        const snapshot = await api.layerAction({
          kind: 'add',
          parent: null,
          index: 3,
          name: 'Unsaved layer',
          group: false,
        });
        if (snapshot.layers.nodes.length !== 4) throw new Error('Layer mutation did not commit');
        await api.diagnosticRecoveryFault('quota');
        await api.autosave();
      });
      await expect(page.getByTestId('recovery-status')).toHaveText('Autosave failed');
      await expect
        .poll(
          async () =>
            JSON.parse((await page.getByTestId('gpu-canvas').getAttribute('data-snapshot'))!)
              .recovery.error,
        )
        .toContain('Previous completed snapshots are retained');
      await page.evaluate(async () => {
        const api = Reflect.get(window, '__fotoeditor') as EngineApi;
        await api.diagnosticRecoveryFault('pause-after-chunk');
        void api.autosave();
      });
      await expect
        .poll(() =>
          page.evaluate(async () => {
            const directory = await (
              await navigator.storage.getDirectory()
            ).getDirectoryHandle('cashgear-recovery-v1');
            const files: string[] = [];
            for await (const [name] of (
              directory as unknown as {
                entries(): AsyncIterableIterator<[string, FileSystemHandle]>;
              }
            ).entries())
              files.push(name);
            return files.some(
              (name) => name.endsWith('.cgfoto') && !files.includes(name + '.json'),
            );
          }),
        )
        .toBe(true);
      const session = await context.newCDPSession(page);
      const crashed = page.waitForEvent('crash');
      void session.send('Page.crash').catch(() => {});
      await crashed;
      await page.close();
      page = await context.newPage();
      await page.goto(query);
      await expect(page.getByTestId('engine-status')).toHaveText('Engine ready');
      await page.getByRole('button', { name: 'Restore session', exact: true }).click();
      await expect
        .poll(
          async () =>
            JSON.parse((await page.getByTestId('gpu-canvas').getAttribute('data-snapshot'))!).layers
              .nodes.length,
        )
        .toBe(3);
      await expect
        .poll(
          async () =>
            JSON.parse((await page.getByTestId('gpu-canvas').getAttribute('data-snapshot'))!)
              .history.undo,
        )
        .toBe(0);
      await expect(page.getByTestId('document-size')).toHaveText('128 × 128');
      await expect(page.getByTestId('engine-status')).toHaveText('Engine ready');
    },
  );
}
