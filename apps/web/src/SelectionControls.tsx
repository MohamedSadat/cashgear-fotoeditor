import type { EngineApi, SelectionAction } from './engine/types';
import { useSelection, performSelection, copySelection, pasteSelection } from './selection-state';
import { useWorkspace, updateWorkspace } from './state';
import { useBrush } from './brush-state';
export function SelectionControls({ api, disabled }: { api?: EngineApi; disabled: boolean }) {
  const settings = useSelection();
  const snapshot = useWorkspace((s) => s.snapshot);
  const blocked = disabled || settings.busy || !api;
  const act = (action: SelectionAction) => {
    if (api) void performSelection(api, action);
  };
  const primary = useBrush((s) => s.primary);
  const secondary = useBrush((s) => s.secondary);
  return (
    <div className="selection-controls" aria-label="Selection and edit controls">
      <details className="editor-menu">
        <summary>Select</summary>
        <div>
          <button disabled={blocked} onClick={() => act({ kind: 'all' })}>
            All · Ctrl+A
          </button>
          <button
            disabled={blocked || !snapshot?.selection.active}
            onClick={() => act({ kind: 'none' })}
          >
            Deselect · Ctrl+D
          </button>
          <button
            disabled={blocked || !snapshot?.selection.active}
            onClick={() => act({ kind: 'modify', operation: 'invert', radius: 0 })}
          >
            Inverse · Ctrl+Shift+I
          </button>
          <label>
            Radius{' '}
            <input
              aria-label="Selection radius"
              type="number"
              min={0}
              max={256}
              value={settings.radius}
              onChange={(e) =>
                useSelection.setState({
                  radius: Math.round(Math.max(0, Math.min(256, Number(e.target.value)))),
                })
              }
            />
          </label>
          {(['feather', 'grow', 'shrink', 'border', 'smooth'] as const).map((operation) => (
            <button
              key={operation}
              disabled={blocked || !snapshot?.selection.active}
              onClick={() => act({ kind: 'modify', operation, radius: settings.radius })}
            >
              {operation[0].toUpperCase() + operation.slice(1)}
            </button>
          ))}
          <button
            disabled={blocked}
            onClick={() =>
              act({
                kind: 'wand',
                allLayers: settings.allLayers,
                options: {
                  seed: [0, 0],
                  tolerance: settings.tolerance,
                  contiguous: false,
                  antialias: settings.antialias,
                  mode: settings.mode,
                  color: primary,
                },
              })
            }
          >
            Color range · foreground
          </button>
        </div>
      </details>
      <details className="editor-menu">
        <summary>Edit</summary>
        <div>
          <button disabled={blocked} onClick={() => act({ kind: 'fill', color: primary })}>
            Fill foreground
          </button>
          <button disabled={blocked} onClick={() => act({ kind: 'fill', color: secondary })}>
            Fill background
          </button>
          <button
            disabled={
              blocked ||
              !snapshot?.selection.active ||
              !!snapshot?.selection.quickMask ||
              !!snapshot?.layers.maskTarget
            }
            title="Fill the selection from surrounding texture. Uses the All layers sampling option."
            onClick={() => act({ kind: 'content-aware', allLayers: settings.allLayers })}
          >
            Content-aware fill
          </button>
          <button
            disabled={blocked}
            onClick={() => act({ kind: 'fill', color: primary, clear: true })}
          >
            Clear · Delete
          </button>
          <hr />
          <button disabled={blocked} onClick={() => api && void copySelection(api, true)}>
            Cut · Ctrl+X
          </button>
          <button disabled={blocked} onClick={() => api && void copySelection(api, false)}>
            Copy · Ctrl+C
          </button>
          <button disabled={blocked} onClick={() => api && void copySelection(api, false, true)}>
            Copy merged · Ctrl+Shift+C
          </button>
          <button disabled={blocked} onClick={() => api && void pasteSelection(api, false)}>
            Paste · Ctrl+V
          </button>
          <button disabled={blocked} onClick={() => api && void pasteSelection(api, true)}>
            Paste in place · Ctrl+Shift+V
          </button>
          <button disabled={blocked} onClick={() => api && void pasteSelection(api, false, true)}>
            Paste system image
          </button>
        </div>
      </details>
      <label>
        Mode{' '}
        <select
          aria-label="Selection mode"
          disabled={blocked}
          value={settings.mode}
          onChange={(e) => useSelection.setState({ mode: e.target.value as typeof settings.mode })}
        >
          <option value="new">New</option>
          <option value="add">Add</option>
          <option value="subtract">Subtract</option>
          <option value="intersect">Intersect</option>
        </select>
      </label>
      <label>
        <input
          type="checkbox"
          checked={settings.antialias}
          onChange={(e) => useSelection.setState({ antialias: e.target.checked })}
        />
        Anti-alias
      </label>
      <label>
        Tolerance{' '}
        <input
          aria-label="Selection tolerance"
          type="number"
          min={0}
          max={255}
          value={settings.tolerance}
          onChange={(e) =>
            useSelection.setState({
              tolerance: Math.round(Math.max(0, Math.min(255, Number(e.target.value)))),
            })
          }
        />
      </label>
      <label>
        <input
          type="checkbox"
          checked={settings.contiguous}
          onChange={(e) => useSelection.setState({ contiguous: e.target.checked })}
        />
        Contiguous
      </label>
      <label>
        <input
          type="checkbox"
          checked={settings.allLayers}
          onChange={(e) => useSelection.setState({ allLayers: e.target.checked })}
        />
        All layers
      </label>
      <button
        disabled={blocked}
        aria-pressed={!!snapshot?.selection.quickMask}
        onClick={() => {
          if (api)
            void api
              .quickMask(!snapshot?.selection.quickMask)
              .then((snapshot) => updateWorkspace({ snapshot }))
              .catch((error) => useSelection.setState({ notice: String(error) }));
        }}
      >
        Quick Mask · Q
      </button>
      <span className="selection-state">
        {snapshot?.selection.active ? 'Selection active' : 'No selection'}
      </span>
    </div>
  );
}
