import { create } from 'zustand';
import type { Backend } from './engine/types';
import type { DocumentSnapshot } from './engine/types';
interface WorkspaceState {
  status: 'starting' | 'ready' | 'error';
  backend?: Backend;
  adapter?: string;
  error?: string;
  size: { width: number; height: number };
  frames: number;
  snapshot?: DocumentSnapshot;
}
export const useWorkspace = create<WorkspaceState>(() => ({
  status: 'starting',
  size: { width: 0, height: 0 },
  frames: 0,
}));
export const updateWorkspace = useWorkspace.setState;
