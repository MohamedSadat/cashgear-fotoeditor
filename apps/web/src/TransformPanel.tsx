import { useState } from 'react';
import type { EngineApi, Interpolation } from './engine/types';
import { useWorkspace } from './state';
import { useGeometry } from './geometry-state';
import {
  applyTransform,
  cancelTransform,
  startTransform,
  useTransform,
  queueTransform,
  setTransformQuad,
  transformQuad,
  mapQuad,
} from './transform-state';
export function TransformPanel({ api, disabled }: { api?: EngineApi; disabled: boolean }) {
  const session = useWorkspace((s) => s.snapshot?.transform);
  const selected = useWorkspace((s) => s.snapshot?.selection.active);
  const settings = useTransform();
  const snapping = useGeometry((s) => s.snapping);
  const [delta, setDelta] = useState({ x: 0, y: 0, sx: 100, sy: 100, angle: 0, kx: 0, ky: 0 });
  const operate = (kind: 'move' | 'scale' | 'rotate' | 'skew') => {
    if (!api) return;
    const q = transformQuad();
    if (!q) return;
    const center = q.reduce((p, q) => [p[0] + q[0] / 4, p[1] + q[1] / 4], [0, 0]);
    let a = 1,
      b = 0,
      c = 0,
      d = 1,
      tx = 0,
      ty = 0;
    if (kind === 'move') {
      tx = delta.x;
      ty = delta.y;
    }
    if (kind === 'scale') {
      a = delta.sx / 100;
      d = delta.sy / 100;
    }
    if (kind === 'rotate') {
      const angle = (delta.angle * Math.PI) / 180;
      a = d = Math.cos(angle);
      b = -Math.sin(angle);
      c = Math.sin(angle);
    }
    if (kind === 'skew') {
      b = Math.tan((delta.kx * Math.PI) / 180);
      c = Math.tan((delta.ky * Math.PI) / 180);
    }
    if (kind !== 'move') {
      tx = center[0] - a * center[0] - b * center[1];
      ty = center[1] - c * center[0] - d * center[1];
    }
    void setTransformQuad(api, mapQuad(q, [a, b, tx, c, d, ty, 0, 0, 1]));
  };
  const fields = (keys: (keyof typeof delta)[], labels: string[]) =>
    keys.map((key, i) => (
      <label key={key}>
        {labels[i]}
        <input
          aria-label={labels[i]}
          type="number"
          step="any"
          value={delta[key]}
          onChange={(e) => setDelta({ ...delta, [key]: Number(e.target.value) })}
        />
      </label>
    ));
  return (
    <section
      className="geometry-panel transform-panel"
      aria-label="Move and transform"
      onKeyDown={(e) => {
        if (
          session &&
          api &&
          (e.key === 'Escape' || (e.key === 'Enter' && e.target instanceof HTMLInputElement))
        ) {
          e.preventDefault();
          e.stopPropagation();
          if (e.key === 'Escape') void cancelTransform(api);
          else void applyTransform(api);
        }
      }}
    >
      <span className="section-eyebrow">MOVE & TRANSFORM</span>
      <label>
        <input
          type="checkbox"
          checked={settings.autoSelect}
          onChange={(e) => useTransform.setState({ autoSelect: e.target.checked })}
        />
        Auto-select layer
      </label>
      <label>
        <input
          type="checkbox"
          checked={snapping}
          onChange={(e) => useGeometry.setState({ snapping: e.target.checked })}
        />
        Snap to guides
      </label>
      {!session ? (
        <>
          <label>
            <input
              type="checkbox"
              disabled={!selected}
              checked={settings.selected && !!selected}
              onChange={(e) => useTransform.setState({ selected: e.target.checked })}
            />
            Transform selected pixels
          </label>
          <button
            disabled={disabled || settings.busy}
            onClick={() => api && void startTransform(api)}
          >
            Free transform
          </button>
        </>
      ) : (
        <>
          <p className="history-note">
            Drag a handle, or drag inside to move. Shift keeps proportions. Enter applies; Escape
            cancels.
          </p>
          <fieldset disabled={settings.busy}>
            <label>
              Handle mode
              <select
                aria-label="Transform handle mode"
                value={settings.mode}
                onChange={(e) =>
                  useTransform.setState({ mode: e.target.value as typeof settings.mode })
                }
              >
                {['scale', 'skew', 'distort', 'perspective'].map((m) => (
                  <option key={m}>{m}</option>
                ))}
              </select>
            </label>
            <label>
              Interpolation
              <select
                aria-label="Transform interpolation"
                value={settings.interpolation}
                onChange={(e) => {
                  const interpolation = e.target.value as Interpolation;
                  useTransform.setState({ interpolation });
                  if (api) void queueTransform(api, { kind: 'interpolation', interpolation });
                }}
              >
                {['nearest', 'bilinear', 'bicubic', 'lanczos3'].map((m) => (
                  <option key={m}>{m}</option>
                ))}
              </select>
            </label>
            {fields(['x', 'y'], ['Move X (px)', 'Move Y (px)'])}
            <button onClick={() => operate('move')}>Move by offset</button>
            {fields(['sx', 'sy'], ['Scale X (%)', 'Scale Y (%)'])}
            <button onClick={() => operate('scale')}>Scale by percent</button>
            {fields(['angle'], ['Rotate by (degrees)'])}
            <button onClick={() => operate('rotate')}>Rotate transform</button>
            {fields(['kx', 'ky'], ['Skew X (degrees)', 'Skew Y (degrees)'])}
            <button onClick={() => operate('skew')}>Skew transform</button>
            <button onClick={() => api && void applyTransform(api)}>Apply transform</button>
            <button onClick={() => api && void cancelTransform(api)}>Cancel transform</button>
          </fieldset>
        </>
      )}
      <p role="status" data-testid="transform-status">
        {settings.notice ||
          (settings.busy
            ? 'Updating transform…'
            : settings.pending
              ? 'Preparing preview…'
              : session
                ? 'Transform preview ready'
                : 'V move · Ctrl+T transform')}
      </p>
    </section>
  );
}
