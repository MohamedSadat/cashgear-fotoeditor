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
  diagnosticTexture?: boolean;
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
  canFade: boolean;
  filterTiming?: { kernelMs: number; totalMs: number; cpuPeakBytes: number };
  adjustmentTiming?: { kernelMs: number; totalMs: number };
  selection: {
    active: boolean;
    quickMask: boolean;
    busy: boolean;
    lastWandPixels: number;
    clipboard: { width: number; height: number } | null;
    timing?: { kernelMs: number; totalMs: number; stages?: number[] };
  };
  layers: {
    nodes: LayerNode[];
    active?: number;
    maskTarget: boolean;
    maskView: boolean;
    compositing: number;
  };
  projectTiming?: { copyMs: number; validateMs: number; compositeMs: number };
  adjustmentPreview?: { cached: boolean; active: boolean };
  revision: number;
  contentRevision: number;
  operation?: string;
  recovery?: RecoveryStatus;
  document: { width: number; height: number; dpi: number; tiles: number } | null;
  view: {
    zoom: number;
    rotation: number;
    flipped: boolean;
    panX: number;
    panY: number;
    screenWidth: number;
    screenHeight: number;
  };
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
  applyFilter(filter: SpatialFilter): Promise<DocumentSnapshot>;
  fadeFilter(opacity: number, blend: string): Promise<DocumentSnapshot>;
  previewFilter(filter: SpatialFilter, target: number): Promise<AdjustmentPreviewTiming>;
  previewAdjustment(
    adjustment: Adjustment,
    target: AdjustmentPreviewTarget,
  ): Promise<AdjustmentPreviewTiming>;
  endAdjustmentPreview(): Promise<void>;
  diagnosticPreviewFault(fault: 'pause-before-render' | 'hold-gpu-wait' | 'budget'): Promise<void>;
  applyAdjustment(adjustment: Adjustment): Promise<DocumentSnapshot>;
  histogram(target: number): Promise<Histogram>;
  diagnosticAdjustmentFault(
    fault: 'pause-before-commit' | 'budget' | 'budget-after-step',
  ): Promise<void>;
  diagnosticSelectionFault(fault: 'pause-before-commit' | 'budget'): Promise<void>;
  selectionAction(action: SelectionAction): Promise<DocumentSnapshot>;
  quickMask(enabled: boolean): Promise<DocumentSnapshot>;
  clipboardPng(): Promise<Uint8Array>;
  clipboardImport(bytes: ArrayBuffer): Promise<void>;
  thumbnail(target: number): Promise<Uint8Array>;
  compareCompositors(): Promise<number[]>;
  compareAdjustmentCompositors(remaining?: boolean, index?: number): Promise<number[]>;
  compareFilterCompositors(index?: number): Promise<number[]>;
  selectLayer(target: number, mask?: boolean, view?: boolean): Promise<DocumentSnapshot>;
  mergeLayers(kind: 'down' | 'visible' | 'flatten', target: number): Promise<DocumentSnapshot>;
  layerAction(action: LayerAction): Promise<DocumentSnapshot>;
  applyMask(target: number): Promise<DocumentSnapshot>;
  openProjectFile(file: File): Promise<DocumentSnapshot>;
  saveProjectFile(handle: FileSystemFileHandle): Promise<number>;
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

export type SpatialFilter =
  | { kind: 'gaussian' | 'box-blur' | 'median'; radius: number }
  | { kind: 'unsharp-mask'; radius: number; amount: number; threshold: number }
  | { kind: 'sharpen'; amount: number }
  | { kind: 'motion-blur'; radius: number; angle: number }
  | { kind: 'high-pass'; radius: number }
  | {
      kind: 'add-noise';
      amount: number;
      seed: number;
      monochrome: boolean;
      distribution: 'uniform' | 'gaussian';
    }
  | { kind: 'emboss'; radius: number; angle: number; amount: number }
  | { kind: 'pixelate'; size: number };
export type SelectionMode = 'new' | 'add' | 'subtract' | 'intersect';
export type Adjustment =
  | { kind: 'brightness-contrast'; brightness: number; contrast: number }
  | {
      kind: 'levels';
      black: number;
      white: number;
      gamma: number;
      outputBlack: number;
      outputWhite: number;
    }
  | { kind: 'curves'; points: [number, number][] }
  | { kind: 'exposure'; stops: number; offset: number; gamma: number }
  | { kind: 'hue-saturation'; hue: number; saturation: number; lightness: number }
  | { kind: 'vibrance'; amount: number }
  | {
      kind: 'color-balance';
      shadows: number[];
      midtones: number[];
      highlights: number[];
      preserveLuminosity: boolean;
    }
  | { kind: 'black-white'; weights: number[] }
  | { kind: 'invert' }
  | { kind: 'threshold'; level: number }
  | { kind: 'posterize'; levels: number }
  | { kind: 'gradient-map'; stops: [number, number, number, number][] };
export type AdjustmentPreviewTarget =
  | { kind: 'pixels' | 'layer'; target: number }
  | { kind: 'new-layer'; parent: number | null; index: number };
export interface AdjustmentPreviewTiming {
  serial: number;
  preparationMs: number;
  updateMs: number;
  renderSubmitMs: number;
  frameWaitMs: number;
  width: number;
  height: number;
}
export interface Histogram {
  bins: number[][];
  weight: number;
  pixels: number;
  milliseconds: number;
}
export type SelectionShape =
  | { kind: 'rectangle' | 'ellipse'; x0: number; y0: number; x1: number; y1: number }
  | { kind: 'polygon'; points: [number, number][] };
export interface WandOptions {
  seed: [number, number];
  tolerance: number;
  contiguous: boolean;
  antialias: boolean;
  mode: SelectionMode;
  color?: Rgba;
}
export type SelectionAction =
  | { kind: 'all' | 'none' }
  | { kind: 'shape'; shape: SelectionShape; mode: SelectionMode; antialias: boolean }
  | {
      kind: 'modify';
      operation: 'invert' | 'feather' | 'grow' | 'shrink' | 'border' | 'smooth';
      radius: number;
    }
  | { kind: 'wand'; options: WandOptions; allLayers: boolean }
  | { kind: 'bucket'; options: WandOptions; allLayers: boolean; color: Rgba }
  | { kind: 'fill'; color: Rgba; clear?: boolean }
  | { kind: 'copy' | 'cut'; allLayers: boolean }
  | { kind: 'paste'; inPlace: boolean };
export interface LayerProperties {
  visible: boolean;
  opacity: number;
  fill: number;
  blend: string;
  passThrough: boolean;
  clipped: boolean;
  locks: { pixels: boolean; alpha: boolean; position: boolean; all: boolean };
}
export interface LayerNode {
  adjustment?: Adjustment | null;
  id: number;
  name: string;
  properties: LayerProperties;
  mask: { enabled: boolean } | null;
  children: LayerNode[] | null;
}
export type LayerAction =
  | { kind: 'add-adjustment'; adjustment: Adjustment; parent: number | null; index: number }
  | { kind: 'set-adjustment'; target: number; adjustment: Adjustment }
  | { kind: 'add'; name: string; parent: number | null; index: number; group: boolean }
  | { kind: 'delete' | 'duplicate'; ids: number[] }
  | { kind: 'rename'; target: number; name: string }
  | { kind: 'move'; ids: number[]; parent: number | null; index: number }
  | { kind: 'group'; ids: number[]; name: string }
  | { kind: 'ungroup' | 'mask-add' | 'mask-delete'; target: number }
  | { kind: 'properties'; target: number; properties: LayerProperties }
  | { kind: 'mask-enabled'; target: number; enabled: boolean };
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
