export type BackendPreference = 'auto' | 'webgl2';
export type Backend = 'WebGPU' | 'WebGL2';
export interface PhysicalSize {
  width: number;
  height: number;
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
}
export interface EngineApi {
  init(canvas: OffscreenCanvas, options: InitOptions): Promise<EngineInfo>;
  resize(size: PhysicalSize): void | Promise<void>;
  frame(timestamp: number): FrameResult | Promise<FrameResult>;
  dispose(): void | Promise<void>;
  diagnosticFault(kind: 'device-loss' | 'panic'): void | Promise<void>;
}
export interface EngineFatalEvent {
  type: 'engine-fatal';
  message: string;
}
