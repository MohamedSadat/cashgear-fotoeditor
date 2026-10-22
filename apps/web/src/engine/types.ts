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
  document: { width: number; height: number; dpi: number; tiles: number } | null;
  view: { zoom: number; rotation: number; flipped: boolean; panX: number; panY: number };
  memory: { document: number; history: number; temporary: number; gpu: number };
  visibleTiles: number;
  refining: number;
  uploadedBytes: number;
  history: { undo: number; redo: number };
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
export interface EngineFatalEvent {
  type: 'engine-fatal';
  message: string;
}
