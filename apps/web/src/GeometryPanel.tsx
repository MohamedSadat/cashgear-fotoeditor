import { useState } from 'react';
import type { EngineApi, GeometryMetadata, Guide } from './engine/types';
import { updateWorkspace, useWorkspace } from './state';
import { useGeometry } from './geometry-state';

export function GeometryPanel({ api, disabled }: { api?: EngineApi; disabled: boolean }) {
  const doc = useWorkspace((s) => s.snapshot?.document);
  const [crop, setCrop] = useState({ x: 0, y: 0, width: 100, height: 100 });
  const [axis, setAxis] = useState<Guide['axis']>('vertical');
  const [position, setPosition] = useState(0);
  const [notice, setNotice] = useState('');
  const [busy, setBusy] = useState(false);
  const { showGuides, showRulers } = useGeometry();
  const apply = async (action: GeometryMetadata) => {
    if (!api || disabled || busy) return;
    setBusy(true);
    setNotice('');
    try {
      const snapshot = await api.geometryMetadata(action);
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
