import { memo, useState } from 'react';
import { hex, parseHex, presets, setBrush, useBrush } from './brush-state';
import type { BrushSettings, Curve, Rgba } from './brush-state';

export function rgbToHsv(rgb: Rgba) {
  const [r, g, b] = rgb.map((c) => c / 255);
  const max = Math.max(r, g, b),
    min = Math.min(r, g, b),
    d = max - min;
  let h =
    d === 0 ? 0 : max === r ? ((g - b) / d) % 6 : max === g ? (b - r) / d + 2 : (r - g) / d + 4;
  h = (h * 60 + 360) % 360;
  return { h, s: max === 0 ? 0 : d / max, v: max };
}
export function hsvToRgb(h: number, s: number, v: number, alpha = 255): Rgba {
  const c = v * s,
    x = c * (1 - Math.abs(((h / 60) % 2) - 1)),
    m = v - c;
  const [r, g, b] =
    h < 60
      ? [c, x, 0]
      : h < 120
        ? [x, c, 0]
        : h < 180
          ? [0, c, x]
          : h < 240
            ? [0, x, c]
            : h < 300
              ? [x, 0, c]
              : [c, 0, x];
  return [Math.round((r + m) * 255), Math.round((g + m) * 255), Math.round((b + m) * 255), alpha];
}
export const BrushPanel = memo(function BrushPanel() {
  const {
    settings,
    primary,
    secondary,
    recent,
    setColor,
    swap,
    tool,
    cloneAligned,
    cloneAllLayers,
  } = useBrush();
  const color = hex(primary);
  const [picker, setPicker] = useState({ ...rgbToHsv(primary), color });
  const hsv = picker.color === color ? picker : { ...rgbToHsv(primary), color };
  const [draft, setDraft] = useState<string | undefined>();
  const pick = (h: number, s: number, v: number) => {
    const rgba = hsvToRgb(h, s, v, primary[3]);
    setPicker({ h, s, v, color: hex(rgba) });
    setColor(rgba);
    setDraft(undefined);
  };
  const sliders: [keyof BrushSettings, string, number, number, number][] = [
    ['size', 'Size', 1, 2048, 1],
    ['hardness', 'Hardness', 0, 1, 0.01],
    ['opacity', 'Opacity', 0, 1, 0.01],
    ['flow', 'Flow', 0, 1, 0.01],
    ['spacing', 'Spacing', 0.01, 1, 0.01],
    ['roundness', 'Roundness', 0.05, 1, 0.01],
    ['angle', 'Angle', -180, 180, 1],
  ];
  return (
    <div className="brush-panel">
      <span className="section-eyebrow">BRUSH & COLOR</span>
      {tool === 'clone' && (
        <fieldset className="clone-controls">
          <legend>Clone Stamp</legend>
          <p>Alt-click to pick a source. S selects Clone Stamp.</p>
          <label>
            <input
              type="checkbox"
              checked={cloneAligned}
              onChange={(e) => useBrush.setState({ cloneAligned: e.target.checked })}
            />
            Aligned source
          </label>
          <label>
            Clone sample
            <select
              aria-label="Clone sample"
              value={cloneAllLayers ? 'all' : 'current'}
              onChange={(e) => useBrush.setState({ cloneAllLayers: e.target.value === 'all' })}
            >
              <option value="current">Current layer</option>
              <option value="all">All visible layers</option>
            </select>
          </label>
        </fieldset>
      )}
      <div className="color-swatches">
        <button
          aria-label="Primary color"
          style={{ background: color }}
          onClick={() => document.getElementById('color-hex')?.focus()}
        />
        <button
          aria-label="Secondary color"
          style={{ background: hex(secondary) }}
          onClick={swap}
        />
        <button title="Swap colors · X" aria-label="Swap colors" onClick={swap}>
          ⇆
        </button>
      </div>
      <div
        className="hsv-square"
        role="slider"
        tabIndex={0}
        aria-label="Saturation and brightness"
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={Math.round(hsv.s * 100)}
        aria-valuetext={`${Math.round(hsv.s * 100)}% saturation, ${Math.round(hsv.v * 100)}% brightness`}
        style={{
          background: `linear-gradient(to top,#000,transparent),linear-gradient(to right,#fff,hsl(${hsv.h} 100% 50%))`,
        }}
        onKeyDown={(e) => {
          if (['ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown'].includes(e.key)) {
            e.preventDefault();
            pick(
              hsv.h,
              Math.max(
                0,
                Math.min(
                  1,
                  hsv.s + (e.key === 'ArrowRight' ? 0.01 : e.key === 'ArrowLeft' ? -0.01 : 0),
                ),
              ),
              Math.max(
                0,
                Math.min(
                  1,
                  hsv.v + (e.key === 'ArrowUp' ? 0.01 : e.key === 'ArrowDown' ? -0.01 : 0),
                ),
              ),
            );
          }
        }}
        onPointerDown={(e) => {
          e.currentTarget.setPointerCapture(e.pointerId);
          const r = e.currentTarget.getBoundingClientRect();
          pick(
            hsv.h,
            Math.max(0, Math.min(1, (e.clientX - r.left) / r.width)),
            Math.max(0, Math.min(1, 1 - (e.clientY - r.top) / r.height)),
          );
        }}
        onPointerMove={(e) => {
          if (e.buttons !== 1) return;
          const r = e.currentTarget.getBoundingClientRect();
          pick(
            hsv.h,
            Math.max(0, Math.min(1, (e.clientX - r.left) / r.width)),
            Math.max(0, Math.min(1, 1 - (e.clientY - r.top) / r.height)),
          );
        }}
      >
        <span style={{ left: hsv.s * 100 + '%', top: (1 - hsv.v) * 100 + '%' }} />
      </div>
      <input
        className="hue-slider"
        aria-label="Hue"
        type="range"
        min="0"
        max="359.9"
        step="0.1"
        value={hsv.h}
        onChange={(e) => pick(e.target.valueAsNumber, hsv.s, hsv.v)}
      />
      <label className="hex-label">
        Hex
        <input
          id="color-hex"
          aria-label="Color hex"
          title="RGB hex, or RGBA hex with alpha last"
          value={draft ?? color}
          spellCheck={false}
          maxLength={9}
          aria-invalid={draft !== undefined && !parseHex(draft)}
          onBlur={() => setDraft(undefined)}
          onChange={(e) => {
            setDraft(e.target.value);
            const rgba = parseHex(e.target.value);
            if (rgba) setColor(rgba);
          }}
        />
      </label>
      <div className="recent-colors" aria-label="Recent colors">
        {recent.map((c) => (
          <button
            key={c}
            aria-label={'Use ' + c}
            style={{ background: c }}
            onClick={() => {
              setColor(parseHex(c)!);
              setDraft(undefined);
            }}
          />
        ))}
      </div>
      <label>
        Brush preset
        <select
          aria-label="Brush preset"
          defaultValue="Hard round"
          onChange={(e) => setBrush(presets[e.target.value])}
        >
          {Object.keys(presets).map((p) => (
            <option key={p}>{p}</option>
          ))}
        </select>
      </label>
      {sliders.map(([key, label, min, max, step]) => (
        <label className="brush-slider" key={key}>
          {label}
          <output>
            {key === 'size'
              ? settings.size + ' px'
              : key === 'angle'
                ? settings.angle + '°'
                : Math.round((settings[key] as number) * 100) + '%'}
          </output>
          <input
            aria-label={'Brush ' + label.toLowerCase()}
            type="range"
            min={min}
            max={max}
            step={step}
            value={settings[key] as number}
            onChange={(e) =>
              setBrush({
                [key]: e.target.valueAsNumber,
                ...(key === 'size' ? { pencil: false } : {}),
              })
            }
          />
        </label>
      ))}
      <label>
        Stabilizer
        <select
          aria-label="Stabilizer"
          value={settings.stabilizer}
          onChange={(e) => setBrush({ stabilizer: e.target.value as BrushSettings['stabilizer'] })}
        >
          <option value="off">Off</option>
          <option value="pulled">Pulled string</option>
          <option value="weighted">Weighted average</option>
        </select>
      </label>
      <label className="brush-slider">
        Strength<output>{Math.round(settings.strength * 100)}%</output>
        <input
          aria-label="Stabilizer strength"
          type="range"
          min="0"
          max="1"
          step="0.01"
          value={settings.strength}
          onChange={(e) => setBrush({ strength: e.target.valueAsNumber })}
        />
      </label>
      <details className="dynamics">
        <summary>Pen dynamics & curves</summary>
        <p>Five curve values at 0, 25, 50, 75 and 100% input. Tilt uses pen inclination.</p>
        {(
          [
            ['pressureSize', 'sizePressure', 'Pressure size'],
            ['pressureOpacity', 'opacityPressure', 'Pressure opacity'],
            ['tiltSize', 'sizeTilt', 'Tilt size'],
            ['tiltOpacity', 'opacityTilt', 'Tilt opacity'],
          ] as const
        ).map(([enabled, key, label]) => (
          <div key={key}>
            <label className="checkbox">
              <input
                type="checkbox"
                checked={settings[enabled]}
                onChange={(e) => setBrush({ [enabled]: e.target.checked })}
              />
              {label}
            </label>
            <div className="curve-values">
              {settings[key].map((value, index) => (
                <input
                  key={index}
                  type="number"
                  aria-label={`${label} curve ${index * 25}%`}
                  min="0"
                  max="1"
                  step="0.05"
                  value={value}
                  onChange={(e) => {
                    const n = e.target.valueAsNumber;
                    if (!Number.isFinite(n)) return;
                    const curve = [...settings[key]] as Curve;
                    curve[index] = Math.max(0, Math.min(1, n));
                    setBrush({ [key]: curve });
                  }}
                />
              ))}
            </div>
            <svg viewBox="0 0 100 36" aria-label={label + ' curve'}>
              <path d="M0 0V36H100" />
              <polyline
                points={settings[key].map((v, i) => `${i * 25},${36 - v * 36}`).join(' ')}
              />
            </svg>
          </div>
        ))}
      </details>
    </div>
  );
});
