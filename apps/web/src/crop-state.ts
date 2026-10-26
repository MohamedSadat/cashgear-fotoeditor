import { create } from 'zustand';
import type { CanvasRect, EngineApi, Interpolation } from './engine/types';
import { updateWorkspace, useWorkspace } from './state';
import { useBrush } from './brush-state';
export const useCrop = create<{
  rect?: CanvasRect;
  sourceRevision?: number;
  ratio: string;
  degrees: number;
  interpolation: Interpolation;
  straighten: boolean;
  busy: boolean;
  notice: string;
}>(() => ({
  ratio: 'free',
  degrees: 0,
  interpolation: 'lanczos3',
  straighten: false,
  busy: false,
  notice: '',
}));
let epoch = 0;
export function resetCrop() {
  ++epoch;
  cancelCrop();
  useCrop.setState({ busy: false });
}
export function startCrop() {
  const s = useWorkspace.getState().snapshot,
    d = s?.document;
  if (!d || s.operation || s.painting || useCrop.getState().busy) return;
  useBrush.setState({ tool: 'crop' });
  useCrop.setState({
    rect: { x: d.originX ?? 0, y: d.originY ?? 0, width: d.width, height: d.height },
    sourceRevision: s.contentRevision,
    degrees: 0,
    interpolation: 'lanczos3',
    straighten: false,
    notice: '',
  });
}
export function cancelCrop() {
  useCrop.setState({
    rect: undefined,
    sourceRevision: undefined,
    degrees: 0,
    straighten: false,
    notice: '',
  });
}
export async function applyCrop(api: EngineApi) {
  const token = epoch;
  const prefs = useCrop.getState(),
    s = useWorkspace.getState().snapshot;
  if (prefs.busy || !prefs.rect || !s) return;
  if (s.contentRevision !== prefs.sourceRevision) {
    cancelCrop();
    return;
  }
  useCrop.setState({ busy: true, notice: '' });
  try {
    const snapshot = await api.geometryAction({
      kind: 'crop-straighten',
      rect: prefs.rect,
      degrees: prefs.degrees,
      interpolation: prefs.interpolation,
    });
    if (token === epoch) {
      updateWorkspace({ snapshot });
      cancelCrop();
    }
  } catch (e) {
    if (token === epoch) useCrop.setState({ notice: String(e) });
  } finally {
    if (token === epoch) useCrop.setState({ busy: false });
  }
}
