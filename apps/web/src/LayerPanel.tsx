import { useEffect, useRef, useState } from 'react';
import type { EngineApi, LayerAction, LayerNode, LayerProperties } from './engine/types';
import { updateWorkspace, useWorkspace } from './state';

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
export function LayerPanel({ api, disabled }: { api?: EngineApi; disabled: boolean }) {
  const layers = useWorkspace((s) => s.snapshot?.layers);
  const revision = useWorkspace((s) => s.snapshot?.contentRevision);
  const painting = useWorkspace((s) => s.snapshot?.painting);
  const [thumbnails, setThumbnails] = useState<Record<number, string>>({});
  const thumbnailRevisions = useRef(new Map<number, number | undefined>());
  const urls = useRef(new Map<number, string>());
  const [selected, setSelected] = useState<number[]>([]);
  const [error, setError] = useState('');
  const [pending, setPending] = useState(false);
  const flatten = (nodes: LayerNode[]): LayerNode[] =>
    nodes.flatMap((n) => [n, ...flatten(n.children ?? [])]);
  const nodes = layers?.nodes ?? [];
  const all = flatten(nodes);
  const active =
    all.find((n) => n.id === layers?.active) ?? all.find((n) => selected.includes(n.id)) ?? all[0];
  const ids = selected.filter((id) => all.some((n) => n.id === id));
  const chosen = ids.length ? ids : active ? [active.id] : [];
  const blocked = disabled || pending || !api;
  const activeId = active?.id;
  const nodeIds = all.map((node) => node.id).join(',');
  useEffect(() => {
    if (!api || !activeId || painting || disabled) return;
    let current = true;
    const live = new Set(all.map((node) => node.id));
    for (const [id, url] of urls.current)
      if (!live.has(id)) {
        URL.revokeObjectURL(url);
        urls.current.delete(id);
        thumbnailRevisions.current.delete(id);
      }
    const targets = all.filter(
      (node) =>
        !urls.current.has(node.id) ||
        (thumbnailRevisions.current.get(node.id) !== revision &&
          (node.id === activeId ||
            flatten(node.children ?? []).some((child) => child.id === activeId))),
    );
    void (async () => {
      for (const node of targets) {
        if (!current) return;
        try {
          const bytes = await api.thumbnail(node.id);
          if (!current) return;
          const url = URL.createObjectURL(
            new Blob([bytes as Uint8Array<ArrayBuffer>], { type: 'image/png' }),
          );
          const old = urls.current.get(node.id);
          if (old) URL.revokeObjectURL(old);
          urls.current.set(node.id, url);
          thumbnailRevisions.current.set(node.id, revision);
          setThumbnails((previous) => ({
            ...Object.fromEntries(Object.entries(previous).filter(([id]) => live.has(Number(id)))),
            [node.id]: url,
          }));
        } catch {
          break;
        }
      }
    })();
    return () => {
      current = false;
    };
  }, [api, activeId, nodeIds, revision, painting, disabled]);
  useEffect(() => {
    const owned = urls.current;
    return () => {
      for (const url of owned.values()) URL.revokeObjectURL(url);
    };
  }, []);
  const run = async (action: LayerAction) => {
    if (!api || blocked) return;
    setPending(true);
    setError('');
    try {
      const snapshot = await api.layerAction(action);
      updateWorkspace({ snapshot });
      if (['add', 'duplicate', 'group'].includes(action.kind) && snapshot.layers.active)
        setSelected([snapshot.layers.active]);
    } catch (e) {
      setError(String(e));
    } finally {
      setPending(false);
    }
  };
  const select = async (node: LayerNode, multiple = false, mask = false, view = false) => {
    if (!api || blocked) return;
    setSelected(
      multiple
        ? chosen.includes(node.id)
          ? chosen.filter((id) => id !== node.id)
          : [...chosen, node.id]
        : [node.id],
    );
    setPending(true);
    try {
      updateWorkspace({ snapshot: await api.selectLayer(node.id, mask, view) });
    } catch (e) {
      setError(String(e));
    } finally {
      setPending(false);
    }
  };
  const props = (node: LayerNode, change: Partial<LayerProperties>) =>
    run({ kind: 'properties', target: node.id, properties: { ...node.properties, ...change } });
  const rows = (siblings: LayerNode[], parent: number | null = null, depth = 0) =>
    [...siblings].reverse().map((node) => (
      <div key={node.id}>
        <div
          className={'layer-row' + (chosen.includes(node.id) ? ' selected' : '')}
          style={{ paddingLeft: 4 + depth * 14 }}
          draggable={!blocked}
          onDragStart={(event) =>
            event.dataTransfer.setData(
              'application/x-cashgear-layers',
              JSON.stringify(chosen.includes(node.id) ? chosen : [node.id]),
            )
          }
          onDragOver={(event) => event.preventDefault()}
          onDrop={(event) => {
            event.preventDefault();
            try {
              const moved = JSON.parse(
                event.dataTransfer.getData('application/x-cashgear-layers'),
              ) as number[];
              const remaining = siblings.filter((n) => !moved.includes(n.id));
              const index = Math.max(
                0,
                remaining.findIndex((n) => n.id === node.id),
              );
              void run({ kind: 'move', ids: moved, parent, index });
            } catch {
              setError('Choose layers in this document to reorder.');
            }
          }}
        >
          <button
            aria-label={(node.properties.visible ? 'Hide ' : 'Show ') + node.name}
            disabled={blocked}
            onClick={() => void props(node, { visible: !node.properties.visible })}
          >
            {node.properties.visible ? '●' : '○'}
          </button>
          <button
            className="layer-name"
            aria-pressed={chosen.includes(node.id)}
            disabled={blocked}
            onClick={(event) => void select(node, event.ctrlKey || event.metaKey || event.shiftKey)}
          >
            {thumbnails[node.id] && (
              <img className="layer-thumbnail" src={thumbnails[node.id]} alt="" />
            )}
            {node.children ? '▾ ' : '▧ '}
            {node.name}
          </button>
          {node.mask && (
            <button
              title="Select mask · Alt-click to view mask"
              aria-label={'Mask of ' + node.name}
              aria-pressed={layers?.active === node.id && layers.maskTarget}
              className="layer-mask"
              disabled={blocked}
              onClick={(event) => void select(node, false, true, event.altKey)}
            >
              ◧
            </button>
          )}
        </div>
        {node.children && rows(node.children, node.id, depth + 1)}
      </div>
    ));
  return (
    <section className="layers-panel" aria-label="Layers">
      <div className="panel-heading">
        <h2>Layers</h2>
        <small>sRGB</small>
      </div>
      <div className="layer-actions">
        <button
          disabled={blocked}
          onClick={() =>
            void run({
              kind: 'add',
              name: 'Layer ' + (all.length + 1),
              parent: null,
              index: nodes.length,
              group: false,
            })
          }
        >
          + Layer
        </button>
        <button
          disabled={blocked || !chosen.length}
          onClick={() => void run({ kind: 'group', ids: chosen, name: 'Group' })}
        >
          Group
        </button>
        <button
          disabled={blocked || !chosen.length}
          onClick={() => void run({ kind: 'duplicate', ids: chosen })}
        >
          Duplicate
        </button>
        <button
          disabled={blocked || !chosen.length}
          onClick={() => void run({ kind: 'delete', ids: chosen })}
        >
          Delete
        </button>
      </div>
      <div className="layer-list" role="group" aria-label="Layer stack">
        {rows(nodes)}
      </div>
      {active && (
        <fieldset disabled={blocked}>
          <label>
            Name
            <input
              key={active.id + active.name}
              aria-label="Layer name"
              defaultValue={active.name}
              maxLength={512}
              onBlur={(event) => {
                if (event.target.value !== active.name)
                  void run({ kind: 'rename', target: active.id, name: event.target.value });
              }}
              onKeyDown={(event) => {
                if (event.key === 'Enter') event.currentTarget.blur();
              }}
            />
          </label>
          <label>
            Blend
            <select
              aria-label="Layer blend mode"
              value={active.properties.passThrough ? 'pass-through' : active.properties.blend}
              onChange={(event) =>
                void props(active, {
                  passThrough: event.target.value === 'pass-through',
                  blend: event.target.value === 'pass-through' ? 'normal' : event.target.value,
                })
              }
            >
              {active.children && <option value="pass-through">Pass-through</option>}
              {modes.map((mode) => (
                <option key={mode} value={mode}>
                  {mode.replaceAll('-', ' ')}
                </option>
              ))}
            </select>
          </label>
          {(['opacity', 'fill'] as const).map((key) => (
            <label key={key}>
              {key}
              <input
                aria-label={'Layer ' + key}
                type="number"
                min="0"
                max="100"
                step="1"
                value={Math.round(active.properties[key] * 100)}
                onChange={(event) =>
                  void props(active, { [key]: Number(event.target.value) / 100 })
                }
              />
              %
            </label>
          ))}
          <div className="layer-locks">
            {(['pixels', 'alpha', 'position', 'all'] as const).map((key) => (
              <label key={key}>
                <input
                  aria-label={'Lock ' + key}
                  type="checkbox"
                  checked={active.properties.locks[key]}
                  onChange={(event) =>
                    void props(active, {
                      locks: { ...active.properties.locks, [key]: event.target.checked },
                    })
                  }
                />
                {key}
              </label>
            ))}
          </div>
          <label>
            <input
              type="checkbox"
              checked={active.properties.clipped}
              onChange={(event) => void props(active, { clipped: event.target.checked })}
            />
            Clip to layer below
          </label>
          <div className="layer-actions">
            {(['down', 'visible', 'flatten'] as const).map((kind) => (
              <button
                key={kind}
                onClick={() => {
                  if (api) {
                    setPending(true);
                    void api
                      .mergeLayers(kind, active.id)
                      .then((snapshot) => updateWorkspace({ snapshot }))
                      .catch((e) => setError(String(e)))
                      .finally(() => setPending(false));
                  }
                }}
              >
                {kind === 'down' ? 'Merge down' : kind === 'visible' ? 'Merge visible' : 'Flatten'}
              </button>
            ))}
            {!active.mask ? (
              <button onClick={() => void run({ kind: 'mask-add', target: active.id })}>
                Add mask
              </button>
            ) : (
              <>
                <button
                  onClick={() =>
                    void run({
                      kind: 'mask-enabled',
                      target: active.id,
                      enabled: !active.mask!.enabled,
                    })
                  }
                >
                  {active.mask.enabled ? 'Disable mask' : 'Enable mask'}
                </button>
                <button
                  onClick={() => {
                    if (api) {
                      setPending(true);
                      void api
                        .applyMask(active.id)
                        .then((snapshot) => updateWorkspace({ snapshot }))
                        .catch((e) => setError(String(e)))
                        .finally(() => setPending(false));
                    }
                  }}
                >
                  Apply mask
                </button>
                <button onClick={() => void run({ kind: 'mask-delete', target: active.id })}>
                  Delete mask
                </button>
              </>
            )}
            {active.children && (
              <>
                <button
                  onClick={() =>
                    void run({
                      kind: 'add',
                      name: 'Layer ' + (all.length + 1),
                      parent: active.id,
                      index: active.children!.length,
                      group: false,
                    })
                  }
                >
                  Add inside group
                </button>
                <button
                  onClick={() =>
                    void run({
                      kind: 'move',
                      ids: chosen.filter((id) => id !== active.id),
                      parent: active.id,
                      index: active.children!.filter((n) => !chosen.includes(n.id)).length,
                    })
                  }
                >
                  Move selected inside
                </button>
                <button onClick={() => void run({ kind: 'ungroup', target: active.id })}>
                  Ungroup
                </button>
              </>
            )}
          </div>
        </fieldset>
      )}
      <p className="layer-help">Ctrl/Shift-click to select several layers. Drag to reorder.</p>
      {layers?.maskTarget && (
        <p className="layer-help">
          {layers.maskView ? 'Viewing mask' : 'Painting mask'} · Click the layer name to return to
          pixels.
        </p>
      )}
      {error && <p role="alert">{error}</p>}
    </section>
  );
}
