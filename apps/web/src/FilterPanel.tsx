import { useState } from 'react';
import type { EngineApi, LayerNode, SpatialFilter } from './engine/types';
import { updateWorkspace, useWorkspace } from './state';
import { useAdjustmentPreview } from './useAdjustmentPreview';
type Kind = SpatialFilter['kind'];
const names: Record<Kind, string> = {
  gaussian: 'Gaussian blur',
  'box-blur': 'Box blur',
  'unsharp-mask': 'Unsharp mask',
  sharpen: 'Sharpen',
  median: 'Median',
};
const modes = [
  'normal',
  'multiply',
  'screen',
  'overlay',
  'darken',
  'lighten',
  'color-dodge',
  'color-burn',
  'hard-light',
  'soft-light',
  'difference',
  'exclusion',
  'hue',
  'saturation',
  'color',
  'luminosity',
];
function defaults(kind: Kind): SpatialFilter {
  if (kind === 'sharpen') return { kind, amount: 1 };
  if (kind === 'unsharp-mask') return { kind, radius: 3, amount: 1, threshold: 0 };
  return { kind, radius: kind === 'median' ? 1 : 3 };
}
function find(nodes: LayerNode[], id?: number): LayerNode | undefined {
  for (const node of nodes) {
    if (node.id === id) return node;
    const child = find(node.children ?? [], id);
    if (child) return child;
  }
}
export function FilterPanel({ api, disabled }: { api?: EngineApi; disabled: boolean }) {
  const snapshot = useWorkspace((s) => s.snapshot);
  const previewEnabled = useWorkspace((s) => s.previewOwner === 'filter');
  const [filter, setFilter] = useState<SpatialFilter>(defaults('gaussian'));
  const [opacity, setOpacity] = useState(100);
  const [blend, setBlend] = useState('normal');
  const [applying, setApplying] = useState(false);
  const [notice, setNotice] = useState('');
  const active = snapshot?.layers.active ?? snapshot?.layers.nodes[0]?.id;
  const node = find(snapshot?.layers.nodes ?? [], active);
  const raster =
    !!node &&
    node.children === null &&
    !node.adjustment &&
    !snapshot?.layers.maskTarget &&
    !snapshot?.selection.quickMask;
  const blocked = disabled || applying || !!snapshot?.operation || !api;
  const preview = useAdjustmentPreview(
    api,
    previewEnabled && raster && !blocked,
    active === undefined ? undefined : { kind: 'pixels', target: active },
    filter,
    JSON.stringify([snapshot?.revision, snapshot?.view]),
    true,
  );
  const apply = async (fade = false) => {
    if (!api || blocked) return;
    setApplying(true);
    setNotice('');
    updateWorkspace({ previewOwner: undefined });
    try {
      await api.endAdjustmentPreview();
      const next = fade
        ? await api.fadeFilter(opacity / 100, blend)
        : await api.applyFilter(filter);
      updateWorkspace({ snapshot: next });
      setNotice(
        next.revision === snapshot?.revision
          ? 'No pixels changed.'
          : fade
            ? 'Fade applied.'
            : 'Filter applied. Fade is available until the next document edit.',
      );
    } catch (e) {
      setNotice(String(e));
    } finally {
      setApplying(false);
    }
  };
  const fields: [string, string, number, number, number][] = [];
  if ('radius' in filter)
    fields.push(['radius', 'Filter radius (px)', 0, filter.kind === 'median' ? 5 : 64, 1]);
  if ('amount' in filter) fields.push(['amount', 'Sharpen amount', 0, 5, 0.05]);
  if ('threshold' in filter) fields.push(['threshold', 'Unsharp threshold', 0, 255, 1]);
  return (
    <section className="filter-panel adjustment-panel" aria-label="Filters">
      <span className="section-eyebrow">DETAIL & TEXTURE</span>
      <h2>Filters</h2>
      <p className="history-note">
        Preview the visible area. Apply processes the full selection on this raster layer.
      </p>
      <form
        onSubmit={(e) => {
          e.preventDefault();
          void apply();
        }}
      >
        <fieldset disabled={blocked || !raster}>
          <label>
            Filter
            <select
              aria-label="Filter"
              value={filter.kind}
              onChange={(e) => setFilter(defaults(e.target.value as Kind))}
            >
              {(Object.keys(names) as Kind[]).map((kind) => (
                <option key={kind} value={kind}>
                  {names[kind]}
                </option>
              ))}
            </select>
          </label>
          <label className="check-label">
            <input
              type="checkbox"
              checked={previewEnabled}
              onChange={(e) =>
                updateWorkspace({ previewOwner: e.target.checked ? 'filter' : undefined })
              }
            />
            Live filter preview
          </label>
          {previewEnabled && (
            <p
              role="status"
              className="history-note"
              data-testid="filter-preview-status"
              data-settings={preview.completed}
              data-serial={preview.timing?.serial}
              data-preparation-ms={preview.timing?.preparationMs}
              data-update-ms={preview.timing?.updateMs}
              data-submit-ms={preview.timing?.renderSubmitMs}
              data-frame-wait-ms={preview.timing?.frameWaitMs}
            >
              {preview.error ||
                (preview.pending
                  ? 'Preparing filter preview…'
                  : preview.timing
                    ? 'Filter preview ready. Apply to keep this result.'
                    : 'Starting filter preview…')}
            </p>
          )}
          {fields.map(([field, label, min, max, step]) => (
            <label key={field}>
              {label}
              {(['number', 'range'] as const).map((type) => (
                <input
                  key={type}
                  type={type}
                  aria-label={label + (type === 'range' ? ' slider' : '')}
                  required
                  min={min}
                  max={max}
                  step={step}
                  value={(filter as unknown as Record<string, number>)[field]}
                  onChange={(e) =>
                    setFilter({ ...filter, [field]: Number(e.target.value) } as SpatialFilter)
                  }
                />
              ))}
            </label>
          ))}
          <div className="adjustment-actions">
            <button type="button" onClick={() => setFilter(defaults(filter.kind))}>
              Reset filter
            </button>
            <button type="submit">Apply filter</button>
          </div>
        </fieldset>
      </form>
      {!raster && snapshot?.document && (
        <p className="history-note">Select raster layer pixels to apply a filter.</p>
      )}
      <form
        aria-label="Fade last filter"
        onSubmit={(e) => {
          e.preventDefault();
          void apply(true);
        }}
      >
        <fieldset disabled={blocked || !snapshot?.canFade}>
          <h3>Fade last filter</h3>
          <label>
            Fade opacity (%)
            <input
              type="number"
              aria-label="Fade opacity (%)"
              min={0}
              max={100}
              step={1}
              value={opacity}
              onChange={(e) => setOpacity(Number(e.target.value))}
            />
          </label>
          <label>
            Fade blend mode
            <select
              aria-label="Fade blend mode"
              value={blend}
              onChange={(e) => setBlend(e.target.value)}
            >
              {modes.map((mode) => (
                <option key={mode} value={mode}>
                  {mode}
                </option>
              ))}
            </select>
          </label>
          <button type="submit">Apply Fade</button>
        </fieldset>
      </form>
      {notice && (
        <p role="status" className="history-note" data-testid="filter-notice">
          {notice}
        </p>
      )}
    </section>
  );
}
