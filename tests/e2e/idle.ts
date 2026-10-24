import type { Page } from '@playwright/test';

// Autosave can acquire the worker between RPCs, especially with software GPUs.
// These admission errors occur before an edit; never retry processing failures.
export async function whenIdle<T>(page: Page, action: () => Promise<T>): Promise<T> {
  const deadline = Date.now() + 60000;
  for (;;) {
    try {
      return await action();
    } catch (error) {
      if (
        !/Finish or cancel the current operation first|Histogram is already updating/.test(
          String(error),
        ) ||
        Date.now() >= deadline
      )
        throw error;
      await page.waitForTimeout(50);
    }
  }
}
