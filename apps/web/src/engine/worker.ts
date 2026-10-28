import { expose, transfer } from 'comlink';
import type {
  EngineApi,
  EngineInfo,
  InitOptions,
  PhysicalSize,
  Interaction,
  TransformUpdate,
} from './types';
import initWasm, { Engine, codec_chunk } from './pkg/fotoeditor_wasm';
import type { Adjustment, AdjustmentPreviewTarget, SpatialFilter, DocumentSnapshot } from './types';
import type { RecoveryStatus } from './types';
import { RecoveryStore } from './recovery';
import type { RecoveryFault } from './recovery';
import { yieldProjectTask } from './yield-task';
import { CHUNK, PagingStore } from './paging';
import type { SyncHandle } from './paging';
const pages = new PagingStore();

let engine: Engine | undefined;
let initializing = false;
let disposed = false;
let diagnostic = false;
let size: PhysicalSize = { width: 0, height: 0 };
let frames = 0;
let job = 0;
let busy = false;
let cancelAllowed = true;
let fileMaskView = false;
let presentation: { resolve: () => void; reject: (error: Error) => void } | undefined;
let name = 'Untitled';
let cursorScreen: [number, number] | undefined;
let editError: string | undefined;
let sampledColor: DocumentSnapshot['sampledColor'];
let inputsWaiting: Interaction[] = [];
let dirty = true;
let refining = false;
let operation: string | undefined;
let projectTiming: { copyMs: number; validateMs: number; compositeMs: number } | undefined;
let projectSaveTiming: DocumentSnapshot['projectSaveTiming'];
let imageExportTiming: DocumentSnapshot['imageExportTiming'];
let recovery: RecoveryStatus = { state: 'starting', persistent: 'unavailable', candidates: [] };
let store: RecoveryStore | undefined;
let savedRevision = -1;
let recoveryFault: RecoveryFault | undefined;
let externalBytes = 0;
let thumbnailRunning = false;
let selectionTiming: DocumentSnapshot['selection']['timing'];
let selectionFault: 'pause-before-commit' | 'budget' | undefined;
let releaseSelectionPause: (() => void) | undefined;
let filterTiming: DocumentSnapshot['filterTiming'];
let geometryTiming: DocumentSnapshot['geometryTiming'];
let adjustmentTiming: DocumentSnapshot['adjustmentTiming'];
let adjustmentFault: 'pause-before-commit' | 'budget' | 'budget-after-step' | undefined;
let releaseAdjustmentPause: (() => void) | undefined;
let pauseNextImport = false;
let importPaused = false;
let releaseImportPause: (() => void) | undefined;
let activeImageReader: FileReader | undefined;
let cancelCodec: (() => void) | undefined;
let exportBudgetFault = false;
let activeFileJob: Promise<void> | undefined;
let projectSaveFault: 'pause-after-write' | 'quota' | undefined;
let releaseProjectSavePause: (() => void) | undefined;
let projectSavePaused = false;
function beginFileJob() {
  let finish!: () => void;
  const pending = new Promise<void>((resolve) => {
    finish = resolve;
  });
  activeFileJob = pending;
  return () => {
    if (activeFileJob === pending) activeFileJob = undefined;
    finish();
  };
}
const codecErrors = new Map<string, (error: Error) => void>();
self.addEventListener(
  'message',
  (event: MessageEvent<{ type?: string; id?: string; error?: string }>) => {
    if (event.data?.type === 'codec-error' && event.data.id)
      codecErrors.get(event.data.id)?.(new Error(event.data.error ?? 'Image codec worker failed'));
  },
);
function createCodecWorker() {
  const id = crypto.randomUUID();
  const channel = new MessageChannel();
  // Main-thread creation avoids the WebView2 custom-protocol nested-worker bug.
  self.postMessage({ type: 'codec-start', id, port: channel.port2 }, [channel.port2]);
  return {
    id,
    port: channel.port1,
    terminate() {
      codecErrors.delete(id);
      channel.port1.close();
      self.postMessage({ type: 'codec-stop', id });
    },
  };
}
let histogramRunning = false;
let previewRequest = 0;
let previewKey: string | undefined;
let previewRunning = false;
let previewCompletion: Promise<void> | undefined;
let previewFault: 'pause-before-render' | 'hold-gpu-wait' | 'budget' | undefined;
let releasePreviewPause: (() => void) | undefined;
function beginJob() {
  cancelAllowed = true;
  return ++job;
}
function external(bytes: number) {
  requireEngine().external_bytes(bytes);
  externalBytes = bytes;
}
const yieldTask = () => new Promise<void>((resolve) => setTimeout(resolve, 0));
function snapshot(): DocumentSnapshot {
  const p = cursorScreen ? engine!.cursor(...cursorScreen) : undefined;
  const state = JSON.parse(engine!.snapshot()) as DocumentSnapshot;
  return {
    ...state,
    selection: { ...state.selection, timing: selectionTiming },
    operation,
    projectTiming,
    projectSaveTiming,
    imageExportTiming,
    projectSavePaused,
    adjustmentTiming,
    filterTiming,
    geometryTiming,
    importPaused,
    recovery,
    // A completed stroke can have the next stroke waiting behind it. Keep the
    // entire queued edit busy so open/export cannot interleave in that gap.
    painting: state.painting || inputsWaiting.some((input) => input.kind.startsWith('stroke-')),
    name,
    cursor: p ? [p[0], p[1]] : undefined,
    editError,
    sampledColor,
  };
}
function requireEngine() {
  if (!engine || disposed) throw new Error('Engine is unavailable');
  return engine;
}

function fatal(message: string) {
  self.postMessage({ type: 'engine-fatal', message });
}
function requireIdle() {
  const e = requireEngine();
  if (busy || snapshot().painting) throw new Error('Finish or cancel the current operation first');
  return e;
}
function cancelled(token: number) {
  if (disposed || token !== job) throw new Error('Operation cancelled');
}
async function readImageFile(file: File): Promise<ArrayBuffer> {
  const reader = new FileReader();
  activeImageReader = reader;
  try {
    return await new Promise<ArrayBuffer>((resolve, reject) => {
      reader.onload = () => resolve(reader.result as ArrayBuffer);
      reader.onerror = () => reject(reader.error ?? new Error('Could not read this image file'));
      reader.onabort = () => reject(new Error('Operation cancelled'));
      reader.readAsArrayBuffer(file);
    });
  } finally {
    if (activeImageReader === reader) activeImageReader = undefined;
    reader.onload = reader.onerror = reader.onabort = null;
    reader.abort();
  }
}
async function importImage(input: File | ArrayBuffer, fileName: string): Promise<DocumentSnapshot> {
  const e = requireIdle();
  const fromFile = !(input instanceof ArrayBuffer);
  const length = input instanceof ArrayBuffer ? input.byteLength : input.size;
  if (!Number.isSafeInteger(length) || length < 0 || length > 128 * CHUNK)
    throw new Error('Image files must be at most 128 MiB');
  busy = true;
  operation = 'Reading image…';
  const token = beginJob();
  const finishFileJob = beginFileJob();
  const priorExternal = externalBytes;
  const pauseBeforeCommit = pauseNextImport;
  pauseNextImport = false;
  try {
    // Admission precedes file reading. Include an encoded File/Blob owner and
    // its read buffer; Rust separately admits its argument and owned input copy.
    external(priorExternal + 2 * length + 2 * CHUNK);
    let bytes = input instanceof ArrayBuffer ? input : await readImageFile(input);
    cancelled(token);
    operation = 'Decoding image…';
    e.begin_import(new Uint8Array(bytes));
    bytes = new ArrayBuffer(0);
    input = new ArrayBuffer(0);
    external(priorExternal + (fromFile ? length : 0));
    while (true) {
      await yieldProjectTask();
      cancelled(token);
      if (e.import_step()) break;
    }
    if (pauseBeforeCommit) {
      importPaused = true;
      await new Promise<void>((resolve) => {
        releaseImportPause = resolve;
      });
      importPaused = false;
      releaseImportPause = undefined;
    }
    cancelled(token);
    e.finish_import();
    cancelAllowed = false;
    name = fileName;
    inputsWaiting = [];
    dirty = true;
    cursorScreen = undefined;
    editError = undefined;
    sampledColor = undefined;
    operation = undefined;
    // Only FramePump may present the newly committed candidate.
    busy = false;
    await new Promise<void>((resolve, reject) => {
      presentation = { resolve, reject };
    });
    return snapshot();
  } finally {
    try {
      if (token === job) {
        e.cancel_job();
        external(priorExternal);
        busy = false;
        operation = undefined;
      }
    } finally {
      finishFileJob();
    }
  }
}
async function recompose(token = job) {
  const e = requireEngine();
  if (!e.composite_needed() || !e.composite_begin()) return;
  await runComposite(token);
}
async function runComposite(token = job, committing = false) {
  const e = requireEngine();
  let done = false;
  while (!done) {
    cancelled(token);
    await pages.hydrate(e, 'render');
    const readback = e.composite_gpu_begin();
    if (readback) {
      const bytes = (await readback) as Uint8Array;
      cancelled(token);
      done = e.composite_gpu_accept(bytes);
    } else done = e.composite_step();
    if (done && committing) cancelAllowed = false;
    await yieldProjectTask();
  }
  dirty = true;
}
async function readStream(
  file: File,
  commit: boolean,
  token = job,
  existing?: { id: string; file: File },
) {
  cancelled(token);
  const e = requireEngine();
  const priorExternal = externalBytes;
  external(priorExternal + 2 * CHUNK);
  try {
    const start = performance.now();
    const source =
      existing ??
      (commit
        ? await pages.copy(file, () => cancelled(token))
        : { id: crypto.randomUUID().replaceAll('-', ''), file });
    cancelled(token);
    const copied = performance.now();
    e.stream_read_begin(source.file.size, source.id);
    for (let offset = 0; offset < source.file.size;) {
      cancelled(token);
      const bytes = new Uint8Array(await source.file.slice(offset, offset + CHUNK).arrayBuffer());
      for (let used = 0; used < bytes.length;) {
        let count: number;
        try {
          count = e.stream_read_feed(bytes.subarray(used));
        } catch (error) {
          throw new Error(
            String(error) +
              ` · input offset ${offset + used}, original ${file.size}, backing ${source.file.size}`,
            { cause: error },
          );
        }
        if (!count) throw new Error('Streamed project reader made no progress');
        used += count;
      }
      offset += bytes.length;
      await yieldProjectTask();
    }
    cancelled(token);
    e.stream_read_finish(commit);
    if (commit) cancelAllowed = false;
    const validated = performance.now();
    if (commit) {
      await recompose(token);
      await pages.collect(e);
      projectTiming = {
        copyMs: copied - start,
        validateMs: validated - copied,
        compositeMs: performance.now() - validated,
      };
    }
  } finally {
    if (!disposed && token === job) external(priorExternal);
  }
}
async function readZip(file: File, commit: boolean, token: number) {
  const e = requireEngine();
  const priorExternal = externalBytes;
  external(priorExternal + 2 * CHUNK);
  let created: string | undefined;
  let registered = false;
  try {
    const { id, handle } = await pages.create();
    created = id;
    cancelled(token);
    e.zip_read_begin(file.size);
    const access = await handle.createSyncAccessHandle();
    let written = 0;
    try {
      for (;;) {
        cancelled(token);
        const output = e.zip_read_output();
        if (output) {
          let at = 0;
          while (at < output.length) {
            const count = access.write(output.subarray(at), { at: written + at });
            if (!Number.isInteger(count) || count <= 0 || count > output.length - at)
              throw new Error('Project decompression storage write failed');
            at += count;
          }
          written += output.length;
        }
        const request = JSON.parse(e.zip_read_request()) as {
          offset: number;
          length: number;
        } | null;
        if (!request) break;
        const bytes = new Uint8Array(
          await file.slice(request.offset, request.offset + request.length).arrayBuffer(),
        );
        cancelled(token);
        e.zip_read_feed(bytes);
        await yieldProjectTask();
      }
      if (e.zip_read_finish() !== written) throw new Error('Project decompression length mismatch');
      access.truncate(written);
      access.flush();
    } finally {
      access.close();
    }
    cancelled(token);
    const source = await pages.register(id, handle, written);
    registered = true;
    cancelled(token);
    await readStream(source.file, commit, token, source);
    if (!commit) {
      await pages.discard(id);
      registered = false;
    }
  } finally {
    if (created) {
      const retained =
        registered && !disposed && (JSON.parse(e.page_stores()) as string[]).includes(created);
      if (!retained) await pages.discard(created);
    }
    if (!disposed && token === job) external(priorExternal);
  }
}

async function writeZip(write: (bytes: Uint8Array) => Promise<void>, token: number) {
  const e = requireEngine();
  const maskView = e.mask_view_enabled();
  fileMaskView = maskView;
  let written = 0;
  const priorExternal = externalBytes;
  const timing = {
    serial: (projectSaveTiming?.serial ?? 0) + 1,
    totalMs: 0,
    hydrateMs: 0,
    encodeMs: 0,
    writeMs: 0,
    yieldMs: 0,
    writes: 0,
    packets: 0,
    bytes: 0,
  };
  const started = performance.now();
  const fault = projectSaveFault;
  projectSaveFault = undefined;
  try {
    // One batching buffer, one browser write copy and bounded bridge/paging
    // scratch coexist. Admit before allocating or beginning the Rust writer.
    external(priorExternal + 16 * CHUNK);
    const pending = new Uint8Array(4 * CHUNK);
    let buffered = 0;
    const flush = async () => {
      if (!buffered) return;
      cancelled(token);
      const start = performance.now();
      await write(pending.subarray(0, buffered));
      if (timing.writes === 0 && fault) {
        if (fault === 'quota')
          throw new DOMException('Diagnostic project write failure', 'QuotaExceededError');
        projectSavePaused = true;
        try {
          await new Promise<void>((resolve) => {
            releaseProjectSavePause = resolve;
          });
        } finally {
          releaseProjectSavePause = undefined;
          projectSavePaused = false;
        }
      }
      cancelled(token);
      timing.writeMs += performance.now() - start;
      timing.writes++;
      buffered = 0;
    };
    if (maskView) {
      e.preview_mask(false);
      await recompose(token);
    }
    e.zip_write_begin();
    for (;;) {
      cancelled(token);
      let start = performance.now();
      await pages.hydrate(e, 'write');
      timing.hydrateMs += performance.now() - start;
      start = performance.now();
      const bytes = e.zip_write_next();
      timing.encodeMs += performance.now() - start;
      if (!bytes) break;
      if (bytes.length) {
        if (bytes.length > pending.length - buffered) await flush();
        pending.set(bytes, buffered);
        buffered += bytes.length;
        written += bytes.length;
        timing.packets++;
      }
      start = performance.now();
      await yieldProjectTask();
      timing.yieldMs += performance.now() - start;
    }
    await flush();
    timing.totalMs = performance.now() - started;
    timing.bytes = written;
    projectSaveTiming = timing;
    return written;
  } finally {
    if (token === job) {
      e.cancel_job();
      external(priorExternal);
      fileMaskView = false;
      if (maskView) {
        e.preview_mask(true);
        await recompose(token);
      }
    }
  }
}
async function writeStream(
  write: (bytes: Uint8Array) => Promise<void>,
  token = job,
  fault?: RecoveryFault,
) {
  const e = requireEngine();
  for (;;) {
    cancelled(token);
    await pages.hydrate(e, 'write');
    const bytes = e.stream_write_next();
    if (!bytes) break;
    if (fault === 'quota')
      throw new DOMException('Diagnostic storage quota exceeded', 'QuotaExceededError');
    await write(bytes);
    if (fault === 'pause-after-chunk') await new Promise<void>(() => {});
    if (fault === 'partial') throw new Error('Diagnostic interrupted snapshot write');
    await yieldProjectTask();
  }
}
async function writeRecoveryStream(handle: SyncHandle, token: number, fault?: RecoveryFault) {
  const access = await handle.createSyncAccessHandle();
  let offset = 0;
  try {
    await writeStream(
      async (bytes) => {
        let written = 0;
        while (written < bytes.length) {
          const count = access.write(bytes.subarray(written), { at: offset + written });
          if (!Number.isInteger(count) || count <= 0 || count > bytes.length - written)
            throw new Error('Recovery storage quota or write failure');
          written += count;
        }
        offset += bytes.length;
      },
      token,
      fault,
    );
    access.truncate(offset);
    access.flush();
    if (fault === 'after-flush')
      throw new Error('Diagnostic interruption before generation commit');
  } finally {
    access.close();
  }
}
async function readProject(input: Uint8Array | File, commit: boolean, token = job) {
  const e = requireEngine();
  if (input instanceof File) {
    const header = await input.slice(0, 12).arrayBuffer();
    if (header.byteLength >= 4 && new DataView(header).getUint32(0, true) === 0x04034b50) {
      await readZip(input, commit, token);
      return;
    }
    if (header.byteLength === 12 && new DataView(header).getUint32(8, true) === 2) {
      await readStream(input, commit, token);
      return;
    }
    if (input.size > 128 * CHUNK) throw new Error('Legacy project exceeds its 128 MiB file limit');
    e.admit_project_buffer(input.size);
    input = new Uint8Array(await input.arrayBuffer());
  }
  const bytes = input;
  const priorExternal = externalBytes;
  try {
    external(priorExternal + bytes.length);
    if (commit) e.begin_project_read(bytes);
    else e.begin_project_validation(bytes);
    let done = false;
    while (!done) {
      const deadline = performance.now() + 4;
      for (let i = 0; i < 64 && !done && performance.now() < deadline; i++)
        done = e.project_read_step();
      await yieldProjectTask();
      if (disposed || token !== job) throw new Error('Operation cancelled');
    }
    if (commit) {
      e.finish_project_read(true);
      cancelAllowed = false;
    }
  } finally {
    if (!disposed) {
      e.cancel_job();
      external(priorExternal);
    }
  }
}
async function writeProject(token = job, recovery = false) {
  const e = requireEngine();
  if (e.active_cold()) await pages.hydrate(e, 'active');
  e.begin_project_write(recovery);
  let done = false;
  while (!done) {
    const deadline = performance.now() + 4;
    for (let i = 0; i < 64 && !done && performance.now() < deadline; i++)
      done = e.project_write_step();
    await yieldProjectTask();
    if (disposed || token !== job) throw new Error('Operation cancelled');
  }
  const bytes = e.finish_project_write();
  external(bytes.length);
  return bytes;
}
function replaced(fileName: string) {
  name = fileName;
  inputsWaiting = [];
  dirty = true;
  cursorScreen = undefined;
  editError = undefined;
  sampledColor = undefined;
}
async function runPreview(
  adjustment: Adjustment | SpatialFilter,
  target: AdjustmentPreviewTarget,
  filter = false,
) {
  const e = requireIdle();
  const request = ++previewRequest;
  const token = job;
  busy = true;
  previewRunning = true;
  let finishPreview: () => void = () => {};
  const completion = new Promise<void>((resolve) => {
    finishPreview = resolve;
  });
  previewCompletion = completion;
  cancelAllowed = true;
  const fault = previewFault;
  previewFault = undefined;
  const priorExternal = externalBytes;
  const current = () => {
    cancelled(token);
    if (request !== previewRequest) throw new Error('Preview cancelled');
  };
  const start = performance.now();
  let preparationMs = 0;
  let cold = false;
  try {
    if (fault === 'budget') {
      e.preview_cancel();
      previewKey = undefined;
      const memory = snapshot().memory;
      external(priorExternal + 768 * 1048576 - memory.document - memory.history - memory.temporary);
    }
    const state = snapshot();
    const key = JSON.stringify([state.revision, state.view, size, target, filter]);
    if (previewKey !== key || !e.preview_valid()) {
      cold = true;
      if (filter) e.filter_preview_begin(JSON.stringify(target));
      else e.preview_begin(JSON.stringify(target));
      previewKey = key;
      let slice = performance.now();
      while (!e.preview_ready()) {
        current();
        await pages.hydrate(e, 'preview');
        current();
        e.preview_step();
        if (performance.now() - slice >= 4) {
          await yieldProjectTask();
          slice = performance.now();
        }
      }
      preparationMs = performance.now() - start;
    }
    current();
    if (fault === 'pause-before-render') {
      operation = 'Preview ready to render (diagnostic)…';
      await new Promise<void>((resolve) => {
        releasePreviewPause = resolve;
      });
      current();
    }
    const updated = performance.now();
    if (filter) e.filter_preview_update(JSON.stringify(adjustment));
    else e.preview_update(JSON.stringify(adjustment));
    let done = false;
    while (!done) {
      current();
      await pages.hydrate(e, 'preview-render');
      current();
      done = e.preview_render_step();
    }
    const submitted = performance.now();
    dirty = true;
    await new Promise<void>((resolve, reject) => {
      presentation = { resolve, reject };
    });
    current();
    const presented = performance.now();
    if (cold) {
      // Include asynchronous GPU pipeline initialization in preparation.
      if (fault === 'hold-gpu-wait') operation = 'Preview GPU wait (diagnostic)…';
      await e.preview_settled(fault === 'hold-gpu-wait');
      current();
      preparationMs += performance.now() - presented;
    }
    return {
      serial: request,
      preparationMs,
      updateMs: presented - updated,
      renderSubmitMs: submitted - updated,
      frameWaitMs: presented - submitted,
      width: Math.min(e.width(), 2048),
      height: Math.min(e.height(), 2048),
    };
  } catch (error) {
    if (!disposed && token === job && request === previewRequest) {
      previewKey = undefined;
      e.preview_cancel();
      dirty = true;
    }
    throw error;
  } finally {
    if (token === job) {
      external(priorExternal);
      busy = false;
      operation = undefined;
      releasePreviewPause = undefined;
    }
    finishPreview();
    if (previewCompletion === completion) {
      previewRunning = false;
      previewCompletion = undefined;
    }
  }
}

async function runFilter(begin: (engine: Engine) => void) {
  const e = requireIdle();
  busy = true;
  operation = 'Applying filter…';
  const token = beginJob();
  const start = performance.now();
  const fault = adjustmentFault;
  adjustmentFault = undefined;
  const priorExternal = externalBytes;
  let kernelMs = 0;
  try {
    if (fault === 'budget') {
      const m = snapshot().memory;
      external(priorExternal + 768 * 1048576 - m.document - m.history - m.temporary);
    }
    begin(e);
    let slice = performance.now();
    while (!e.filter_ready()) {
      cancelled(token);
      await pages.hydrate(e, 'filter');
      cancelled(token);
      const t = performance.now();
      e.filter_step();
      kernelMs += performance.now() - t;
      if (fault === 'budget-after-step') {
        const memory = snapshot().memory;
        external(
          // Refuse this diagnostic allocation immediately after a real step.
          // A tiny job may finish in that step and release its scratch before
          // the next allocation, so merely filling the budget is insufficient.
          priorExternal + 768 * 1048576 - memory.document - memory.history - memory.temporary + 1,
        );
      }
      if (performance.now() - slice >= 4) {
        await yieldProjectTask();
        slice = performance.now();
      }
    }
    if (fault === 'pause-before-commit') {
      operation = 'Filter ready (diagnostic)…';
      await new Promise<void>((resolve) => {
        releaseAdjustmentPause = resolve;
      });
    }
    cancelled(token);
    e.filter_commit();
    cancelAllowed = false;
    await recompose(token);
    filterTiming = {
      kernelMs,
      totalMs: performance.now() - start,
      cpuPeakBytes: e.filter_cpu_peak(),
    };
    dirty = true;
    return snapshot();
  } finally {
    if (token === job) {
      external(priorExternal);
      releaseAdjustmentPause = undefined;
      e.filter_cancel();
      busy = false;
      operation = undefined;
    }
  }
}
async function runGeometry(begin: (engine: Engine) => void) {
  const e = requireIdle();
  busy = true;
  operation = 'Updating geometry…';
  const token = beginJob();
  const start = performance.now();
  const fault = adjustmentFault;
  adjustmentFault = undefined;
  const priorExternal = externalBytes;
  let kernelMs = 0;
  try {
    await pages.persist(e);
    cancelled(token);
    if (fault === 'budget') {
      const m = snapshot().memory;
      external(priorExternal + 768 * 1048576 - m.document - m.history - m.temporary);
    }
    begin(e);
    let slice = performance.now();
    let persistedTemporary = snapshot().memory.temporary;
    while (!e.geometry_ready()) {
      cancelled(token);
      await pages.hydrate(e, 'geometry');
      cancelled(token);
      const t = performance.now();
      e.geometry_step();
      kernelMs += performance.now() - t;
      if (fault === 'budget-after-step') {
        const m = snapshot().memory;
        external(priorExternal + 768 * 1048576 - m.document - m.history - m.temporary + 1);
      }
      if (performance.now() - slice >= 4) {
        const temporary = snapshot().memory.temporary;
        if (temporary - persistedTemporary >= 32 * CHUNK) {
          await pages.persist(e);
          persistedTemporary = temporary;
        }
        await yieldProjectTask();
        slice = performance.now();
      }
    }
    if (fault === 'pause-before-commit') {
      operation = 'Geometry ready (diagnostic)…';
      await new Promise<void>((resolve) => {
        releaseAdjustmentPause = resolve;
      });
    }
    cancelled(token);
    e.geometry_commit();
    cancelAllowed = false;
    await recompose(token);
    geometryTiming = {
      kernelMs,
      totalMs: performance.now() - start,
      cpuPeakBytes: e.geometry_cpu_peak(),
    };
    dirty = true;
    return snapshot();
  } finally {
    if (token === job) {
      external(priorExternal);
      releaseAdjustmentPause = undefined;
      e.geometry_cancel();
      busy = false;
      operation = undefined;
    }
  }
}
async function runTransformPreview(update: TransformUpdate) {
  const e = requireIdle();
  const request = ++previewRequest;
  const token = job;
  busy = true;
  previewRunning = true;
  cancelAllowed = true;
  let finishPreview: () => void = () => {};
  const completion = new Promise<void>((resolve) => {
    finishPreview = resolve;
  });
  previewCompletion = completion;
  const fault = previewFault;
  previewFault = undefined;
  const priorExternal = externalBytes;
  const current = () => {
    cancelled(token);
    if (request !== previewRequest) throw new Error('Preview cancelled');
  };
  const start = performance.now();
  let preparationMs = 0;
  let started = false;
  try {
    if (fault === 'budget') {
      const m = snapshot().memory;
      external(priorExternal + 768 * 1048576 - m.document - m.history - m.temporary);
    }
    e.transform_update(JSON.stringify(update));
    started = true;
    const cold = !e.transform_source_ready() || !e.transform_background_ready();
    let slice = performance.now();
    while (!e.transform_source_ready()) {
      current();
      await pages.hydrate(e, 'transform-source');
      current();
      e.transform_source_step();
      if (performance.now() - slice >= 4) {
        await yieldProjectTask();
        slice = performance.now();
      }
    }
    while (!e.transform_background_ready()) {
      current();
      await pages.hydrate(e, 'transform-background');
      current();
      e.transform_background_step();
      if (performance.now() - slice >= 4) {
        await yieldProjectTask();
        slice = performance.now();
      }
    }
    if (cold) preparationMs = performance.now() - start;
    if (fault === 'pause-before-render') {
      operation = 'Transform preview ready (diagnostic)…';
      await new Promise<void>((resolve) => {
        releasePreviewPause = resolve;
      });
      current();
    }
    const updated = performance.now();
    let done = false;
    while (!done) {
      current();
      await pages.hydrate(e, 'transform-render');
      current();
      done = e.transform_render_step();
    }
    const submitted = performance.now();
    dirty = true;
    await new Promise<void>((resolve, reject) => {
      presentation = { resolve, reject };
    });
    current();
    const presented = performance.now();
    if (!e.transform_preview_valid()) throw new Error('Transform preview view changed');
    if (cold) {
      if (fault === 'hold-gpu-wait') operation = 'Transform GPU wait (diagnostic)…';
      await e.preview_settled(fault === 'hold-gpu-wait');
      current();
      preparationMs += performance.now() - presented;
    }
    if (!e.transform_preview_valid()) throw new Error('Transform preview view changed');
    return {
      serial: request,
      preparationMs,
      updateMs: presented - updated,
      renderSubmitMs: submitted - updated,
      frameWaitMs: presented - submitted,
      width: e.transform_preview_width(),
      height: e.transform_preview_height(),
    };
  } catch (error) {
    if (
      started &&
      !disposed &&
      token === job &&
      request === previewRequest &&
      !/Transform preview view changed/.test(String(error))
    ) {
      e.transform_cancel();
      dirty = true;
    }
    throw error;
  } finally {
    if (token === job) {
      external(priorExternal);
      busy = false;
      operation = undefined;
      releasePreviewPause = undefined;
    }
    finishPreview();
    if (previewCompletion === completion) {
      previewRunning = false;
      previewCompletion = undefined;
    }
  }
}
const api: EngineApi = {
  diagnosticProjectSaveFault(fault) {
    if (!diagnostic) throw new Error('Project diagnostics are disabled');
    projectSaveFault = fault;
  },
  diagnosticExportBudget() {
    if (!diagnostic) throw new Error('Export diagnostics are disabled');
    exportBudgetFault = true;
  },
  async exportImage(options) {
    const e = requireIdle();
    busy = true;
    operation = 'Exporting image…';
    const token = beginJob();
    const finishFileJob = beginFileJob();
    const priorExternal = externalBytes;
    const maskView = e.mask_view_enabled();
    const budgetFault = exportBudgetFault;
    exportBudgetFault = false;
    let created: string | undefined;
    let complete = false;
    let helper: ReturnType<typeof createCodecWorker> | undefined;
    const started = performance.now();
    let codecHeapPeakBytes = 0;
    let codecHeapLimitBytes = 0;
    let codecBaseline = 0;
    try {
      fileMaskView = maskView;
      if (maskView) {
        e.preview_mask(false);
        await recompose(token);
      }
      const document = snapshot().document!;
      e.begin_image_export(JSON.stringify(options));
      while (true) {
        await yieldProjectTask();
        cancelled(token);
        await pages.hydrate(e, 'export');
        if (options.format === 'png' ? e.export_step() : e.capture_step()) break;
      }
      const { id, handle } = await pages.create();
      const captureMs = performance.now() - started;
      created = id;
      cancelled(token);
      let length: number;
      if (options.format === 'png') {
        length = e.finish_export_file();
        external(priorExternal + 8 * CHUNK);
        const access = await handle.createSyncAccessHandle();
        try {
          for (let offset = 0; offset < length;) {
            cancelled(token);
            const bytes = codec_chunk(offset);
            if (!bytes.length) throw new Error('Incomplete encoded image');
            let written = 0;
            while (written < bytes.length) {
              const count = access.write(bytes.subarray(written), { at: offset + written });
              if (!Number.isInteger(count) || count <= 0 || count > bytes.length - written)
                throw new Error('Image export storage write failed');
              written += count;
            }
            offset += bytes.length;
            await yieldProjectTask();
          }
          access.truncate(length);
          access.flush();
        } finally {
          access.close();
        }
      } else {
        const pixels = e.finish_capture();
        external(priorExternal + pixels.byteLength + 8 * CHUNK);
        await pages.persist(e);
        cancelled(token);
        e.page_evict(64 * CHUNK, '[]');
        const budget = budgetFault
          ? Math.min(256 * 1024, e.export_codec_budget())
          : e.export_codec_budget();
        // Reserve the helper's entire heap allowance in the owner while it runs.
        codecHeapLimitBytes = budget;
        codecBaseline = 768 * CHUNK - e.export_codec_budget();
        external(externalBytes + budget);
        helper = createCodecWorker();
        operation = 'Encoding ' + (options.format === 'webp' ? 'WebP' : 'JPEG') + '…';
        const endpoint = helper;
        const worker = endpoint.port;
        length = await new Promise<number>((resolve, reject) => {
          cancelCodec = () => {
            endpoint.terminate();
            reject(new Error('Operation cancelled'));
          };
          codecErrors.set(endpoint.id, reject);
          worker.onmessageerror = () => reject(new Error('Could not read an image codec response'));
          worker.onmessage = ({
            data,
          }: MessageEvent<{ length?: number; peak?: number; error?: string }>) => {
            codecHeapPeakBytes = data.peak ?? 0;
            if (data.error) reject(new Error(data.error));
            else if (
              !Number.isInteger(data.length) ||
              data.length! <= 0 ||
              data.length! > 128 * CHUNK
            )
              reject(new Error('Invalid encoded image length'));
            else resolve(data.length!);
          };
          worker.postMessage(
            {
              pixels: pixels.buffer,
              width: document.width,
              height: document.height,
              dpi: document.dpi,
              options,
              handle,
              budget,
            },
            [pixels.buffer],
          );
          worker.start();
        });
        helper.terminate();
        helper = undefined;
        cancelCodec = undefined;
      }
      cancelled(token);
      const file = await handle.getFile();
      if (file.size !== length || length > 128 * CHUNK) throw new Error('Incomplete image export');
      const extension = options.format === 'jpeg' ? 'jpg' : options.format;
      complete = true;
      imageExportTiming = {
        serial: (imageExportTiming?.serial ?? 0) + 1,
        captureMs,
        encodeAndWriteMs: performance.now() - started - captureMs,
        codecHeapPeakBytes,
        codecHeapLimitBytes,
        logicalCpuAtCodecPeakBytes: codecBaseline + codecHeapPeakBytes,
        bytes: length,
      };
      return new File([file], name.replace(/\.[^.]+$/, '') + '.' + extension, {
        type: 'image/' + options.format,
      });
    } finally {
      try {
        helper?.terminate();
        cancelCodec = undefined;
        if (!complete && created) await pages.discard(created);
        if (token === job) {
          e.cancel_job();
          external(priorExternal);
          if (maskView) {
            e.preview_mask(true);
            await recompose(token);
          }
          fileMaskView = false;
          busy = false;
          operation = undefined;
        }
      } finally {
        finishFileJob();
      }
    }
  },
  async hitTest(x, y) {
    const e = requireIdle();
    busy = true;
    operation = 'Selecting layer…';
    const token = beginJob();
    try {
      e.hit_test_begin(x, y);
      let done = false,
        slice = performance.now();
      while (!done) {
        cancelled(token);
        await pages.hydrate(e, 'hit-test');
        cancelled(token);
        done = e.hit_test_step();
        if (performance.now() - slice >= 4) {
          await yieldProjectTask();
          slice = performance.now();
        }
      }
      return e.hit_test_finish() || null;
    } finally {
      if (token === job) {
        e.hit_test_cancel();
        busy = false;
        operation = undefined;
      }
    }
  },
  geometryAction(action) {
    return runGeometry((e) => e.geometry_begin(JSON.stringify(action)));
  },
  async beginTransform(target, selected) {
    const e = requireIdle();
    busy = true;
    operation = 'Preparing transform…';
    const token = beginJob();
    try {
      e.transform_begin(target, selected);
      let slice = performance.now();
      while (!e.transform_bounds_ready()) {
        cancelled(token);
        await pages.hydrate(e, 'transform-bounds');
        cancelled(token);
        e.transform_bounds_step();
        if (performance.now() - slice >= 4) {
          await yieldProjectTask();
          slice = performance.now();
        }
      }
      dirty = true;
      return snapshot();
    } catch (error) {
      if (token === job) {
        e.transform_cancel();
        dirty = true;
      }
      throw error;
    } finally {
      if (token === job) {
        busy = false;
        operation = undefined;
      }
    }
  },
  previewTransform(update) {
    return runTransformPreview(update);
  },
  applyTransform() {
    return runGeometry((e) => e.transform_apply_begin());
  },
  async cancelTransform() {
    await api.endAdjustmentPreview();
    return snapshot();
  },
  async compareFilterCompositors(index?: number) {
    if (!diagnostic) throw new Error('Diagnostics are disabled');
    const e = requireIdle();
    busy = true;
    const errors: number[] = [];
    try {
      const remaining: SpatialFilter[] = [
        { kind: 'motion-blur', radius: 7, angle: 31 },
        { kind: 'motion-blur', radius: 64, angle: -137 },
        { kind: 'high-pass', radius: 3 },
        { kind: 'high-pass', radius: 64 },
        { kind: 'add-noise', amount: 0.4, seed: 123, monochrome: false, distribution: 'uniform' },
        {
          kind: 'add-noise',
          amount: 0.4,
          seed: 4294967295,
          monochrome: true,
          distribution: 'uniform',
        },
        { kind: 'add-noise', amount: 0.4, seed: 123, monochrome: false, distribution: 'gaussian' },
        {
          kind: 'add-noise',
          amount: 0.4,
          seed: 4294967295,
          monochrome: true,
          distribution: 'gaussian',
        },
        { kind: 'emboss', radius: 4, angle: -41, amount: 1.3 },
        { kind: 'emboss', radius: 64, angle: 135, amount: 5 },
        { kind: 'pixelate', size: 37 },
        { kind: 'pixelate', size: 64 },
        { kind: 'motion-blur', radius: 1, angle: 60 },
        { kind: 'emboss', radius: 1, angle: 60, amount: 1 },
      ];
      for (const filter of index === undefined
        ? [
            { kind: 'gaussian', radius: 20 },
            { kind: 'box-blur', radius: 20 },
            { kind: 'unsharp-mask', radius: 3, amount: 1.3, threshold: 7 },
            { kind: 'sharpen', amount: 1.2 },
            { kind: 'median', radius: 1 },
            { kind: 'gaussian', radius: 0 },
          ]
        : remaining.slice(index, index + 1)) {
        for (let tile = 0; tile < 9; tile++) {
          const bytes = (await e.filter_golden_begin(JSON.stringify(filter), tile)) as Uint8Array;
          errors.push(e.golden_compare(bytes));
          await yieldProjectTask();
        }
      }
      for (const filter of index === undefined
        ? [
            { kind: 'gaussian', radius: 64 },
            { kind: 'median', radius: 5 },
          ]
        : []) {
        const bytes = (await e.filter_golden_begin(JSON.stringify(filter), 0)) as Uint8Array;
        errors.push(e.golden_compare(bytes));
        await yieldProjectTask();
      }
      return errors;
    } finally {
      busy = false;
    }
  },
  async diagnosticPreviewFault(fault) {
    if (!diagnostic) throw new Error('Diagnostics are disabled');
    previewFault = fault;
  },
  async previewAdjustment(adjustment, target) {
    return runPreview(adjustment, target);
  },
  async previewFilter(filter, target) {
    return runPreview(filter, { kind: 'pixels', target }, true);
  },
  async applyFilter(filter) {
    return runFilter((e) => e.filter_begin(JSON.stringify(filter)));
  },
  async fadeFilter(opacity, blend) {
    return runFilter((e) => e.fade_begin(opacity, JSON.stringify(blend)));
  },
  async endAdjustmentPreview() {
    const completion = previewCompletion;
    ++previewRequest;
    releasePreviewPause?.();
    previewKey = undefined;
    if (!disposed && engine) engine.preview_cancel();
    if (previewRunning) {
      presentation?.resolve();
      presentation = undefined;
    }
    dirty = true;
    await completion;
  },
  async diagnosticAdjustmentFault(fault) {
    if (!diagnostic) throw new Error('Diagnostics are disabled');
    adjustmentFault = fault;
  },
  async applyAdjustment(adjustment) {
    const e = requireIdle();
    busy = true;
    operation = 'Applying adjustment…';
    const token = beginJob();
    const start = performance.now();
    const fault = adjustmentFault;
    adjustmentFault = undefined;
    const priorExternal = externalBytes;
    let kernelMs = 0;
    try {
      if (fault === 'budget') {
        const memory = JSON.parse(e.snapshot()).memory as DocumentSnapshot['memory'];
        external(
          priorExternal + 768 * 1048576 - memory.document - memory.history - memory.temporary,
        );
      }
      e.adjustment_begin(JSON.stringify(adjustment));
      let sliceStart = performance.now();
      while (!e.adjustment_ready()) {
        cancelled(token);
        await pages.hydrate(e, 'adjustment');
        cancelled(token);
        const stepStart = performance.now();
        e.adjustment_step();
        kernelMs += performance.now() - stepStart;
        if (performance.now() - sliceStart >= 4) {
          await yieldProjectTask();
          sliceStart = performance.now();
        }
      }
      if (fault === 'pause-before-commit') {
        operation = 'Adjustment ready to commit (diagnostic)…';
        await new Promise<void>((resolve) => {
          releaseAdjustmentPause = resolve;
        });
      }
      cancelled(token);
      e.adjustment_commit();
      cancelAllowed = false;
      await recompose(token);
      adjustmentTiming = { kernelMs, totalMs: performance.now() - start };
      dirty = true;
      return snapshot();
    } finally {
      if (token === job) {
        external(priorExternal);
        releaseAdjustmentPause = undefined;
        e.adjustment_cancel();
        busy = false;
        operation = undefined;
      }
    }
  },
  async histogram(target) {
    const e = requireIdle();
    if (histogramRunning) throw new Error('Histogram is already updating');
    histogramRunning = true;
    const revision = snapshot().contentRevision;
    const start = performance.now();
    try {
      e.histogram_begin(target);
      let done = false;
      let sliceStart = performance.now();
      while (!done) {
        if (disposed || busy || e.painting() || snapshot().contentRevision !== revision)
          throw new Error('Histogram source changed; refresh when idle');
        await pages.hydrate(e, 'histogram');
        if (disposed || busy || snapshot().contentRevision !== revision)
          throw new Error('Histogram source changed; refresh when idle');
        done = e.histogram_step();
        if (performance.now() - sliceStart >= 4) {
          await yieldProjectTask();
          sliceStart = performance.now();
        }
      }
      return { ...JSON.parse(e.histogram_finish()), milliseconds: performance.now() - start };
    } finally {
      if (!disposed) e.histogram_cancel();
      histogramRunning = false;
    }
  },
  async diagnosticSelectionFault(fault) {
    if (!diagnostic) throw new Error('Diagnostics are disabled');
    selectionFault = fault;
  },
  async selectionAction(action) {
    const e = requireIdle();
    busy = true;
    operation = 'Updating selection…';
    const token = beginJob();
    const start = performance.now();
    let kernelMs = 0;
    const stages = [0, 0, 0, 0, 0];
    const maskView = e.mask_view_enabled();
    const fault = selectionFault;
    selectionFault = undefined;
    const priorExternal = externalBytes;
    try {
      if (fault === 'budget') {
        const memory = JSON.parse(e.snapshot()).memory as DocumentSnapshot['memory'];
        external(
          priorExternal + 768 * 1048576 - memory.document - memory.history - memory.temporary,
        );
      }
      if ('allLayers' in action && action.allLayers && maskView) {
        e.preview_mask(false);
        await recompose(token);
      }
      const encoded = JSON.stringify(action);
      if (action.kind === 'fill' || action.kind === 'bucket') {
        if (e.quick_mask_enabled()) await pages.hydrate(e, 'selection-mask');
        else if (snapshot().layers.maskTarget) await pages.hydrate(e, 'active');
      }
      e.selection_prepare(encoded);
      if (action.kind === 'wand' || action.kind === 'bucket')
        await pages.hydrate(e, 'selection-seed');
      cancelled(token);
      e.selection_begin(encoded);
      let sliceStart = performance.now();
      for (;;) {
        cancelled(token);
        if (e.selection_cold()) await pages.hydrate(e, 'selection');
        cancelled(token);
        const stepStart = performance.now();
        const phase = e.selection_phase();
        if (fault === 'pause-before-commit' && phase === 3) {
          operation = 'Selection ready to commit (diagnostic)…';
          await new Promise<void>((resolve) => {
            releaseSelectionPause = resolve;
          });
          cancelled(token);
        }
        const done = e.selection_step();
        const elapsed = performance.now() - stepStart;
        kernelMs += elapsed;
        stages[phase] += elapsed;
        if (done) break;
        if (performance.now() - sliceStart >= 4) {
          await yieldProjectTask();
          sliceStart = performance.now();
        }
      }
      cancelAllowed = false;
      await recompose(token);
      selectionTiming = { kernelMs, totalMs: performance.now() - start, stages };
      dirty = true;
      return snapshot();
    } finally {
      if (token === job) {
        external(priorExternal);
        releaseSelectionPause = undefined;
        e.selection_cancel();
        if (maskView && action.kind !== 'paste') {
          e.preview_mask(true);
          await recompose(token);
        }
        busy = false;
        operation = undefined;
      }
    }
  },
  async quickMask(enabled) {
    const e = requireIdle();
    e.quick_mask(enabled);
    dirty = true;
    return snapshot();
  },
  async clipboardPng() {
    const e = requireIdle();
    busy = true;
    operation = 'Copying PNG to clipboard…';
    const token = beginJob();
    try {
      e.clipboard_export_begin();
      for (;;) {
        cancelled(token);
        await pages.hydrate(e, 'export');
        if (e.export_step()) break;
        await yieldProjectTask();
      }
      const bytes = e.finish_export();
      return transfer(bytes, [bytes.buffer]);
    } finally {
      if (token === job) {
        e.cancel_job();
        busy = false;
        operation = undefined;
      }
    }
  },
  async clipboardImport(bytes) {
    const e = requireIdle();
    busy = true;
    operation = 'Reading clipboard image…';
    const token = beginJob();
    try {
      e.begin_import(new Uint8Array(bytes));
      bytes = new ArrayBuffer(0);
      for (;;) {
        cancelled(token);
        if (e.clipboard_import_step()) break;
        await yieldProjectTask();
      }
    } finally {
      if (token === job) {
        e.cancel_job();
        busy = false;
        operation = undefined;
      }
    }
  },
  async thumbnail(target) {
    const e = requireEngine();
    if (busy || thumbnailRunning || e.painting())
      throw new Error('Thumbnail deferred while editing');
    thumbnailRunning = true;
    const token = job;
    try {
      e.thumbnail_begin(target);
      let done = false;
      while (!done) {
        cancelled(token);
        if (busy || e.painting()) throw new Error('Thumbnail deferred while editing');
        await pages.hydrate(e, 'thumbnail');
        cancelled(token);
        if (busy || e.painting()) throw new Error('Thumbnail deferred while editing');
        done = e.thumbnail_step();
        await yieldProjectTask();
      }
      const bytes = e.thumbnail_finish();
      return transfer(bytes, [bytes.buffer]);
    } finally {
      if (!disposed) e.thumbnail_cancel();
      thumbnailRunning = false;
    }
  },
  async compareCompositors() {
    if (!diagnostic) throw new Error('Diagnostics are disabled');
    const e = requireIdle();
    busy = true;
    const errors: number[] = [];
    try {
      for (let mode = 0; mode < 16; mode++)
        for (let scenario = 0; scenario < 4; scenario++) {
          const bytes = (await e.golden_begin(mode, scenario, undefined)) as Uint8Array;
          errors.push(e.golden_compare(bytes));
          await yieldProjectTask();
        }
      return errors;
    } finally {
      busy = false;
    }
  },
  async compareAdjustmentCompositors(remaining = false, index?: number) {
    if (!diagnostic) throw new Error('Diagnostics are disabled');
    const e = requireIdle();
    busy = true;
    const errors: number[] = [];
    const adjustments = remaining
      ? [
          { kind: 'vibrance', amount: 0.7 },
          {
            kind: 'color-balance',
            shadows: [-0.3, 0.1, 0.4],
            midtones: [0.2, -0.4, 0.1],
            highlights: [0.3, 0.1, -0.2],
            preserveLuminosity: true,
          },
          {
            kind: 'color-balance',
            shadows: [0.4, -0.7, 0.3],
            midtones: [-0.6, 0.2, 0.4],
            highlights: [0.3, -0.4, 0.7],
            preserveLuminosity: false,
          },
          { kind: 'black-white', weights: [0.4, 0.8, 0.3, 0.7, -0.2, 1.1] },
          { kind: 'invert' },
          { kind: 'threshold', level: 128 },
          { kind: 'posterize', levels: 5 },
          {
            kind: 'gradient-map',
            stops: [
              [0, 0.2, 0, 0.4],
              [0.37, 1, 0.1, 0.2],
              [1, 0.8, 1, 0.2],
            ],
          },
        ]
      : [
          { kind: 'brightness-contrast', brightness: 0.17, contrast: -0.3 },
          {
            kind: 'levels',
            black: 0.12,
            white: 0.85,
            gamma: 1.7,
            outputBlack: 0.04,
            outputWhite: 0.94,
          },
          {
            kind: 'curves',
            points: [
              [0, 0.9],
              [0.3, 0.2],
              [0.8, 0.7],
              [1, 0.1],
            ],
          },
          { kind: 'exposure', stops: 1.3, offset: -0.04, gamma: 0.8 },
          { kind: 'hue-saturation', hue: -127, saturation: 0.27, lightness: -0.17 },
        ];
    try {
      for (const adjustment of index === undefined
        ? adjustments
        : adjustments.slice(index, index + 1))
        for (let mode = 0; mode < 16; mode++)
          for (let scene = 0; scene < 5; scene++) {
            const bytes = (await e.golden_begin(
              mode,
              scene,
              JSON.stringify(adjustment),
            )) as Uint8Array;
            errors.push(e.golden_compare(bytes));
            await yieldProjectTask();
          }
      return errors;
    } finally {
      busy = false;
    }
  },
  async selectLayer(target, mask = false, view = false) {
    const e = requireIdle();
    busy = true;
    try {
      e.select_layer(target);
      if (mask) e.select_mask(target, view);
      if (mask || !e.layer_is_group(target)) await pages.hydrate(e, 'active');
      await recompose();
      return snapshot();
    } finally {
      busy = false;
    }
  },
  async mergeLayers(kind, target) {
    const e = requireIdle();
    busy = true;
    operation = 'Merging layers…';
    const token = beginJob();
    cancelAllowed = true;
    try {
      await pages.persist(e);
      cancelled(token);
      e.merge_begin(kind, target);
      await runComposite(token, true);
      await pages.persist(e);
      await recompose(token);
      dirty = true;
      return snapshot();
    } finally {
      if (token === job) {
        busy = false;
        operation = undefined;
      }
    }
  },
  async layerAction(action) {
    const e = requireIdle();
    busy = true;
    operation = 'Updating layers…';
    const token = beginJob();
    cancelAllowed = true;
    try {
      await pages.persist(e);
      cancelled(token);
      e.layer_action(JSON.stringify(action));
      cancelAllowed = false;
      await recompose(token);
      dirty = true;
      return snapshot();
    } finally {
      if (token === job) {
        busy = false;
        operation = undefined;
      }
    }
  },
  async geometryMetadata(action) {
    const e = requireIdle();
    busy = true;
    operation = 'Updating canvas…';
    const token = beginJob();
    try {
      await pages.persist(e);
      cancelled(token);
      e.geometry_metadata(JSON.stringify(action));
      cancelAllowed = false;
      await recompose(token);
      dirty = true;
      return snapshot();
    } finally {
      if (token === job) {
        busy = false;
        operation = undefined;
      }
    }
  },
  async applyMask(target) {
    const e = requireIdle();
    busy = true;
    operation = 'Applying mask…';
    const token = beginJob();
    cancelAllowed = true;
    try {
      e.select_layer(target);
      if (e.layer_is_group(target)) {
        e.merge_begin('mask', target);
        await runComposite(token, true);
      } else {
        await pages.hydrate(e, 'active');
        cancelled(token);
        e.apply_mask(target);
        cancelAllowed = false;
      }
      await pages.persist(e);
      await recompose(token);
      return snapshot();
    } finally {
      if (token === job) {
        busy = false;
        operation = undefined;
      }
    }
  },
  async configureHistory(bytes, steps) {
    const e = requireIdle();
    busy = true;
    try {
      e.history_limits(bytes, steps);
      await pages.collect(e);
    } finally {
      busy = false;
    }
  },
  async historyJump(target) {
    const e = requireIdle();
    busy = true;
    try {
      e.history_jump(target);
      await recompose();
      dirty = true;
    } finally {
      busy = false;
    }
  },
  async resolution(dpi, gesture) {
    requireIdle().resolution(dpi, gesture);
  },
  async openProject(buffer, fileName) {
    const e = requireIdle();
    busy = true;
    operation = 'Opening project…';
    const token = beginJob();
    const finishFileJob = beginFileJob();
    try {
      if (buffer.byteLength > 128 * CHUNK)
        throw new Error('Use streamed project opening for files larger than 128 MiB');
      e.admit_project_buffer(buffer.byteLength);
      const layered =
        buffer.byteLength >= 12 &&
        (new DataView(buffer).getUint32(8, true) === 2 ||
          new DataView(buffer).getUint32(0, true) === 0x04034b50);
      await readProject(
        layered ? new File([buffer], fileName) : new Uint8Array(buffer),
        true,
        token,
      );
      buffer = new ArrayBuffer(0);
      replaced(fileName);
      return snapshot();
    } finally {
      try {
        if (token === job) {
          e.cancel_job();
          busy = false;
          operation = undefined;
        }
      } finally {
        finishFileJob();
      }
    }
  },
  async openProjectFile(file) {
    const e = requireIdle();
    busy = true;
    operation = 'Opening project…';
    const token = beginJob();
    const finishFileJob = beginFileJob();
    try {
      await readProject(file, true, token);
      replaced(file.name);
      return snapshot();
    } finally {
      try {
        if (token === job) {
          e.cancel_job();
          busy = false;
          operation = undefined;
        }
      } finally {
        finishFileJob();
      }
    }
  },
  async saveProjectFile(handle) {
    const e = requireIdle();
    if (!/\.cgfoto$/i.test(handle.name)) throw new Error('Choose a .cgfoto file for this project.');
    busy = true;
    operation = 'Saving project…';
    const token = beginJob();
    const finishFileJob = beginFileJob();
    let writable: FileSystemWritableFileStream | undefined;
    try {
      writable = await handle.createWritable();
      const output = writable;
      const size = await writeZip(async (bytes) => {
        await output.write(bytes as Uint8Array<ArrayBuffer>);
      }, token);
      cancelled(token);
      cancelAllowed = false;
      await writable.close();
      writable = undefined;
      name = handle.name;
      return size;
    } finally {
      try {
        await writable?.abort().catch(() => {});
        if (token === job) {
          e.cancel_job();
          busy = false;
          operation = undefined;
        }
      } finally {
        finishFileJob();
      }
    }
  },
  async saveProjectDownload() {
    const e = requireIdle();
    busy = true;
    operation = 'Preparing project download…';
    const token = beginJob();
    const finishFileJob = beginFileJob();
    let created: string | undefined;
    let complete = false;
    try {
      const { id, handle } = await pages.create();
      created = id;
      cancelled(token);
      const access = await handle.createSyncAccessHandle();
      let offset = 0;
      try {
        const size = await writeZip(async (bytes) => {
          let at = 0;
          while (at < bytes.length) {
            const count = access.write(bytes.subarray(at), { at: offset + at });
            if (!Number.isInteger(count) || count <= 0 || count > bytes.length - at)
              throw new Error('Project download storage write failed');
            at += count;
          }
          offset += bytes.length;
        }, token);
        if (size !== offset) throw new Error('Project download length mismatch');
        access.truncate(offset);
        access.flush();
      } finally {
        access.close();
      }
      cancelled(token);
      const file = await handle.getFile();
      if (file.size !== offset) throw new Error('Incomplete project download');
      complete = true;
      return new File([file], name.replace(/\.[^.]+$/, '') + '.cgfoto', {
        type: 'application/octet-stream',
      });
    } finally {
      try {
        if (!complete && created) await pages.discard(created);
        if (token === job) {
          e.cancel_job();
          busy = false;
          operation = undefined;
        }
      } finally {
        finishFileJob();
      }
    }
  },
  async saveProject() {
    const e = requireIdle();
    busy = true;
    operation = 'Saving project…';
    const token = beginJob();
    try {
      const bytes = await writeProject(token);
      return transfer(bytes, [bytes.buffer]);
    } finally {
      if (token === job) {
        e.cancel_job();
        external(0);
        busy = false;
        operation = undefined;
      }
    }
  },
  async startRecovery(persistent) {
    requireIdle();
    busy = true;
    recovery = { state: 'starting', persistent, candidates: [] };
    try {
      store = new RecoveryStore(
        (bytes) => readProject(bytes, false),
        (size) => requireEngine().admit_project_buffer(size),
        (file) => readStream(file, false),
      );
      await store.init();
      const scan = await store.scan();
      recovery = {
        ...recovery,
        state: 'idle',
        candidates: scan.candidates,
        error: scan.invalid
          ? 'A damaged recovery generation was ignored; prior valid snapshots were checked.'
          : undefined,
      };
    } catch (error) {
      recovery = { ...recovery, state: 'unavailable', error: String(error) };
    } finally {
      busy = false;
    }
  },
  async autosave() {
    if (
      !store ||
      recovery.state === 'starting' ||
      recovery.state === 'unavailable' ||
      busy ||
      snapshot().painting
    )
      return;
    const state = snapshot();
    if (!state.document || state.contentRevision === savedRevision) return;
    const e = requireEngine();
    busy = true;
    operation = 'Autosaving…';
    const token = beginJob();
    recovery = { ...recovery, state: 'saving', error: undefined };
    const started = performance.now();
    try {
      const fault = recoveryFault;
      recoveryFault = undefined;
      const result = e.legacy_project()
        ? await store.save(await writeProject(token, true), name, fault)
        : await store.saveStream(e.stream_write_begin(), name, (handle) =>
            writeRecoveryStream(handle, token, fault),
          );
      if (disposed || token !== job) return;
      savedRevision = state.contentRevision;
      recovery = {
        ...recovery,
        state: 'saved',
        lastSavedAt: result.savedAt,
        lastSaveMs: performance.now() - started,
      };
    } catch (error) {
      if (!disposed && token === job)
        recovery = {
          ...recovery,
          state: 'error',
          error:
            String(error) +
            ' · Previous completed snapshots are retained. Retry autosave or save a project.',
        };
    } finally {
      if (!disposed && token === job) {
        e.cancel_job();
        external(0);
        busy = false;
        operation = undefined;
      }
    }
  },
  async restoreRecovery(file) {
    const e = requireIdle();
    const candidate = recovery.candidates.find((c) => c.file === file);
    if (!store || !candidate) throw new Error('Recovery snapshot is unavailable');
    busy = true;
    operation = 'Restoring session…';
    const token = beginJob();
    try {
      await store.restore(candidate, (bytes) => readProject(bytes, true, token));
      replaced(candidate.name);
      // Source generations survive until the user explicitly discards them.
      recovery = { ...recovery, candidates: recovery.candidates.filter((c) => c.file !== file) };
      return snapshot();
    } finally {
      if (token === job) {
        e.cancel_job();
        busy = false;
        operation = undefined;
      }
    }
  },
  async discardRecovery(file) {
    requireIdle();
    const candidate = recovery.candidates.find((c) => c.file === file);
    if (!store || !candidate) throw new Error('Recovery snapshot is unavailable');
    busy = true;
    try {
      await store.discard(candidate);
      recovery = { ...recovery, candidates: recovery.candidates.filter((c) => c.file !== file) };
    } finally {
      busy = false;
    }
  },
  async diagnosticRecoveryFault(fault) {
    if (!diagnostic) throw new Error('Diagnostics are disabled');
    recoveryFault = fault;
  },
  async init(canvas: OffscreenCanvas, options: InitOptions): Promise<EngineInfo> {
    if (engine || initializing || disposed)
      throw new Error('Engine worker cannot be initialized twice');
    initializing = true;
    diagnostic = options.diagnostic;
    if (diagnostic && options.failInitialization)
      throw new Error('Diagnostic initialization failure');
    // Worker-local fault injection lets browser tests exercise the real Rust
    // fallback path without changing browser flags or the renderer policy.
    if (diagnostic && options.simulateGpuFailure) {
      const gpu = Reflect.get(navigator, 'gpu') as
        { requestAdapter: (...args: unknown[]) => Promise<object | null> } | undefined;
      if (!gpu) throw new Error('GPU failure diagnostics require the WebGPU API');
      const requestAdapter = gpu.requestAdapter.bind(gpu);
      Object.defineProperty(gpu, 'requestAdapter', {
        value: async (...args: unknown[]) => {
          if (options.simulateGpuFailure === 'adapter') return null;
          const adapter = await requestAdapter(...args);
          if (adapter)
            Object.defineProperty(adapter, 'requestDevice', {
              value: () => Promise.reject(new Error('Diagnostic device request failure')),
            });
          return adapter;
        },
      });
    }
    await initWasm({ module_or_path: new URL('./pkg/fotoeditor_wasm_bg.wasm', import.meta.url) });
    const created = await Engine.create(
      canvas,
      options.backend,
      diagnostic && options.diagnosticTexture !== false,
      fatal,
    );
    if (disposed) {
      created.dispose();
      created.free();
      throw new Error('Engine disposed during initialization');
    }
    engine = created;
    size = { width: options.width, height: options.height };
    engine.resize(size.width, size.height, options.dpr ?? 1);
    const backend = engine.backend();
    if (backend !== 'WebGPU' && backend !== 'WebGL2')
      throw new Error('Unexpected GPU backend: ' + backend);
    return { backend, adapter: engine.adapter_name() };
  },
  resize(nextSize) {
    if (disposed) return;
    size = nextSize;
    engine?.resize(size.width, size.height, size.dpr ?? 1);
    dirty = true;
  },
  async frame(_timestamp, inputs, preview = false) {
    if (disposed || !engine) throw new Error('Engine is unavailable');
    await api.interact(inputs ?? []);
    const start = performance.now();
    try {
      if (!busy) {
        const wasPainting = engine.painting();
        if (engine.sampled_stroke()) {
          busy = true;
          try {
            await pages.hydrate(engine, 'stroke');
          } finally {
            busy = false;
          }
        }
        engine.stroke_step();
        if (wasPainting && !engine.painting()) {
          dirty = true;
          busy = true;
          try {
            if (!engine.legacy_project()) await pages.persist(engine);
            await pages.collect(engine);
          } finally {
            busy = false;
          }
        }
        busy = true;
        try {
          await recompose();
        } finally {
          busy = false;
        }
      }
    } catch (error) {
      editError = String(error);
      engine.stroke_cancel();
      dirty = true;
    }
    const painted = performance.now();
    if (!busy) {
      try {
        if (engine.selection_active() || engine.quick_mask_enabled())
          await pages.hydrate(engine, 'selection-mask');
        engine.selection_overlay_prepare();
      } catch (error) {
        editError = String(error);
        engine.selection_overlay_clear();
      }
    }
    // Input gets first use of the frame slot. A later preview request keeps an
    // active stroke on the display cadence even between spaced dabs.
    // Commands already submit their own GPU composition/readback work. Do not
    // queue old viewport/selection frames while they own admission, especially
    // on a slow adapter. A prepared live preview still needs one presentation
    // before it can release that admission slot.
    const pendingPresentation = presentation;
    const presented =
      (!busy || presentation !== undefined) &&
      (dirty ||
        refining ||
        engine.selection_active() ||
        engine.quick_mask_enabled() ||
        (preview && engine.painting()) ||
        engine.needs_render())
        ? engine.frame()
        : false;
    if (presented) dirty = false;
    // Capture the state/dimensions that this frame actually submitted. Commands
    // can change them while its asynchronous GPU fence waits.
    const submitted = performance.now();
    const state = snapshot();
    const snapshotMs = performance.now() - submitted;
    const width = engine.width(),
      height = engine.height();
    refining = state.refining > 0;
    if (presented) {
      // FramePump holds its slot until this RPC returns. Include GPU completion
      // so animation cannot queue frames faster than a slow adapter draws them.
      await engine.frame_settled();
      if (disposed || !engine) throw new Error('Engine is unavailable');
    }
    const rendered = performance.now();
    if (presented) {
      frames++;
      // A new preview can reach presentation while this frame's GPU fence
      // waits. Only acknowledge the preview that was submitted in this frame.
      if (presentation === pendingPresentation) {
        presentation?.resolve();
        presentation = undefined;
      }
    }
    state.timing = {
      paintMs: painted - start,
      renderMs: rendered - painted - snapshotMs,
      snapshotMs,
    };
    return {
      presented,
      // The RPC still waits for GPU completion, but this frame used the slot
      // in which it was submitted. A late fence must not consume the next
      // RAF's input slot as well.
      presentedAt: presented ? performance.timeOrigin + submitted : undefined,
      frames,
      width,
      height,
      snapshot: state,
    };
  },
  dispose() {
    releaseProjectSavePause?.();
    ++job;
    cancelCodec?.();
    cancelCodec = undefined;
    activeImageReader?.abort();
    activeImageReader = undefined;
    releaseImportPause?.();
    releaseImportPause = undefined;
    importPaused = false;
    releaseAdjustmentPause?.();
    releasePreviewPause?.();
    releaseAdjustmentPause = undefined;
    busy = false;
    presentation?.reject(new Error('Engine disposed'));
    presentation = undefined;
    disposed = true;
    store?.dispose();
    pages.dispose();
    engine?.dispose();
    engine?.free();
    engine = undefined;
  },
  diagnosticFault(kind) {
    if (!diagnostic || !engine) throw new Error('Diagnostics are disabled');
    if (kind === 'device-loss') engine.diagnostic_device_loss();
    else engine.diagnostic_panic();
  },
  async interact(inputs) {
    const e = requireEngine();
    let cancel = -1;
    inputs.forEach((input, index) => {
      if (input.kind === 'stroke-cancel') cancel = index;
    });
    if (cancel >= 0) {
      e.stroke_cancel();
      inputsWaiting = [];
      inputs = inputs.slice(cancel);
    }
    if (inputsWaiting.length + inputs.length > 4096) {
      inputsWaiting = [];
      e.stroke_cancel();
      dirty = true;
      editError = 'Stroke input queue exceeded; stroke cancelled';
      return;
    }
    const pending = [...inputsWaiting, ...inputs];
    inputsWaiting = [];
    for (let index = 0; index < pending.length; index++) {
      const input = pending[index];
      if (e.painting() && ['stroke-begin', 'clone-source', 'undo', 'redo'].includes(input.kind)) {
        inputsWaiting = pending.slice(index);
        break;
      }
      try {
        if (
          !['cursor', 'sample-color', 'stroke-begin', 'stroke-sample', 'stroke-end'].includes(
            input.kind,
          )
        )
          dirty = true;
        if (input.kind === 'cursor') cursorScreen = [input.x, input.y];
        else if (input.kind === 'clone-source') {
          if (busy || e.painting()) throw new Error('Finish the current stroke or operation first');
          e.clone_source(input.x, input.y);
          editError = undefined;
        } else if (input.kind === 'stroke-cancel') {
          e.stroke_cancel();
          if (input.amount < 0) editError = 'Stroke input queue exceeded; stroke cancelled';
        } else if (input.kind.startsWith('stroke-')) {
          if (input.kind !== 'stroke-begin' && !e.painting()) continue;
          if (busy) throw new Error('Finish the current operation first');
          if (input.kind === 'stroke-begin') {
            editError = undefined;
            if (input.brush?.clone?.allLayers || input.brush?.healing?.allLayers) {
              busy = true;
              try {
                await recompose();
              } finally {
                busy = false;
              }
            }
            if (e.quick_mask_enabled() || e.active_cold()) {
              busy = true;
              try {
                await pages.hydrate(e, e.quick_mask_enabled() ? 'selection-mask' : 'active');
              } finally {
                busy = false;
              }
            }
            e.stroke_begin(JSON.stringify(input.brush), input.x, input.y);
          }
          if (input.sample) e.stroke_sample(new Float64Array(input.sample));
          if (input.kind === 'stroke-end') e.stroke_end();
        } else if (input.kind === 'sample-color') {
          const rgba = e.sample_color(input.x, input.y);
          if (rgba.length === 4)
            sampledColor = {
              rgba: [rgba[0], rgba[1], rgba[2], rgba[3]],
              serial: (sampledColor?.serial ?? 0) + 1,
            };
        } else if (input.kind === 'undo' || input.kind === 'redo') {
          if (busy || e.painting()) throw new Error('Finish the current stroke or operation first');
          e[input.kind]();
          editError = undefined;
        } else if (!e.painting()) e.navigate(input.kind, input.x, input.y, input.amount);
      } catch (error) {
        editError = String(error);
        e.stroke_cancel();
        dirty = true;
      }
    }
  },
  newDocument(width, height, dpi) {
    if (busy || snapshot().painting || inputsWaiting.some((i) => i.kind.startsWith('stroke-')))
      throw new Error('Finish or cancel the current operation first');
    requireEngine().new_document(width, height, dpi);
    dirty = true;
    name = 'Untitled';
    cursorScreen = undefined;
    editError = undefined;
    sampledColor = undefined;
    inputsWaiting = [];
    return snapshot();
  },
  openImage(bytes, fileName) {
    return importImage(bytes, fileName);
  },
  openImageFile(file) {
    return importImage(file, file.name);
  },
  diagnosticImportFault() {
    if (!diagnostic) throw new Error('Import fault testing is disabled');
    pauseNextImport = true;
  },
  async exportPng() {
    if (busy || snapshot().painting)
      throw new Error('Finish or cancel the current operation first');
    busy = true;
    const token = beginJob();
    const e = requireEngine();
    const maskView = e.mask_view_enabled();
    try {
      if (maskView) {
        e.preview_mask(false);
        await recompose(token);
      }
      e.begin_export();
      while (true) {
        await yieldTask();
        if (token !== job || disposed) throw new Error('Operation cancelled');
        await pages.hydrate(e, 'export');
        if (e.export_step()) break;
      }
      const bytes = e.finish_export();
      return transfer(bytes, [bytes.buffer]);
    } finally {
      if (token === job) {
        e.cancel_job();
        if (maskView) {
          e.preview_mask(true);
          await recompose(token);
        }
        busy = false;
      }
    }
  },
  async cancelJob() {
    // A completed document replacement is atomic and cannot be cancelled after
    // commit while it waits for its first normal viewport frame.
    if (
      !busy ||
      !cancelAllowed ||
      operation === 'Autosaving…' ||
      operation === 'Restoring session…'
    )
      return false;
    const fileJob = activeFileJob;
    ++job;
    cancelCodec?.();
    cancelCodec = undefined;
    activeImageReader?.abort();
    activeImageReader = undefined;
    ++previewRequest;
    previewKey = undefined;
    requireEngine().preview_cancel();
    releasePreviewPause?.();
    releaseSelectionPause?.();
    releaseAdjustmentPause?.();
    releaseImportPause?.();
    releaseImportPause = undefined;
    importPaused = false;
    releaseAdjustmentPause = undefined;
    releaseSelectionPause = undefined;
    requireEngine().cancel_job();
    if (fileMaskView) {
      fileMaskView = false;
      requireEngine().preview_mask(true);
      dirty = true;
    }
    if (fileJob) {
      cancelAllowed = false;
      releaseProjectSavePause?.();
      operation = 'Cancelling file operation…';
      await fileJob;
      if (disposed) return true;
    }
    external(0);
    operation = undefined;
    busy = false;
    return true;
  },
};
expose(api);
