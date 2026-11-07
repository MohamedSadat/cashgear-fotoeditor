import { create } from 'zustand';
import type { Backend } from './engine/types';
import type { DocumentSnapshot } from './engine/types';
const telemetry = [
  'memory',
  'cursor',
  'pendingSamples',
  'uploadedBytes',
  'paintTime',
  'paintCompleteTime',
  'timing',
] as const;
type ControlSnapshot = Omit<DocumentSnapshot, (typeof telemetry)[number]>;
interface WorkspaceState {
  previewOwner?: 'adjustment' | 'filter';
  status: 'starting' | 'ready' | 'error';
  backend?: Backend;
  adapter?: string;
  error?: string;
  size: { width: number; height: number };
  frames: number;
  snapshot?: DocumentSnapshot;
  controls?: ControlSnapshot;
  controlsKey?: string;
}
export const useWorkspace = create<WorkspaceState>(() => ({
  status: 'starting',
  size: { width: 0, height: 0 },
  frames: 0,
}));
export function updateWorkspace(
  update: Partial<WorkspaceState> | ((state: WorkspaceState) => Partial<WorkspaceState>),
) {
  useWorkspace.setState((state) => {
    const next = typeof update === 'function' ? update(state) : update;
    if (!Object.hasOwn(next, 'snapshot') || next.snapshot === state.snapshot) return next;
    if (!next.snapshot) return { ...next, controls: undefined, controlsKey: undefined };
    // Keep the complete worker snapshot live. Only control subscriptions share
    // unchanged metadata, so telemetry never rerenders the full editing UI.
    const controls: Partial<DocumentSnapshot> = { ...next.snapshot };
    for (const key of telemetry) delete controls[key];
    const controlsKey = JSON.stringify(controls);
    return {
      ...next,
      controls: controlsKey === state.controlsKey ? state.controls : (controls as ControlSnapshot),
      controlsKey,
    };
  });
}
