import { test, expect } from '@playwright/test';
import type { Page } from '@playwright/test';
import { readFile } from 'node:fs/promises';
import type { DocumentSnapshot } from '../../apps/web/src/engine/types';
const snap = async (page: Page) =>
  JSON.parse(
    (await page.getByTestId('gpu-canvas').getAttribute('data-snapshot'))!,
  ) as DocumentSnapshot;
async function ready(page: Page) {
  await expect(page.getByTestId('engine-status')).toHaveText('Engine ready');
}
async function idle(page: Page, undo?: number) {
  await expect.poll(async () => (await snap(page)).painting).toBe(false);
  await expect.poll(async () => (await snap(page)).operation).toBeUndefined();
  if (undo !== undefined) await expect.poll(async () => (await snap(page)).history.undo).toBe(undo);
}
async function create(page: Page) {
  await page.getByRole('button', { name: 'New canvas', exact: true }).click();
  await page.getByRole('spinbutton', { name: 'Width', exact: true }).fill('512');
  await page.getByRole('spinbutton', { name: 'Height', exact: true }).fill('256');
  await page.getByRole('button', { name: 'Create document' }).click();
  await expect(page.getByTestId('document-size')).toHaveText('512 × 256');
}
async function stroke(page: Page, color: string, offset = 0) {
  await page.getByRole('textbox', { name: 'Color hex' }).fill(color);
  const box = (await page.getByTestId('gpu-canvas').boundingBox())!;
  await page.mouse.click(box.x + box.width / 2 + offset, box.y + box.height / 2);
  await idle(page);
}
async function project(page: Page) {
  // Exercise the download fallback explicitly, without a native save dialog.
  await page.evaluate(() => Reflect.set(window, 'showSaveFilePicker', undefined));
  const download = page.waitForEvent('download');
  await page.getByRole('button', { name: 'Save project', exact: true }).click();
  const bytes = await readFile((await (await download).path())!);
  await idle(page);
  return bytes;
}
async function autosave(page: Page) {
  const previous = (await snap(page)).recovery?.lastSavedAt ?? 0;
  await page.getByRole('button', { name: 'Save recovery now', exact: true }).click();
  await expect(page.getByTestId('recovery-status')).toHaveText(/^Autosaved /);
  await expect
    .poll(async () => (await snap(page)).recovery?.lastSavedAt ?? 0)
    .toBeGreaterThan(previous);
  await idle(page);
}
for (const backend of ['WebGPU', 'WebGL2']) {
  const query = backend === 'WebGL2' ? '?gpu=webgl2' : '';
  test(
    backend + ' history jumps branches evicts and merges document property gestures',
    async ({ page }) => {
      await page.goto('/' + query);
      await ready(page);
      await create(page);
      for (const [i, color] of ['#ff0000', '#00ff00', '#0000ff'].entries()) {
        await stroke(page, color, i * 30);
        await idle(page, i + 1);
      }
      const end = await project(page);
      await page.getByRole('listitem').filter({ hasText: 'Initial state' }).click();
      await idle(page, 0);
      await page.getByRole('listitem').filter({ hasText: '3. Brush stroke' }).click();
      await idle(page, 3);
      expect((await project(page)).equals(end)).toBe(true);
      await page.getByRole('button', { name: 'Undo', exact: true }).click();
      await idle(page, 2);
      await stroke(page, '#ff00ff', -40);
      await idle(page, 3);
      expect((await snap(page)).history.redo).toBe(0);
      await page.getByText('History limits', { exact: true }).click();
      await page.getByRole('spinbutton', { name: 'Maximum history steps' }).fill('2');
      await page.getByRole('button', { name: 'Apply history limits' }).click();
      await idle(page, 2);
      expect((await snap(page)).history.evicted).toBe(1);
      await page.getByRole('spinbutton', { name: 'Maximum history steps' }).fill('200');
      await page.getByRole('button', { name: 'Apply history limits' }).click();
      const slider = page.getByRole('slider', { name: 'Document resolution DPI' });
      await slider.focus();
      for (const value of [100, 200, 300]) {
        await slider.evaluate((element, value) => {
          Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')!.set!.call(
            element,
            String(value),
          );
          element.dispatchEvent(new Event('input', { bubbles: true }));
          element.dispatchEvent(new Event('change', { bubbles: true }));
        }, value);
        await expect.poll(async () => (await snap(page)).document?.dpi).toBe(value);
      }
      await idle(page, 3);
      await page.getByRole('button', { name: 'Undo', exact: true }).click();
      await idle(page, 2);
      expect((await snap(page)).document?.dpi).toBe(72);
    },
  );
  test(
    backend + ' project roundtrip preserves bytes and rejects corrupt versions atomically',
    async ({ page }) => {
      await page.goto('/' + query);
      await ready(page);
      await create(page);
      await stroke(page, '#57392101');
      await idle(page, 1);
      const bytes = await project(page);
      await page.getByTestId('open-project').setInputFiles({
        name: 'exact.cgfoto',
        mimeType: 'application/octet-stream',
        buffer: bytes,
      });
      await expect(page.getByTestId('operation-status')).toHaveText(/Opened in/);
      await idle(page, 0);
      expect((await project(page)).equals(bytes)).toBe(true);
      for (const offset of [8, 100]) {
        const bad = Buffer.from(bytes);
        bad[offset] ^= 255;
        await page.getByTestId('open-project').setInputFiles({
          name: 'broken.cgfoto',
          mimeType: 'application/octet-stream',
          buffer: bad,
        });
        await expect(page.getByTestId('operation-status')).toHaveText(/ZIP|version|checksum/);
        await idle(page, 0);
        expect((await project(page)).equals(bytes)).toBe(true);
      }
    },
  );
  test(
    backend + ' crash recovery retains completed generations and skips corrupt latest snapshot',
    async ({ page, context }) => {
      await page.goto('/' + query);
      await ready(page);
      await create(page);
      await stroke(page, '#ff0000');
      await idle(page, 1);
      const first = await project(page);
      await autosave(page);
      await stroke(page, '#0000ff', 40);
      await idle(page, 2);
      await autosave(page);
      // Damage only the newest payload. Prior immutable generations must survive.
      await page.evaluate(async () => {
        const directory = await (
          await navigator.storage.getDirectory()
        ).getDirectoryHandle('cashgear-recovery-v1');
        const entries: { file: string; savedAt: number }[] = [];
        for await (const [name, handle] of (
          directory as unknown as {
            entries(): AsyncIterableIterator<[string, FileSystemFileHandle]>;
          }
        ).entries()) {
          if (name.endsWith('.json'))
            entries.push(JSON.parse(await (await handle.getFile()).text()));
        }
        entries.sort((a, b) => b.savedAt - a.savedAt);
        const handle = await directory.getFileHandle(entries[0].file);
        const writer = await handle.createWritable();
        await writer.write('interrupted');
        await writer.close();
      });
      const session = await context.newCDPSession(page);
      const crashed = page.waitForEvent('crash');
      void session.send('Page.crash').catch(() => {});
      await crashed;
      await page.close();
      page = await context.newPage();
      await page.goto('/' + query);
      await ready(page);
      await expect(page.getByRole('dialog', { name: 'Restore last session?' })).toBeVisible();
      await page.getByRole('button', { name: 'Restore session', exact: true }).click();
      await idle(page, 0);
      await expect(page.getByTestId('document-size')).toHaveText('512 × 256');
      expect((await project(page)).equals(first)).toBe(true);
    },
  );
  test(
    backend + ' autosaves on timer and tab hide despite persistence request failure',
    async ({ page }) => {
      await page.clock.install();
      await page.addInitScript(() => {
        Object.defineProperty(navigator.storage, 'persist', {
          value: () => Promise.reject(new Error('Persistence denied')),
        });
      });
      await page.goto('/' + query);
      await ready(page);
      await create(page);
      await stroke(page, '#ff0000');
      await idle(page, 1);
      expect((await snap(page)).recovery?.persistent).toBe('unavailable');
      await page.clock.fastForward(30001);
      await expect(page.getByTestId('recovery-status')).toHaveText(/^Autosaved /);
      await idle(page);
      const first = (await snap(page)).recovery!.lastSavedAt!;
      await page.clock.fastForward(30001);
      await idle(page);
      expect((await snap(page)).recovery?.lastSavedAt).toBe(first);
      await stroke(page, '#0000ff', 30);
      await idle(page, 2);
      await page.evaluate(() => {
        Object.defineProperty(document, 'visibilityState', {
          configurable: true,
          get: () => 'hidden',
        });
        document.dispatchEvent(new Event('visibilitychange'));
      });
      await expect
        .poll(async () => (await snap(page)).recovery?.lastSavedAt ?? 0)
        .toBeGreaterThan(first);
    },
  );
  test(
    backend + ' actual renderer kill during an uncommitted write recovers prior generation',
    async ({ page, context }) => {
      const url = '/' + (query ? query + '&diagnostic=1' : '?diagnostic=1');
      await page.goto(url);
      await ready(page);
      await create(page);
      await stroke(page, '#ff0000');
      await idle(page, 1);
      const prior = await project(page);
      await autosave(page);
      await page.getByRole('button', { name: 'New canvas', exact: true }).click();
      await page.getByRole('spinbutton', { name: 'Width', exact: true }).fill('1024');
      await page.getByRole('spinbutton', { name: 'Height', exact: true }).fill('1024');
      await page.getByRole('button', { name: 'Create document' }).click();
      await idle(page, 0);
      await page.getByRole('slider', { name: 'Brush size', exact: true }).evaluate((element) => {
        Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')!.set!.call(
          element,
          '2048',
        );
        element.dispatchEvent(new Event('input', { bubbles: true }));
      });
      await stroke(page, '#0000ff');
      await idle(page, 1);
      await page
        .getByRole('button', { name: 'Fail next autosave: pause-after-chunk', exact: true })
        .click();
      await page.getByRole('button', { name: 'Save recovery now', exact: true }).click();
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
            return files.some((f) => f.endsWith('.cgfoto') && !files.includes(f + '.json'));
          }),
        )
        .toBe(true);
      const session = await context.newCDPSession(page);
      const crashed = page.waitForEvent('crash');
      void session.send('Page.crash').catch(() => {});
      await crashed;
      await page.close();
      page = await context.newPage();
      await page.goto('/' + query);
      await ready(page);
      await page.getByRole('button', { name: 'Restore session', exact: true }).click();
      await idle(page, 0);
      expect((await project(page)).equals(prior)).toBe(true);
    },
  );
  test(backend + ' active tabs own independent recovery writers', async ({ page, context }) => {
    await page.goto('/' + query);
    await ready(page);
    await create(page);
    await stroke(page, '#ff0000');
    await idle(page, 1);
    await autosave(page);
    const second = await context.newPage();
    await second.goto('/' + query);
    await ready(second);
    await expect(second.getByRole('dialog', { name: 'Restore last session?' })).not.toBeVisible();
    await create(second);
    await stroke(second, '#0000ff');
    await idle(second, 1);
    const blue = await project(second);
    await autosave(second);
    await second.close();
    const third = await context.newPage();
    await third.goto('/' + query);
    await ready(third);
    await third.getByRole('button', { name: 'Restore session', exact: true }).click();
    await idle(third, 0);
    expect((await project(third)).equals(blue)).toBe(true);
    expect((await snap(page)).document?.width).toBe(512);
  });
  test(
    backend + ' quota and interrupted writes preserve prior valid recovery and editing',
    async ({ page }) => {
      await page.goto('/' + (query ? query + '&diagnostic=1' : '?diagnostic=1'));
      await ready(page);
      await create(page);
      await stroke(page, '#ff0000');
      await idle(page, 1);
      const prior = await project(page);
      await autosave(page);
      const savedAt = (await snap(page)).recovery?.lastSavedAt;
      await stroke(page, '#0000ff', 30);
      await idle(page, 2);
      const current = await project(page);
      for (const fault of ['quota', 'partial', 'after-flush']) {
        await page
          .getByRole('button', { name: 'Fail next autosave: ' + fault, exact: true })
          .click();
        await page.getByRole('button', { name: 'Save recovery now', exact: true }).click();
        await expect(page.getByTestId('recovery-status')).toHaveText('Autosave failed');
        await expect
          .poll(async () => (await snap(page)).recovery?.error)
          .toContain(
            fault === 'quota'
              ? 'quota'
              : fault === 'partial'
                ? 'interrupted snapshot'
                : 'before generation commit',
          );
        await idle(page, 2);
        expect((await snap(page)).recovery?.lastSavedAt).toBe(savedAt);
        expect((await project(page)).equals(current)).toBe(true);
      }
      await page.reload();
      await ready(page);
      await page.getByRole('button', { name: 'Restore session', exact: true }).click();
      await idle(page, 0);
      expect((await project(page)).equals(prior)).toBe(true);
    },
  );
}
