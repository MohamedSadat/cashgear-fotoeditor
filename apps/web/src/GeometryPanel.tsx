import { useState } from 'react';
import type {
  EngineApi,
  GeometryMetadata,
  Guide,
  GeometryAction,
  Interpolation,
} from './engine/types';
import { updateWorkspace, useWorkspace } from './state';
import { useGeometry } from './geometry-state';

export function GeometryPanel({ api, disabled }: { api?: EngineApi; disabled: boolean }) {
  const doc = useWorkspace((s) => s.snapshot?.document);
  const [crop, setCrop] = useState({ x: 0, y: 0, width: 100, height: 100 });
  const [axis, setAxis] = useState<Guide['axis']>('vertical');
  const [position, setPosition] = useState(0);
  const [notice, setNotice] = useState('');
  const [busy, setBusy] = useState(false);
  const [size, setSize] = useState({ width: 100, height: 100, dpi: 72 });
  const [proportions, setProportions] = useState(true);
  const [interpolation, setInterpolation] = useState<Interpolation>('lanczos3');
  const [anchor, setAnchor] = useState('0.5,0.5');
  const [degrees, setDegrees] = useState(0);
  const { showGuides, showRulers } = useGeometry();
  const apply = async (action: GeometryMetadata | GeometryAction) => {
    if (!api || disabled || busy) return;
    setBusy(true);
    setNotice('');
    try {
      const snapshot = await (action.kind === 'crop' || action.kind === 'guides'
        ? api.geometryMetadata(action)
        : api.geometryAction(action));
      updateWorkspace({ snapshot });
      if (action.kind === 'crop' && snapshot.document)
        setCrop({ x: 0, y: 0, width: snapshot.document.width, height: snapshot.document.height });
    } catch (error) {
      setNotice(String(error));
    } finally {
      setBusy(false);
    }
  };
  return (
    <section className="geometry-panel" aria-label="Canvas and guides">
      <span className="section-eyebrow">CANVAS</span>
      <details>
        <summary
          onClick={(e) => {
            if (!e.currentTarget.parentElement?.hasAttribute('open') && doc)
              setSize({ width: doc.width, height: doc.height, dpi: doc.dpi });
          }}
        >
          Image and canvas size
        </summary>
        <fieldset disabled={disabled || busy}>
          {(['width', 'height'] as const).map((key) => (
            <label key={key}>
              New {key}
              <input
                aria-label={'New ' + key}
                type="number"
                min="1"
                max="16384"
                step="1"
                value={size[key]}
                onChange={(e) => {
                  const value = Number(e.target.value);
                  const other = key === 'width' ? 'height' : 'width';
                  setSize({
                    ...size,
                    [key]: value,
                    ...(proportions && doc
                      ? { [other]: Math.max(1, Math.round((value * doc[other]) / doc[key])) }
                      : {}),
                  });
                }}
              />
            </label>
          ))}
          <label>
            <input
              type="checkbox"
              checked={proportions}
              onChange={(e) => setProportions(e.target.checked)}
            />
            Keep proportions
          </label>
          <label>
            DPI
            <input
              aria-label="Resize DPI"
              type="number"
              min="1"
              max="2400"
              value={size.dpi}
              onChange={(e) => setSize({ ...size, dpi: Number(e.target.value) })}
            />
          </label>
          <label>
            Interpolation
            <select
              aria-label="Resize interpolation"
              value={interpolation}
              onChange={(e) => setInterpolation(e.target.value as Interpolation)}
            >
              <option value="nearest">Nearest</option>
              <option value="bilinear">Bilinear</option>
              <option value="bicubic">Bicubic</option>
              <option value="lanczos3">Lanczos3</option>
            </select>
          </label>
          <button onClick={() => void apply({ kind: 'image-size', ...size, interpolation })}>
            Resize image
          </button>
          <label>
            Canvas anchor
            <select
              aria-label="Canvas anchor"
              value={anchor}
              onChange={(e) => setAnchor(e.target.value)}
            >
              {[
                ['0,0', 'Top left'],
                ['0.5,0', 'Top'],
                ['1,0', 'Top right'],
                ['0,0.5', 'Left'],
                ['0.5,0.5', 'Center'],
                ['1,0.5', 'Right'],
                ['0,1', 'Bottom left'],
                ['0.5,1', 'Bottom'],
                ['1,1', 'Bottom right'],
              ].map(([value, label]) => (
                <option key={value} value={value}>
                  {label}
                </option>
              ))}
            </select>
          </label>
          <button
            onClick={() =>
              void apply({
                kind: 'canvas-size',
                width: size.width,
                height: size.height,
                anchor: anchor.split(',').map(Number) as [number, number],
              })
            }
          >
            Resize canvas
          </button>
          <p className="history-note">
            Canvas size keeps existing pixels. Image size resamples all layers, masks and the
            selection.
          </p>
        </fieldset>
      </details>
      <details>
        <summary>Rotate and flip canvas</summary>
        <fieldset disabled={disabled || busy}>
          <div className="geometry-actions">
            {[-90, 90, 180].map((degrees) => (
              <button
                key={degrees}
                onClick={() => void apply({ kind: 'canvas-rotate', degrees, interpolation })}
              >
                {degrees}°
              </button>
            ))}
          </div>
          <label>
            Angle (degrees)
            <input
              aria-label="Canvas rotation angle"
              type="number"
              value={degrees}
              onChange={(e) => setDegrees(Number(e.target.value))}
            />
          </label>
          <button onClick={() => void apply({ kind: 'canvas-rotate', degrees, interpolation })}>
            Rotate canvas
          </button>
          <button onClick={() => void apply({ kind: 'canvas-flip', horizontal: true })}>
            Flip canvas horizontally
          </button>
          <button onClick={() => void apply({ kind: 'canvas-flip', horizontal: false })}>
            Flip canvas vertically
          </button>
        </fieldset>
      </details>
      <details>
        <summary
          onClick={(e) => {
            if (!e.currentTarget.parentElement?.hasAttribute('open') && doc)
              setCrop({ x: 0, y: 0, width: doc.width, height: doc.height });
          }}
        >
          Crop canvas
        </summary>
        <p className="history-note">Outside pixels are kept. Reveal them again at any time.</p>
        <fieldset disabled={disabled || busy}>
          {(['x', 'y', 'width', 'height'] as const).map((key) => (
            <label key={key}>
              Crop {key}
              <input
                aria-label={'Crop ' + key}
                type="number"
                min={key === 'x' || key === 'y' ? 0 : 1}
                step="1"
                value={crop[key]}
                onChange={(e) => setCrop({ ...crop, [key]: Number(e.target.value) })}
              />
            </label>
          ))}
          <button
            onClick={() =>
              setCrop({ x: 0, y: 0, width: doc?.width ?? 1, height: doc?.height ?? 1 })
            }
          >
            Use current canvas
          </button>
          <button
            onClick={() =>
              void apply({
                kind: 'crop',
                rect: { ...crop, x: crop.x + (doc?.originX ?? 0), y: crop.y + (doc?.originY ?? 0) },
              })
            }
          >
            Apply crop
          </button>
          <button disabled={!doc?.cropped} onClick={() => void apply({ kind: 'crop', rect: null })}>
            Reveal all pixels
          </button>
        </fieldset>
      </details>
      <details>
        <summary>Guides</summary>
        <label>
          <input
            type="checkbox"
            checked={showGuides}
            onChange={(e) => useGeometry.setState({ showGuides: e.target.checked })}
          />
          Show guides
        </label>
        <label>
          <input
            type="checkbox"
            checked={showRulers}
            onChange={(e) => useGeometry.setState({ showRulers: e.target.checked })}
          />
          Show rulers
        </label>
        <fieldset disabled={disabled || busy}>
          <label>
            Direction
            <select
              aria-label="Guide direction"
              value={axis}
              onChange={(e) => setAxis(e.target.value as Guide['axis'])}
            >
              <option value="vertical">Vertical</option>
              <option value="horizontal">Horizontal</option>
            </select>
          </label>
          <label>
            Position (px)
            <input
              aria-label="Guide position"
              type="number"
              value={position}
              onChange={(e) => setPosition(Number(e.target.value))}
            />
          </label>
          <button
            disabled={(doc?.guides?.length ?? 0) >= 256}
            onClick={() =>
              void apply({
                kind: 'guides',
                guides: [
                  ...(doc?.guides ?? []),
                  {
                    axis,
                    position:
                      position + (axis === 'vertical' ? (doc?.originX ?? 0) : (doc?.originY ?? 0)),
                  },
                ],
              })
            }
          >
            Add guide
          </button>
          {(doc?.guides ?? []).map((guide, index) => (
            <div key={index} className="guide-row">
              <label>
                {guide.axis}
                <input
                  key={guide.position}
                  aria-label={'Guide ' + (index + 1) + ' position'}
                  type="number"
                  step="any"
                  defaultValue={
                    guide.position -
                    (guide.axis === 'vertical' ? (doc?.originX ?? 0) : (doc?.originY ?? 0))
                  }
                  onBlur={(e) => {
                    const position =
                      Number(e.target.value) +
                      (guide.axis === 'vertical' ? (doc?.originX ?? 0) : (doc?.originY ?? 0));
                    if (position !== guide.position)
                      void apply({
                        kind: 'guides',
                        guides: doc!.guides!.map((g, i) => (i === index ? { ...g, position } : g)),
                      });
                  }}
                />
              </label>
              <button
                aria-label={'Remove guide ' + (index + 1)}
                onClick={() =>
                  void apply({ kind: 'guides', guides: doc!.guides!.filter((_, i) => i !== index) })
                }
              >
                Remove
              </button>
            </div>
          ))}
        </fieldset>
      </details>
      {notice && <p role="alert">{notice}</p>}
    </section>
  );
}
