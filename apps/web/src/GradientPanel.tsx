import { useState } from 'react';
import {
  builtinGradients,
  deleteGradient,
  saveGradient,
  setGradient,
  useGradient,
} from './gradient-state';
import { hex, parseHex, useBrush } from './brush-state';
import type { GradientSettings } from './engine/types';
export function GradientPanel() {
  const { settings: g, presets, notice } = useGradient();
  const [name, setName] = useState('My gradient');
  const [choice, setChoice] = useState('built:0');
  const change = (i: number, patch: Partial<GradientSettings['stops'][number]>) =>
    setGradient({ stops: g.stops.map((s, n) => (n === i ? { ...s, ...patch } : s)) });
  return (
    <fieldset className="gradient-panel">
      <legend>Gradient editor</legend>
      <p>Drag on the canvas to apply. Escape cancels. Colors include alpha.</p>
      <label>
        Gradient shape{' '}
        <select
          aria-label="Gradient shape"
          value={g.shape}
          onChange={(e) => setGradient({ shape: e.target.value as GradientSettings['shape'] })}
        >
          {['linear', 'radial', 'angle', 'reflected', 'diamond'].map((s) => (
            <option key={s}>{s}</option>
          ))}
        </select>
      </label>
      <label>
        Gradient preset{' '}
        <select
          aria-label="Gradient preset"
          value={choice}
          onChange={(e) => {
            setChoice(e.target.value);
            const [kind, index] = e.target.value.split(':');
            const p = (kind === 'built' ? builtinGradients : presets)[Number(index)];
            if (p) {
              useGradient.setState({ settings: structuredClone(p.settings) });
              setName(p.name);
            }
          }}
        >
          {builtinGradients.map((p, i) => (
            <option key={'built' + i} value={'built:' + i}>
              {p.name}
            </option>
          ))}
          {presets.map((p, i) => (
            <option key={'saved' + i} value={'saved:' + i}>
              {p.name}
            </option>
          ))}
        </select>
      </label>
      <div
        className="gradient-strip"
        aria-label="Gradient color preview"
        style={{
          background: `linear-gradient(to right,${g.stops.map((s) => `rgba(${s.color.slice(0, 3).join(',')},${s.color[3] / 255}) ${s.position * 100}%`).join(',')})`,
        }}
      />
      {g.stops.map((s, i) => (
        <div className="gradient-stop" key={i}>
          <label>
            Position {i + 1}
            <input
              aria-label={`Stop ${i + 1} position`}
              type="number"
              min={i ? g.stops[i - 1].position + 0.001 : 0}
              max={i < g.stops.length - 1 ? g.stops[i + 1].position - 0.001 : 1}
              step="0.01"
              value={s.position}
              disabled={i === 0 || i === g.stops.length - 1}
              onChange={(e) => {
                const v = e.target.valueAsNumber;
                if (
                  Number.isFinite(v) &&
                  v > g.stops[i - 1].position &&
                  v < g.stops[i + 1].position
                )
                  change(i, { position: v });
              }}
            />
          </label>
          <label>
            RGBA {i + 1}
            <input
              aria-label={`Stop ${i + 1} color`}
              key={hex(s.color)}
              defaultValue={hex(s.color)}
              maxLength={9}
              onBlur={(e) => {
                const c = parseHex(e.target.value);
                if (c) change(i, { color: c });
                else e.target.value = hex(s.color);
              }}
            />
          </label>
          <button
            aria-label={`Remove stop ${i + 1}`}
            disabled={i === 0 || i === g.stops.length - 1}
            onClick={() => setGradient({ stops: g.stops.filter((_, n) => n !== i) })}
          >
            −
          </button>
        </div>
      ))}
      <button
        disabled={g.stops.length >= 32}
        onClick={() => {
          let i = 0;
          for (let n = 1; n < g.stops.length - 1; n++)
            if (
              g.stops[n + 1].position - g.stops[n].position >
              g.stops[i + 1].position - g.stops[i].position
            )
              i = n;
          const a = g.stops[i],
            b = g.stops[i + 1];
          setGradient({
            stops: [
              ...g.stops.slice(0, i + 1),
              { position: (a.position + b.position) / 2, color: [...a.color] },
              ...g.stops.slice(i + 1),
            ],
          });
        }}
      >
        Add gradient stop
      </button>
      <button
        onClick={() => {
          const b = useBrush.getState();
          setGradient({
            stops: [
              { position: 0, color: [...b.primary] },
              { position: 1, color: [...b.secondary] },
            ],
          });
        }}
      >
        Use foreground/background
      </button>
      <label>
        Gradient opacity
        <input
          aria-label="Gradient opacity"
          type="range"
          min="0"
          max="1"
          step="0.01"
          value={g.opacity}
          onChange={(e) => setGradient({ opacity: e.target.valueAsNumber })}
        />
      </label>
      <label>
        <input
          type="checkbox"
          checked={g.reverse}
          onChange={(e) => setGradient({ reverse: e.target.checked })}
        />
        Reverse gradient
      </label>
      <label>
        Preset name
        <input
          aria-label="Gradient preset name"
          maxLength={64}
          value={name}
          onChange={(e) => setName(e.target.value)}
        />
      </label>
      <button onClick={() => saveGradient(name)}>Save gradient preset</button>
      <button
        disabled={!presets.some((p) => p.name === name)}
        onClick={() => {
          deleteGradient(name);
          setChoice('built:0');
        }}
      >
        Delete gradient preset
      </button>
      {notice && <p role="status">{notice}</p>}
    </fieldset>
  );
}
