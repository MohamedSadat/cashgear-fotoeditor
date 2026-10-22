import { useEffect, useRef, useState } from 'react';
import { useWorkspace } from './state';
import type { EngineApi } from './engine/types';
const KEY = 'cashgear-history-v1';
function storedLimits(): [number, number] {
  try {
    const value = JSON.parse(localStorage.getItem(KEY) ?? 'null') as number[] | null;
    if (
      value?.length === 2 &&
      value.every(Number.isInteger) &&
      value[0] >= 0 &&
      value[0] <= 768 &&
      value[1] >= 0 &&
      value[1] <= 2000
    )
      return [value[0], value[1]];
  } catch {
    /* Storage preferences are optional. */
  }
  return [256, 200];
}
export function HistoryPanel({ api, disabled }: { api?: EngineApi; disabled: boolean }) {
  const snapshot = useWorkspace((s) => s.snapshot);
  const [limits, setLimits] = useState(storedLimits);
  const [error, setError] = useState('');
  const gesture = useRef(0);
  useEffect(() => {
    if (api)
      void api.configureHistory(limits[0] * 1048576, limits[1]).catch((e) => setError(String(e)));
    // Configure a fresh worker; later changes require Apply so editing a field
    // does not evict history before the user commits their chosen limits.
  }, [api]);
  const action = (work?: Promise<unknown>) => {
    void work?.catch((e) => setError(String(e)));
  };
  const history = snapshot?.history;
  return (
    <section className="history-panel" aria-label="History">
      <span className="section-eyebrow">HISTORY</span>
      <h2>Every step.</h2>
      <div className="history-list" role="list" aria-label="Document history">
        <button
          role="listitem"
          aria-current={history?.undo === 0 ? 'step' : undefined}
          disabled={disabled || !snapshot?.document}
          onClick={() => action(api?.historyJump(0))}
        >
          {history?.evicted ? 'Oldest retained state' : 'Initial state'}
        </button>
        {history?.labels.map((label, index) => (
          <button
            key={index}
            role="listitem"
            className={index >= history.undo ? 'future-step' : ''}
            aria-current={history.undo === index + 1 ? 'step' : undefined}
            disabled={disabled}
            onClick={() => action(api?.historyJump(index + 1))}
          >
            {index + 1}. {label}
          </button>
        ))}
      </div>
      <p className="history-note" data-testid="history-eviction">
        {history?.evicted ?? 0} steps evicted ·{' '}
        {((snapshot?.memory.history ?? 0) / 1048576).toFixed(1)} MiB retained
      </p>
      <details>
        <summary>History limits</summary>
        <form
          onSubmit={(e) => {
            e.preventDefault();
            setError('');
            action(
              api?.configureHistory(limits[0] * 1048576, limits[1]).then(() => {
                try {
                  localStorage.setItem(KEY, JSON.stringify(limits));
                } catch {
                  setError('Limits applied for this session; preferences could not be saved.');
                }
              }),
            );
          }}
        >
          <label>
            History memory (MiB)
            <input
              type="number"
              min="0"
              max="768"
              step="1"
              required
              value={limits[0]}
              onChange={(e) => setLimits([e.target.valueAsNumber, limits[1]])}
            />
          </label>
          <label>
            Maximum history steps
            <input
              type="number"
              min="0"
              max="2000"
              step="1"
              required
              value={limits[1]}
              onChange={(e) => setLimits([limits[0], e.target.valueAsNumber])}
            />
          </label>
          <button disabled={disabled} type="submit">
            Apply history limits
          </button>
        </form>
        <p className="history-note">
          Shares the 768 MiB CPU limit. Oldest steps may be evicted sooner. Zero disables retained
          undo. An edit larger than the limit can complete without an undo step.
        </p>
      </details>
      {snapshot?.document && (
        <label className="document-resolution">
          Document resolution DPI
          <input
            aria-label="Document resolution DPI"
            type="range"
            min="1"
            max="2400"
            step="1"
            value={snapshot.document.dpi}
            disabled={disabled}
            onFocus={() => {
              gesture.current++;
            }}
            onPointerDown={() => {
              gesture.current++;
            }}
            onChange={(e) => action(api?.resolution(e.target.valueAsNumber, gesture.current))}
          />
          <span>{snapshot.document.dpi.toFixed(0)} DPI · pixels unchanged</span>
        </label>
      )}
      {error && <p role="alert">{error}</p>}
    </section>
  );
}
