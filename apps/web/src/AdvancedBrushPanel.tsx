import { useState } from 'react';
import { setBrush, useBrush } from './brush-state';
import type { BrushSettings, PaintingSymmetry } from './brush-state';
import { useWorkspace } from './state';
import type { EngineApi } from './engine/types';
import {
  brushPresetLimit,
  encodeBrushPresets,
  importBrushPresets,
  saveBrushPreset,
  deleteBrushPreset,
  useBrushPresets,
} from './brush-presets';
export function AdvancedBrushPanel({ api }: { api?: EngineApi }) {
  const { settings, symmetry, tool } = useBrush();
  const { presets, notice } = useBrushPresets();
  const doc = useWorkspace((s) => s.snapshot?.document);
  const painting = useWorkspace((s) => s.snapshot?.painting);
  const [name, setName] = useState('');
  const [loading, setLoading] = useState(false);
  const center: [number, number] = symmetry.center ?? [
    (doc?.originX ?? 0) + (doc?.width ?? 0) / 2,
    (doc?.originY ?? 0) + (doc?.height ?? 0) / 2,
  ];
  const updateSymmetry = (change: Partial<PaintingSymmetry>) =>
    useBrush.setState({ symmetry: { ...symmetry, ...change } });
  async function image(file: File | undefined, key: 'tipImage' | 'texture') {
    if (!file || !api) return;
    setLoading(true);
    useBrushPresets.setState({ notice: 'Loading brush image...' });
    try {
      const asset = await api.brushImage(file);
      setBrush({ [key]: asset, pencil: false });
      useBrushPresets.setState({
        notice: `${key === 'tipImage' ? 'Image tip' : 'Texture'} loaded (${asset.width} x ${asset.height}).`,
      });
    } catch (e) {
      useBrushPresets.setState({ notice: String(e) });
    } finally {
      setLoading(false);
    }
  }
  const sliders: [keyof BrushSettings, string, number, number, number][] = [
    ['scatter', 'Scatter', 0, 2, 0.01],
    ['sizeJitter', 'Size jitter', 0, 1, 0.01],
    ['angleJitter', 'Angle jitter', 0, 1, 0.01],
    ['opacityJitter', 'Opacity jitter', 0, 1, 0.01],
    ['textureDepth', 'Texture depth', 0, 1, 0.01],
    ['textureScale', 'Texture scale', 1, 512, 1],
  ];
  return (
    <details className="advanced-brush">
      <summary>Image tips, dynamics & presets</summary>
      <p>
        Black paints; white and transparency leave gaps. Use 8-bit PNG, JPEG or WebP up to 512 x 512
        pixels / 4 MiB.
      </p>
      {(['tipImage', 'texture'] as const).map((key) => (
        <div key={key}>
          <label>
            {key === 'tipImage' ? 'Image tip' : 'Brush texture'}
            <input
              aria-label={key === 'tipImage' ? 'Image tip file' : 'Brush texture file'}
              type="file"
              accept="image/png,image/jpeg,image/webp"
              disabled={!api || painting || loading}
              onChange={(e) => {
                void image(e.target.files?.[0], key);
                e.target.value = '';
              }}
            />
          </label>
          <span>
            {settings[key]
              ? `${settings[key].width} x ${settings[key].height}`
              : key === 'tipImage'
                ? 'Procedural round'
                : 'No texture'}
          </span>
          <button disabled={!settings[key]} onClick={() => setBrush({ [key]: null })}>
            {key === 'tipImage' ? 'Use round tip' : 'Remove texture'}
          </button>
        </div>
      ))}
      {sliders.map(([key, label, min, max, step]) => (
        <label className="brush-slider" key={key}>
          {label}
          <output>{settings[key] as number}</output>
          <input
            type="range"
            aria-label={label}
            min={min}
            max={max}
            step={step}
            value={settings[key] as number}
            onChange={(e) => setBrush({ [key]: e.target.valueAsNumber })}
          />
        </label>
      ))}
      <label>
        Random seed
        <input
          type="number"
          aria-label="Brush random seed"
          min="0"
          max="4294967295"
          step="1"
          value={settings.seed}
          onChange={(e) => {
            const n = e.target.valueAsNumber;
            if (Number.isInteger(n) && n >= 0 && n <= 0xffffffff) setBrush({ seed: n });
          }}
        />
      </label>
      <p>
        The same seed and input path repeat the same dabs. Texture repeats with each dab; scale is
        in document pixels.
      </p>
      <fieldset>
        <legend>Painting symmetry</legend>
        <p>Brush and Eraser, including masks. Retouch tools use a single path.</p>
        <label>
          Symmetry
          <select
            aria-label="Painting symmetry"
            disabled={!['brush', 'eraser'].includes(tool)}
            value={symmetry.mode}
            onChange={(e) => updateSymmetry({ mode: e.target.value as PaintingSymmetry['mode'] })}
          >
            <option value="off">Off</option>
            <option value="vertical">Vertical mirror</option>
            <option value="horizontal">Horizontal mirror</option>
            <option value="both">Both mirrors</option>
            <option value="radial">Radial</option>
          </select>
        </label>
        {symmetry.mode === 'radial' && (
          <label>
            Sectors
            <input
              aria-label="Radial sectors"
              type="number"
              min="2"
              max="16"
              value={symmetry.sectors}
              onChange={(e) => {
                const n = e.target.valueAsNumber;
                if (Number.isInteger(n) && n >= 2 && n <= 16) updateSymmetry({ sectors: n });
              }}
            />
          </label>
        )}
        {(['X', 'Y'] as const).map((axis, i) => (
          <label key={axis}>
            Center {axis}
            <input
              type="number"
              aria-label={`Symmetry center ${axis}`}
              min="0"
              max="16384"
              step="0.5"
              value={center[i]}
              onChange={(e) => {
                const n = e.target.valueAsNumber;
                if (Number.isFinite(n) && n >= 0 && n <= 16384) {
                  const p = [...center] as [number, number];
                  p[i] = n;
                  updateSymmetry({ center: p });
                }
              }}
            />
          </label>
        ))}
        <button onClick={() => updateSymmetry({ center: null })}>Center on canvas</button>
      </fieldset>
      <label>
        Preset name
        <input
          aria-label="Brush preset name"
          maxLength={64}
          value={name}
          onChange={(e) => setName(e.target.value)}
        />
      </label>
      <div className="button-row">
        <button onClick={() => saveBrushPreset(name)}>Save brush</button>
        <button
          disabled={!presets.some((p) => p.name === name)}
          onClick={() => deleteBrushPreset(name)}
        >
          Delete brush
        </button>
      </div>
      <label>
        Import CashGear brushes
        <input
          type="file"
          aria-label="Import CashGear brushes"
          accept=".cgbrush,application/json"
          disabled={loading}
          onChange={(e) => {
            const file = e.target.files?.[0];
            e.target.value = '';
            if (!file) return;
            if (file.size > brushPresetLimit) {
              useBrushPresets.setState({ notice: 'Brush presets exceed their 4 MiB limit.' });
              return;
            }
            setLoading(true);
            void file
              .text()
              .then(importBrushPresets)
              .catch((e: unknown) => useBrushPresets.setState({ notice: String(e) }))
              .finally(() => setLoading(false));
          }}
        />
      </label>
      <button
        disabled={presets.length === 0}
        onClick={() => {
          try {
            const url = URL.createObjectURL(
              new Blob([encodeBrushPresets(presets)], { type: 'application/json' }),
            );
            const a = document.createElement('a');
            a.href = url;
            a.download = 'CashGear-brushes.cgbrush';
            a.click();
            setTimeout(() => URL.revokeObjectURL(url), 1000);
          } catch (e) {
            useBrushPresets.setState({ notice: String(e) });
          }
        }}
      >
        Export CashGear brushes
      </button>
      {notice && (
        <p role="status" aria-label="Brush library status">
          {notice}
        </p>
      )}
    </details>
  );
}
