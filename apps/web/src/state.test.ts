import { afterEach, describe, expect, it } from 'vitest';
import { updateWorkspace, useWorkspace } from './state';
import type { DocumentSnapshot } from './engine/types';

const snapshot = (): DocumentSnapshot => ({
  revision: 1,
  contentRevision: 1,
  painting: true,
  document: { width: 64, height: 64, dpi: 72, tiles: 1 },
  view: {
    zoom: 1,
    panX: 0,
    panY: 0,
    rotation: 0,
    flipped: false,
    screenWidth: 64,
    screenHeight: 64,
  },
  history: { undo: 0, redo: 0, labels: [], evicted: 0, bytesLimit: 268435456, stepsLimit: 200 },
  layers: { active: 1, nodes: [], maskTarget: false, maskView: false, compositing: 0 },
  selection: { active: false, busy: false, quickMask: false, lastWandPixels: 0, clipboard: null },
  canFade: false,
  visibleTiles: 1,
  refining: 0,
  memory: { document: 262144, history: 0, temporary: 0, gpu: 0 },
  cursor: [0, 0],
  pendingSamples: 0,
  uploadedBytes: 0,
  paintTime: 0,
  paintCompleteTime: 0,
});

afterEach(() => updateWorkspace({ snapshot: undefined }));

describe('workspace control subscriptions', () => {
  it('keeps telemetry live while sharing unchanged controls across fresh worker objects', () => {
    const first = snapshot();
    updateWorkspace({ snapshot: first });
    const controls = useWorkspace.getState().controls;
    const next = structuredClone(first);
    next.cursor = [12, 23];
    next.memory.temporary = 1048576;
    next.pendingSamples = 4;
    next.uploadedBytes = 8192;
    next.paintTime = 100;
    next.paintCompleteTime = 120;
    next.timing = { paintMs: 2, renderMs: 10, snapshotMs: 1 };
    updateWorkspace({ snapshot: next });
    expect(useWorkspace.getState().snapshot).toBe(next);
    expect(useWorkspace.getState().snapshot?.cursor).toEqual([12, 23]);
    expect(useWorkspace.getState().snapshot?.memory.temporary).toBe(1048576);
    expect(useWorkspace.getState().controls).toBe(controls);
    expect(controls).not.toHaveProperty('memory');
    expect(controls).not.toHaveProperty('cursor');
  });

  it('publishes admission, command, view, selection and error changes immediately', () => {
    const base = snapshot();
    const changes = [
      { painting: false },
      { operation: 'Autosaving…' },
      { editError: 'Admission refused' },
      { revision: 2, contentRevision: 2 },
      { view: { ...base.view, panX: 10 } },
      { selection: { ...base.selection, busy: true } },
      { history: { ...base.history, undo: 1, labels: ['Brush'] } },
      { layers: { ...base.layers, active: 2 } },
    ];
    for (const change of changes) {
      updateWorkspace({ snapshot: base });
      const before = useWorkspace.getState().controls;
      updateWorkspace({ snapshot: { ...base, ...change } });
      expect(useWorkspace.getState().controls).not.toBe(before);
      expect(useWorkspace.getState().controls).toMatchObject(change);
    }
  });

  it('preserves functional edits and clears control references on worker replacement', () => {
    updateWorkspace({ snapshot: snapshot() });
    updateWorkspace((state) => ({ snapshot: { ...state.snapshot!, painting: false } }));
    expect(useWorkspace.getState().snapshot?.painting).toBe(false);
    expect(useWorkspace.getState().controls?.painting).toBe(false);
    updateWorkspace({ snapshot: undefined });
    expect(useWorkspace.getState().controls).toBeUndefined();
    expect(useWorkspace.getState().controlsKey).toBeUndefined();
  });
});
