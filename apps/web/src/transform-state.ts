import { create } from 'zustand';
import type { EngineApi, TransformUpdate, Interpolation } from './engine/types';
import { useWorkspace, updateWorkspace } from './state';
import { useBrush } from './brush-state';
export type Point = [number, number];
export type Quad = [Point, Point, Point, Point];
export const useTransform = create<{
  mode: 'scale' | 'skew' | 'distort' | 'perspective';
  selected: boolean;
  autoSelect: boolean;
  pending: boolean;
  busy: boolean;
  notice: string;
  quad?: Quad;
  interpolation: Interpolation;
}>(() => ({
  mode: 'scale',
  selected: true,
  autoSelect: false,
  pending: false,
  busy: false,
  notice: '',
  interpolation: 'lanczos3',
}));
let owner: EngineApi | undefined;
let epoch = 0;
let queued: TransformUpdate | undefined;
let running: Promise<void> | undefined;
let nudges: Point = [0, 0];
let nudgeRunning: Promise<void> | undefined;
export function resetTransform() {
  owner = undefined;
  ++epoch;
  queued = undefined;
  running = undefined;
  nudges = [0, 0];
  nudgeRunning = undefined;
  useTransform.setState({ pending: false, busy: false, quad: undefined, notice: '' });
}
function ensure(api: EngineApi) {
  if (owner !== api) {
    owner = api;
    ++epoch;
    queued = undefined;
    running = undefined;
    nudges = [0, 0];
    nudgeRunning = undefined;
    useTransform.setState({ pending: false, busy: false, quad: undefined, notice: '' });
  }
}
async function admitted<T>(api: EngineApi, action: () => Promise<T>, token: number): Promise<T> {
  const deadline = performance.now() + 60000;
  for (;;) {
    if (owner !== api || token !== epoch) throw new Error('Transform cancelled');
    try {
      return await action();
    } catch (error) {
      if (
        !/Finish or cancel the current operation first/.test(String(error)) ||
        performance.now() > deadline
      )
        throw error;
      await new Promise((r) => setTimeout(r, 50));
    }
  }
}
export function transformQuad(): Quad | undefined {
  return (
    useTransform.getState().quad ??
    (useWorkspace.getState().snapshot?.transform?.quad as Quad | undefined)
  );
}
export function queueTransform(api: EngineApi, update: TransformUpdate) {
  ensure(api);
  if (useTransform.getState().busy) return;
  queued = update;
  useTransform.setState({ pending: true, notice: '' });
  if (running) return running;
  const token = epoch;
  running = (async () => {
    while (queued && epoch === token) {
      const update: TransformUpdate = queued;
      queued = undefined;
      try {
        await admitted(api, () => api.previewTransform(update), token);
      } catch (error) {
        if (epoch === token && /Transform preview view changed/.test(String(error))) {
          queued ??= update;
          await new Promise((r) => setTimeout(r, 16));
          continue;
        }
        if (epoch === token) useTransform.setState({ notice: String(error), quad: undefined });
      }
    }
  })().finally(() => {
    if (epoch === token) {
      running = undefined;
      useTransform.setState({ pending: false });
    }
  });
  return running;
}
export function setTransformQuad(api: EngineApi, quad: Quad) {
  ensure(api);
  useTransform.setState({ quad });
  return queueTransform(api, { kind: 'quad', points: quad });
}
export function mapQuad(quad: Quad, values: number[]): Quad {
  return quad.map(([x, y]) => {
    const w = values[6] * x + values[7] * y + values[8];
    return [
      (values[0] * x + values[1] * y + values[2]) / w,
      (values[3] * x + values[4] * y + values[5]) / w,
    ];
  }) as Quad;
}
export async function startTransform(api: EngineApi, selected = useTransform.getState().selected) {
  ensure(api);
  if (useTransform.getState().busy) return;
  useTransform.setState({ busy: true, notice: '' });
  const token = epoch;
  try {
    const state = useWorkspace.getState().snapshot;
    const target = state?.layers.active ?? state?.layers.nodes[0]?.id;
    if (target === undefined) throw new Error('Choose a raster layer');
    const snapshot = await admitted(api, () => api.beginTransform(target, selected), token);
    updateWorkspace({ snapshot });
    useBrush.setState({ tool: 'transform' });
    useTransform.setState({ interpolation: 'lanczos3', busy: false, quad: undefined });
    await queueTransform(api, { kind: 'matrix', matrix: snapshot.transform!.matrix });
  } catch (error) {
    if (epoch === token) useTransform.setState({ notice: String(error) });
  } finally {
    if (epoch === token) useTransform.setState({ busy: false });
  }
}
export async function applyTransform(api: EngineApi) {
  ensure(api);
  if (useTransform.getState().busy) return;
  const token = epoch;
  useTransform.setState({ busy: true, notice: '' });
  try {
    await running;
    const snapshot = await admitted(api, () => api.applyTransform(), token);
    updateWorkspace({ snapshot });
    useBrush.setState({ tool: 'move' });
    useTransform.setState({ quad: undefined });
  } catch (error) {
    if (epoch === token) useTransform.setState({ notice: String(error) });
  } finally {
    if (epoch === token) useTransform.setState({ busy: false });
  }
}
export async function cancelTransform(api: EngineApi, returnToMove = true) {
  ensure(api);
  const token = ++epoch;
  queued = undefined;
  nudges = [0, 0];
  nudgeRunning = undefined;
  useTransform.setState({ busy: true, quad: undefined });
  try {
    const snapshot = await api.cancelTransform();
    if (epoch === token) {
      updateWorkspace({ snapshot });
      if (returnToMove) useBrush.setState({ tool: 'move' });
    }
  } catch (error) {
    if (epoch === token) useTransform.setState({ notice: String(error) });
  } finally {
    if (epoch === token) {
      running = undefined;
      useTransform.setState({ busy: false, pending: false });
    }
  }
}
export function nudgeLayer(api: EngineApi, x: number, y: number) {
  ensure(api);
  if (useTransform.getState().busy && !nudgeRunning) return;
  nudges = [nudges[0] + x, nudges[1] + y];
  if (nudgeRunning) return nudgeRunning;
  const token = epoch;
  useTransform.setState({ busy: true, notice: '' });
  nudgeRunning = (async () => {
    while (epoch === token && (nudges[0] !== 0 || nudges[1] !== 0)) {
      const [x, y] = nudges;
      nudges = [0, 0];
      const s = useWorkspace.getState().snapshot;
      const target = s?.layers.active ?? s?.layers.nodes[0]?.id;
      if (target === undefined) throw new Error('Choose a raster layer');
      const snapshot = await admitted(
        api,
        () =>
          api.geometryAction({
            kind: 'layer-transform',
            target,
            matrix: { values: [1, 0, x, 0, 1, y, 0, 0, 1] },
            interpolation: useTransform.getState().interpolation,
            selection: useTransform.getState().selected && !!s?.selection.active,
          }),
        token,
      );
      if (epoch === token) updateWorkspace({ snapshot });
    }
  })()
    .catch((error) => {
      if (epoch === token) {
        nudges = [0, 0];
        useTransform.setState({ notice: String(error) });
      }
    })
    .finally(() => {
      if (epoch === token) {
        nudgeRunning = undefined;
        useTransform.setState({ busy: false });
      }
    });
  return nudgeRunning;
}
export async function selectLayerAtPoint(api: EngineApi, point: Point) {
  ensure(api);
  const token = epoch;
  useTransform.setState({ busy: true, notice: '' });
  try {
    const id = await admitted(api, () => api.hitTest(...point), token);
    if (id === null) return null;
    const snapshot = await admitted(api, () => api.selectLayer(id), token);
    updateWorkspace({ snapshot });
    return id;
  } finally {
    if (epoch === token) useTransform.setState({ busy: false });
  }
}
