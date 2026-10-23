import { test, expect } from '@playwright/test';
import type { EngineApi } from '../../apps/web/src/engine/types';
for (const backend of ['WebGPU', 'WebGL2']) {
  test(
    backend +
      ' compares every GPU blend mode against independent CPU tiles with masks clipping and groups',
    async ({ page }) => {
      test.setTimeout(180000);
      await page.goto('/?diagnostic=1' + (backend === 'WebGL2' ? '&gpu=webgl2' : ''));
      await expect(page.getByTestId('engine-status')).toHaveText('Engine ready');
      const errors = await page.evaluate(async () => {
        const api = Reflect.get(window, '__fotoeditor') as EngineApi;
        return api.compareCompositors();
      });
      expect(errors).toHaveLength(64);
      expect(Math.max(...errors)).toBeLessThanOrEqual(1);
      await expect(page.getByTestId('engine-status')).toHaveText('Engine ready');
    },
  );
}
