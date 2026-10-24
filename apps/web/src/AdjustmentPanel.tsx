import { useEffect, useState } from 'react';
import type { Adjustment, EngineApi, Histogram, LayerNode } from './engine/types';
import { updateWorkspace, useWorkspace } from './state';

type Kind = Adjustment['kind'];
const names: Record<Kind, string> = {
  'brightness-contrast': 'Brightness / Contrast',
  levels: 'Levels',
  curves: 'Curves',
  exposure: 'Exposure',
  'hue-saturation': 'Hue / Saturation',
};
function defaults(kind: Kind): Adjustment {
  switch (kind) {
    case 'brightness-contrast':
      return { kind, brightness: 0, contrast: 0 };
    case 'levels':
      return { kind, black: 0, white: 1, gamma: 1, outputBlack: 0, outputWhite: 1 };
    case 'curves':
      return {
        kind,
        points: [
          [0, 0],
          [0.5, 0.5],
          [1, 1],
        ],
      };
    case 'exposure':
      return { kind, stops: 0, offset: 0, gamma: 1 };
    case 'hue-saturation':
      return { kind, hue: 0, saturation: 0, lightness: 0 };
  }
}
// Only control values and histogram bins cross this UI. Document pixels stay in Rust.
type Field = [string, string, number, number, number, number];
function fields(kind: Kind): Field[] {
  switch (kind) {
    case 'brightness-contrast':
      return [
        ['brightness', 'Brightness (%)', -100, 100, 1, 100],
        ['contrast', 'Contrast (%)', -100, 100, 1, 100],
      ];
    case 'levels':
      return [
        ['black', 'Input black', 0, 255, 1, 255],
        ['white', 'Input white', 0, 255, 1, 255],
        ['gamma', 'Midtone gamma', 0.1, 10, 0.01, 1],
        ['outputBlack', 'Output black', 0, 255, 1, 255],
        ['outputWhite', 'Output white', 0, 255, 1, 255],
      ];
    case 'exposure':
      return [
        ['stops', 'Exposure (stops)', -10, 10, 0.1, 1],
        ['offset', 'Linear offset (%)', -100, 100, 1, 100],
        ['gamma', 'Exposure gamma', 0.1, 10, 0.01, 1],
      ];
    case 'hue-saturation':
      return [
        ['hue', 'Hue (degrees)', -180, 180, 1, 1],
        ['saturation', 'Saturation (%)', -100, 100, 1, 100],
        ['lightness', 'Lightness (%)', -100, 100, 1, 100],
      ];
    default:
      return [];
  }
}
function find(nodes: LayerNode[], id?: number): LayerNode | undefined {
  for (const node of nodes) {
    if (node.id === id) return node;
    const child = find(node.children ?? [], id);
    if (child) return child;
  }
}
function plot(bins: number[]) {
  const max = Math.max(1, ...bins);
  return bins
    .map((count, i) => `${i === 0 ? 'M' : 'L'}${i} ${(64 - (count / max) * 62).toFixed(2)}`)
    .join(' ');
}
export function AdjustmentPanel({ api, disabled }: { api?: EngineApi; disabled: boolean }) {
  const snapshot = useWorkspace((s) => s.snapshot);
  const [adjustment, setAdjustment] = useState<Adjustment>(defaults('brightness-contrast'));
  const [applying, setApplying] = useState(false);
  const [notice, setNotice] = useState('');
  const [refresh, setRefresh] = useState(0);
  const [histogram, setHistogram] = useState<{ key: string; data: Histogram }>();
  const [histogramError, setHistogramError] = useState('');
  const active = snapshot?.layers.active ?? snapshot?.layers.nodes[0]?.id;
  const node = find(snapshot?.layers.nodes ?? [], active);
  const raster = !!node && node.children === null;
  const revision = snapshot?.contentRevision;
  const key = `${active}:${revision}`;
  const blocked = disabled || applying || !!snapshot?.operation;
  const colorTarget = raster && !snapshot?.layers.maskTarget && !snapshot?.selection.quickMask;
  useEffect(() => {
    if (!api || active === undefined || !raster || blocked) return;
    let current = true;
    let timer: ReturnType<typeof setTimeout>;
    const load = () => {
      void api
        .histogram(active)
        .then((data) => {
          if (current) {
            setHistogram({ key, data });
            setHistogramError('');
          }
        })
        .catch((e) => {
          if (!current) return;
          if (/already updating|source changed|current operation/i.test(String(e)))
            timer = setTimeout(load, 250);
          else setHistogramError(String(e));
        });
    };
    timer = setTimeout(load, 250);
    return () => {
      current = false;
      clearTimeout(timer);
    };
  }, [api, active, raster, blocked, key, refresh]);
  const data = histogram?.key === key ? histogram.data : undefined;
  const reset = () => {
    setAdjustment(defaults(adjustment.kind));
    setNotice('');
  };
  return (
    <section className="adjustment-panel" aria-label="Adjustments and histogram">
      <span className="section-eyebrow">COLOR & TONE</span>
      <h2>Adjustments</h2>
      <p className="history-note">
        Active raster pixels · selection coverage applies. Changes apply on confirmation.
      </p>
      <form
        onSubmit={(event) => {
          event.preventDefault();
          if (!api || blocked) return;
          setApplying(true);
          setNotice('');
          void api
            .applyAdjustment(adjustment)
            .then((next) => {
              updateWorkspace({ snapshot: next });
              setNotice('Adjustment applied. Undo is available in History.');
            })
            .catch((e) => setNotice(String(e)))
            .finally(() => setApplying(false));
        }}
      >
        <fieldset disabled={blocked || !colorTarget || !api}>
          <label>
            Adjustment
            <select
              aria-label="Adjustment"
              value={adjustment.kind}
              onChange={(e) => setAdjustment(defaults(e.target.value as Kind))}
            >
              {(Object.keys(names) as Kind[]).map((kind) => (
                <option key={kind} value={kind}>
                  {names[kind]}
                </option>
              ))}
            </select>
          </label>
          {fields(adjustment.kind).map(([field, label, min, max, step, scale]) => (
            <label key={field}>
              {label}
              <input
                type="number"
                required
                min={min}
                max={max}
                step={step}
                value={Number(
                  ((adjustment as unknown as Record<string, number>)[field] * scale).toFixed(5),
                )}
                onChange={(e) =>
                  setAdjustment({
                    ...adjustment,
                    [field]: Number(e.target.value) / scale,
                  } as Adjustment)
                }
              />
            </label>
          ))}
          {adjustment.kind === 'curves' && (
            <>
              <svg viewBox="0 0 100 100" aria-label="Master RGB curve" role="img">
                <path d="M0 100L100 0" />
                <polyline
                  points={adjustment.points
                    .map(([x, y]) => `${x * 100},${100 - y * 100}`)
                    .join(' ')}
                />
              </svg>
              <div className="curve-points">
                {adjustment.points.map((point, index) => (
                  <div key={index}>
                    {point.map((value, axis) => (
                      <label key={axis}>
                        {axis === 0 ? 'Input' : 'Output'} {index + 1}
                        <input
                          type="number"
                          required
                          min={0}
                          max={100}
                          step={0.1}
                          value={Number((value * 100).toFixed(4))}
                          disabled={
                            axis === 0 && (index === 0 || index === adjustment.points.length - 1)
                          }
                          onChange={(e) =>
                            setAdjustment({
                              ...adjustment,
                              points: adjustment.points.map((p, i) =>
                                i === index
                                  ? (p.map((v, a) =>
                                      a === axis ? Number(e.target.value) / 100 : v,
                                    ) as [number, number])
                                  : p,
                              ),
                            })
                          }
                        />
                      </label>
                    ))}
                    {index > 0 && index < adjustment.points.length - 1 && (
                      <button
                        type="button"
                        aria-label={`Remove curve point ${index + 1}`}
                        onClick={() =>
                          setAdjustment({
                            ...adjustment,
                            points: adjustment.points.filter((_, i) => i !== index),
                          })
                        }
                      >
                        ×
                      </button>
                    )}
                  </div>
                ))}
              </div>
              <button
                type="button"
                disabled={adjustment.points.length >= 16}
                onClick={() => {
                  const points = [...adjustment.points];
                  let index = 0;
                  for (let i = 1; i < points.length - 1; i++)
                    if (points[i + 1][0] - points[i][0] > points[index + 1][0] - points[index][0])
                      index = i;
                  points.splice(index + 1, 0, [
                    (points[index][0] + points[index + 1][0]) / 2,
                    (points[index][1] + points[index + 1][1]) / 2,
                  ]);
                  setAdjustment({ ...adjustment, points });
                }}
              >
                Add curve point
              </button>
            </>
          )}
          <div className="adjustment-actions">
            <button type="button" onClick={reset}>
              Reset
            </button>
            <button type="submit">Apply adjustment</button>
          </div>
        </fieldset>
      </form>
      {!colorTarget && snapshot?.document && (
        <p className="history-note">Select raster layer pixels to adjust color.</p>
      )}
      {notice && (
        <p role="status" className="history-note" data-testid="adjustment-notice">
          {notice}
        </p>
      )}
      <div className="histogram-panel" aria-label="Histogram">
        <h3>Histogram</h3>
        <p className="history-note">
          Full active raster · alpha weighted · before masks and layer effects
        </p>
        {data && (
          <>
            <svg viewBox="0 0 256 66" role="img" aria-label="Red green blue and luma histogram">
              {data.bins.map((bins, i) => (
                <path
                  key={i}
                  d={plot(bins)}
                  stroke={['#f17980', '#78c89b', '#7ea9f5', '#e4dfd7'][i]}
                />
              ))}
            </svg>
            <p className="history-note" data-testid="histogram-total">
              {data.pixels.toLocaleString()} nontransparent pixels · alpha weight{' '}
              {data.weight.toLocaleString()}
            </p>
          </>
        )}
        {!data && (
          <p className="history-note">
            {raster ? 'Waiting for an idle document…' : 'Choose a raster layer.'}
          </p>
        )}
        <button
          type="button"
          disabled={blocked || !raster || !api}
          onClick={() => {
            setHistogramError('');
            setRefresh((r) => r + 1);
          }}
        >
          Refresh histogram
        </button>
        {histogramError && (
          <p role="status" className="history-note">
            {histogramError}
          </p>
        )}
      </div>
    </section>
  );
}
