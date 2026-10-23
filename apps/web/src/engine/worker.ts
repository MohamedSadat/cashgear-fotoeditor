import { expose, transfer } from 'comlink';
import type { EngineApi, EngineInfo, InitOptions, PhysicalSize, Interaction } from './types';
import initWasm, { Engine } from './pkg/fotoeditor_wasm';
import type { DocumentSnapshot } from './types';
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
let recovery: RecoveryStatus = { state: 'starting', persistent: 'unavailable', candidates: [] };
let store: RecoveryStore | undefined;
let savedRevision = -1;
let recoveryFault: RecoveryFault | undefined;
let externalBytes = 0;
let thumbnailRunning = false;
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
    operation,
    projectTiming,
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
async function readStream(file: File, commit: boolean, token = job) {
  const e = requireEngine();
  const priorExternal = externalBytes;
  external(priorExternal + 2 * CHUNK);
  try {
    const start = performance.now();
    const source = commit
      ? await pages.copy(file, () => cancelled(token))
      : { id: crypto.randomUUID().replaceAll('-', ''), file };
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
const api: EngineApi = {
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
          const bytes = (await e.golden_begin(mode, scenario)) as Uint8Array;
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
    try {
      if (buffer.byteLength > 128 * CHUNK)
        throw new Error('Use streamed project opening for files larger than 128 MiB');
      e.admit_project_buffer(buffer.byteLength);
      const layered = buffer.byteLength >= 12 && new DataView(buffer).getUint32(8, true) === 2;
      await readProject(
        layered ? new File([buffer], fileName) : new Uint8Array(buffer),
        true,
        token,
      );
      buffer = new ArrayBuffer(0);
      replaced(fileName);
      return snapshot();
    } finally {
      if (token === job) {
        e.cancel_job();
        busy = false;
        operation = undefined;
      }
    }
  },
  async openProjectFile(file) {
    const e = requireIdle();
    busy = true;
    operation = 'Opening project…';
    const token = beginJob();
    try {
      await readProject(file, true, token);
      replaced(file.name);
      return snapshot();
    } finally {
      if (token === job) {
        e.cancel_job();
        busy = false;
        operation = undefined;
      }
    }
  },
  async saveProjectFile(handle) {
    const e = requireIdle();
    busy = true;
    operation = 'Saving project…';
    const token = beginJob();
    let writable: FileSystemWritableFileStream | undefined;
    try {
      const size = e.stream_write_begin();
      writable = await handle.createWritable();
      const output = writable;
      await writeStream(async (bytes) => {
        await output.write(bytes as Uint8Array<ArrayBuffer>);
      }, token);
      cancelled(token);
      await writable.close();
      writable = undefined;
      return size;
    } finally {
      await writable?.abort().catch(() => {});
      if (token === job) {
        e.cancel_job();
        busy = false;
        operation = undefined;
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
    if (!state.document || state.revision === savedRevision) return;
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
      savedRevision = state.revision;
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
        engine.stroke_step();
        if (wasPainting && !engine.painting()) {
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
    // Input gets first use of the frame slot. A later preview request keeps an
    // active stroke on the display cadence even between spaced dabs.
    const presented =
      dirty || refining || (preview && engine.painting()) || engine.needs_render()
        ? engine.frame()
        : false;
    if (presented) dirty = false;
    const rendered = performance.now();
    if (presented) {
      frames++;
      presentation?.resolve();
      presentation = undefined;
    }
    const state = snapshot();
    refining = state.refining > 0;
    state.timing = {
      paintMs: painted - start,
      renderMs: rendered - painted,
      snapshotMs: performance.now() - rendered,
    };
    return {
      presented,
      presentedAt: presented ? performance.timeOrigin + rendered : undefined,
      frames,
      width: engine.width(),
      height: engine.height(),
      snapshot: state,
    };
  },
  dispose() {
    ++job;
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
      if (e.painting() && ['stroke-begin', 'undo', 'redo'].includes(input.kind)) {
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
        else if (input.kind === 'stroke-cancel') {
          e.stroke_cancel();
          if (input.amount < 0) editError = 'Stroke input queue exceeded; stroke cancelled';
        } else if (input.kind.startsWith('stroke-')) {
          if (input.kind !== 'stroke-begin' && !e.painting()) continue;
          if (busy) throw new Error('Finish the current operation first');
          if (input.kind === 'stroke-begin') {
            editError = undefined;
            if (e.active_cold()) {
              busy = true;
              try {
                await pages.hydrate(e, 'active');
              } finally {
                busy = false;
              }
            }
            e.stroke_begin(JSON.stringify(input.brush));
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
  async openImage(bytes, fileName) {
    if (busy || snapshot().painting)
      throw new Error('Finish or cancel the current operation first');
    busy = true;
    const token = beginJob();
    const e = requireEngine();
    try {
      e.begin_import(new Uint8Array(bytes));
      bytes = new ArrayBuffer(0);
      while (true) {
        await yieldTask();
        if (token !== job || disposed) throw new Error('Operation cancelled');
        if (e.import_step()) break;
      }
      name = fileName;
      inputsWaiting = [];
      dirty = true;
      cursorScreen = undefined;
      editError = undefined;
      sampledColor = undefined;
      // Only FramePump may present a surface. Presenting here as well could
      // acquire the same browser canvas texture twice within one animation frame.
      busy = false;
      await new Promise<void>((resolve, reject) => {
        presentation = { resolve, reject };
      });
      return snapshot();
    } finally {
      if (token === job) {
        e.cancel_job();
        busy = false;
      }
    }
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
      if (e.export_original() && e.active_cold()) await pages.hydrate(e, 'active');
      e.begin_export();
      while (true) {
        await yieldTask();
        if (token !== job || disposed) throw new Error('Operation cancelled');
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
  cancelJob() {
    // A completed document replacement is atomic and cannot be cancelled after
    // commit while it waits for its first normal viewport frame.
    if (
      !busy ||
      !cancelAllowed ||
      operation === 'Autosaving…' ||
      operation === 'Restoring session…'
    )
      return false;
    ++job;
    requireEngine().cancel_job();
    external(0);
    operation = undefined;
    busy = false;
    return true;
  },
};
expose(api);
