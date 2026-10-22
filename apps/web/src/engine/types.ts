export type BackendPreference = 'auto' | 'webgl2';
import type { BrushSettings, Rgba } from '../brush-state';
export type Backend = 'WebGPU' | 'WebGL2';
export interface PhysicalSize {
  width: number;
  height: number;
  dpr?: number;
}
export interface InitOptions extends PhysicalSize {
  backend: BackendPreference;
  diagnostic: boolean;
  failInitialization?: boolean;
  simulateGpuFailure?: 'adapter' | 'device';
}
export interface EngineInfo {
  backend: Backend;
  adapter: string;
}
export interface FrameResult extends PhysicalSize {
  presented: boolean;
  frames: number;
  presentedAt?: number;
  snapshot?: DocumentSnapshot;
}
export interface DocumentSnapshot {
  revision: number;
  operation?: string;
  recovery?: RecoveryStatus;
  document: { width: number; height: number; dpi: number; tiles: number } | null;
  view: { zoom: number; rotation: number; flipped: boolean; panX: number; panY: number };
  memory: { document: number; history: number; temporary: number; gpu: number };
  visibleTiles: number;
  refining: number;
  uploadedBytes: number;
  history: {
    undo: number;
    redo: number;
    labels: string[];
    bytesLimit: number;
    stepsLimit: number;
    evicted: number;
  };
  painting: boolean;
  pendingSamples: number;
  paintTime: number;
  paintCompleteTime: number;
  timing?: { paintMs: number; renderMs: number; snapshotMs: number };
  editError?: string;
  sampledColor?: { rgba: Rgba; serial: number };
  name?: string;
  cursor?: [number, number];
}
export interface Interaction {
  kind:
    | 'pan'
    | 'zoom'
    | 'rotate'
    | 'fit'
    | 'actual'
    | 'flip'
    | 'reset-rotation'
    | 'cursor'
    | 'stroke-begin'
    | 'stroke-sample'
    | 'stroke-end'
    | 'stroke-cancel'
    | 'sample-color'
    | 'undo'
    | 'redo';
  x: number;
  y: number;
  amount: number;
  brush?: BrushSettings & { color: Rgba; eraser: boolean };
  sample?: [number, number, number, number, number, number];
}
export interface EngineApi {
  configureHistory(bytes: number, steps: number): Promise<void>;
  historyJump(target: number): Promise<void>;
  resolution(dpi: number, gesture: number): Promise<void>;
  openProject(bytes: ArrayBuffer, name: string): Promise<DocumentSnapshot>;
  saveProject(): Promise<Uint8Array>;
  startRecovery(persistent: RecoveryStatus['persistent']): Promise<void>;
  autosave(): Promise<void>;
  restoreRecovery(file: string): Promise<DocumentSnapshot>;
  discardRecovery(file: string): Promise<void>;
  diagnosticRecoveryFault(
    fault: 'quota' | 'partial' | 'after-flush' | 'pause-after-chunk',
  ): Promise<void>;
  init(canvas: OffscreenCanvas, options: InitOptions): Promise<EngineInfo>;
  resize(size: PhysicalSize): void | Promise<void>;
  frame(
    timestamp: number,
    inputs?: Interaction[],
    preview?: boolean,
  ): FrameResult | Promise<FrameResult>;
  dispose(): void | Promise<void>;
  diagnosticFault(kind: 'device-loss' | 'panic'): void | Promise<void>;
  interact(inputs: Interaction[]): void | Promise<void>;
  newDocument(
    width: number,
    height: number,
    dpi: number,
  ): DocumentSnapshot | Promise<DocumentSnapshot>;
  openImage(bytes: ArrayBuffer, name: string): Promise<DocumentSnapshot>;
  exportPng(): Promise<Uint8Array>;
  cancelJob(): boolean | Promise<boolean>;
}
export interface RecoveryCandidate {
  session: string;
  file: string;
  name: string;
  savedAt: number;
  size: number;
}
export interface RecoveryStatus {
  state: 'starting' | 'idle' | 'saving' | 'saved' | 'error' | 'unavailable';
  persistent: 'granted' | 'denied' | 'unavailable';
  candidates: RecoveryCandidate[];
  lastSavedAt?: number;
  lastSaveMs?: number;
  error?: string;
}
export interface EngineFatalEvent {
  type: 'engine-fatal';
  message: string;
}
