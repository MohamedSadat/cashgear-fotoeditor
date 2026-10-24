import { describe, it, expect, vi, afterEach } from 'vitest';
import { selectionMode, useSelection, performSelection } from './selection-state';
import type { EngineApi } from './engine/types';
describe('selection input and rejected operations', () => {
  afterEach(() => useSelection.setState({ mode: 'new', busy: false, notice: '' }));
  it('combines modifiers and retains the chosen toolbar mode', () => {
    useSelection.setState({ mode: 'subtract' });
    expect(selectionMode(false, false)).toBe('subtract');
    expect(selectionMode(true, false)).toBe('add');
    expect(selectionMode(false, true)).toBe('subtract');
    expect(selectionMode(true, true)).toBe('intersect');
  });
  it('prevents overlapping mutations and surfaces a refused job', async () => {
    let reject: (error: Error) => void = () => {};
    const action = vi.fn(
      () =>
        new Promise((_, rejectPromise) => {
          reject = rejectPromise;
        }),
    );
    const api = { selectionAction: action } as unknown as EngineApi;
    const first = performSelection(api, { kind: 'all' });
    expect(useSelection.getState().busy).toBe(true);
    await performSelection(api, { kind: 'none' });
    expect(action).toHaveBeenCalledTimes(1);
    reject(new Error('Shared CPU budget exceeded'));
    await first;
    expect(useSelection.getState().busy).toBe(false);
    expect(useSelection.getState().notice).toContain('Shared CPU budget');
  });
});
