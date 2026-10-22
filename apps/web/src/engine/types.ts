export type BackendPreference = 'auto' | 'webgl2';
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
  snapshot?: DocumentSnapshot;
}
export interface DocumentSnapshot {
  document: { width: number; height: number; dpi: number; tiles: number } | null;
  view: { zoom: number; rotation: number; flipped: boolean; panX: number; panY: number };
  memory: { document: number; history: number; temporary: number; gpu: number };
  visibleTiles: number;
  refining: number;
  uploadedBytes: number;
  name?: string;
  cursor?: [number, number];
}
export interface Interaction {
  kind: 'pan' | 'zoom' | 'rotate' | 'fit' | 'actual' | 'flip' | 'reset-rotation' | 'cursor';
  x: number;
  y: number;
  amount: number;
}
export interface EngineApi {
  init(canvas: OffscreenCanvas, options: InitOptions): Promise<EngineInfo>;
  resize(size: PhysicalSize): void | Promise<void>;
  frame(timestamp: number): FrameResult | Promise<FrameResult>;
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
  cancelJob(): void | Promise<void>;
}
export interface EngineFatalEvent {
  type: 'engine-fatal';
  message: string;
}
