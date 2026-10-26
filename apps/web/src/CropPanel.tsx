import type { EngineApi } from './engine/types';
import { useWorkspace } from './state';
import { useCrop, startCrop, applyCrop, cancelCrop } from './crop-state';
export function CropPanel({ api, disabled }: { api?: EngineApi; disabled: boolean }) {
  const p = useCrop(),
    d = useWorkspace((s) => s.snapshot?.document);
  return (
    <section
      className="geometry-panel crop-panel"
      aria-label="Crop tool"
      onKeyDown={(e) => {
        if (!p.rect || p.busy) return;
        if (e.key === 'Enter' && api) {
          e.preventDefault();
          void applyCrop(api);
        }
        if (e.key === 'Escape') {
          e.preventDefault();
          cancelCrop();
        }
      }}
    >
      <span className="section-eyebrow">CROP</span>
      {!p.rect ? (
        <button disabled={disabled || !d} onClick={startCrop}>
          Start crop
        </button>
      ) : (
        <>
          <p>
            Drag a rectangle; drag handles to resize or Alt-drag to move. Enter applies; Escape
            cancels. Outside pixels remain recoverable.
          </p>
          <fieldset disabled={p.busy || disabled}>
            <label>
              Ratio
              <select
                aria-label="Crop ratio"
                value={p.ratio}
                onChange={(e) => useCrop.setState({ ratio: e.target.value })}
              >
                <option value="free">Free</option>
                <option value="original">Original</option>
                <option value="1">1:1</option>
                <option value="1.3333333333333333">4:3</option>
                <option value="1.5">3:2</option>
                <option value="1.7777777777777777">16:9</option>
                <option value="0.5625">9:16</option>
              </select>
            </label>
            {(['x', 'y', 'width', 'height'] as const).map((k) => (
              <label key={k}>
                {k}
                <input
                  aria-label={'Crop draft ' + k}
                  type="number"
                  min={k === 'width' || k === 'height' ? 1 : 0}
                  step="1"
                  value={
                    p.rect![k] - (k === 'x' ? (d?.originX ?? 0) : k === 'y' ? (d?.originY ?? 0) : 0)
                  }
                  onChange={(e) =>
                    useCrop.setState({
                      rect: {
                        ...p.rect!,
                        [k]:
                          Math.round(Number(e.target.value)) +
                          (k === 'x' ? (d?.originX ?? 0) : k === 'y' ? (d?.originY ?? 0) : 0),
                      },
                    })
                  }
                />
              </label>
            ))}
            <label>
              Straighten (degrees)
              <input
                aria-label="Crop straighten degrees"
                type="number"
                min="-45"
                max="45"
                step="0.1"
                value={p.degrees}
                onChange={(e) => useCrop.setState({ degrees: Number(e.target.value) })}
              />
            </label>
            <button
              aria-pressed={p.straighten}
              onClick={() => useCrop.setState({ straighten: !p.straighten })}
            >
              Draw straighten line
            </button>
            <label>
              Interpolation
              <select
                aria-label="Crop interpolation"
                value={p.interpolation}
                onChange={(e) =>
                  useCrop.setState({ interpolation: e.target.value as typeof p.interpolation })
                }
              >
                {['nearest', 'bilinear', 'bicubic', 'lanczos3'].map((v) => (
                  <option key={v} value={v}>
                    {v === 'lanczos3' ? 'Lanczos3' : v}
                  </option>
                ))}
              </select>
            </label>
            <button onClick={() => api && void applyCrop(api)}>Apply crop</button>
          </fieldset>
          <button disabled={p.busy} onClick={cancelCrop}>
            Cancel crop
          </button>
          <span role="status" data-testid="crop-status">
            {p.notice ||
              (p.busy
                ? 'Applying crop…'
                : `${p.rect.width} × ${p.rect.height} · ${p.degrees.toFixed(2)}°`)}
          </span>
        </>
      )}
    </section>
  );
}
