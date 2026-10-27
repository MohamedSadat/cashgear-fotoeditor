import { useRef, useState } from 'react';
import type { EngineApi, ImageExportOptions } from './engine/types';
import { useWorkspace } from './state';

const defaults: ImageExportOptions = {
  format: 'png',
  source: 'full',
  quality: 85,
  lossless: false,
  embedIcc: true,
  matte: [255, 255, 255],
};
export function ExportDialog({
  api,
  onClose,
  onBusy,
}: {
  api: EngineApi;
  onClose: () => void;
  onBusy: (busy: boolean) => void;
}) {
  const dialog = useRef<HTMLDialogElement | null>(null);
  const revision = useWorkspace((s) => s.snapshot?.revision);
  const active = useWorkspace((s) => s.snapshot?.layers.active);
  const document = useWorkspace((s) => s.snapshot?.document);
  const [options, setOptions] = useState(defaults);
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState('');
  const [prepared, setPrepared] = useState<{ key: string; file: File }>();
  const request = useRef(0);
  const key = JSON.stringify([options, revision, active]);
  const current = prepared?.key === key ? prepared.file : undefined;
  const encode = async (download: boolean) => {
    if (busy) return;
    const token = ++request.current;
    setBusy(true);
    onBusy(true);
    setNotice('');
    try {
      const file = current ?? (await api.exportImage(options));
      if (token !== request.current) return;
      setPrepared({ key, file });
      if (download) {
        const url = URL.createObjectURL(file);
        const link = window.document.createElement('a');
        link.href = url;
        link.download = file.name;
        link.click();
        setTimeout(() => URL.revokeObjectURL(url), 30_000);
        setNotice('Image exported.');
      }
    } catch (error) {
      if (token === request.current) setNotice(String(error));
    } finally {
      if (token === request.current) {
        setBusy(false);
        onBusy(false);
      }
    }
  };
  const close = async () => {
    if (busy) {
      await api.cancelJob();
      ++request.current;
      onBusy(false);
    }
    onClose();
  };
  return (
    <dialog
      className="export-dialog"
      aria-label="Export image"
      ref={(node) => {
        dialog.current = node;
        if (node && !node.open) node.showModal();
      }}
      onCancel={(event) => {
        event.preventDefault();
        void close();
      }}
    >
      <h2>Export image</h2>
      <p>
        {document?.width.toLocaleString()} × {document?.height.toLocaleString()} px · visible canvas
      </p>
      <fieldset disabled={busy}>
        <label>
          Format
          <select
            aria-label="Export format"
            value={options.format}
            onChange={(event) =>
              setOptions({ ...options, format: event.target.value as ImageExportOptions['format'] })
            }
          >
            <option value="png">PNG</option>
            <option value="jpeg">JPEG</option>
            <option value="webp">WebP</option>
          </select>
        </label>
        <label>
          Source
          <select
            aria-label="Export source"
            value={options.source}
            onChange={(event) =>
              setOptions({ ...options, source: event.target.value as ImageExportOptions['source'] })
            }
          >
            <option value="full">Full document</option>
            <option value="layer">Active raster layer</option>
            <option value="selection">Selection on canvas</option>
          </select>
        </label>
        {options.source === 'layer' && (
          <p className="dialog-note">
            Includes this raster layer’s enabled mask, opacity and fill. Visibility and blend
            settings do not affect this isolated export.
          </p>
        )}
        {options.source === 'selection' && (
          <p className="dialog-note">
            Canvas-sized image with transparent pixels outside the selection.
          </p>
        )}
        {options.format === 'webp' && (
          <label>
            <input
              type="checkbox"
              checked={options.lossless}
              onChange={(event) => setOptions({ ...options, lossless: event.target.checked })}
            />
            Lossless WebP
          </label>
        )}
        {(options.format === 'jpeg' || (options.format === 'webp' && !options.lossless)) && (
          <label>
            Quality
            <input
              aria-label="Export quality"
              type="number"
              min="1"
              max="100"
              value={options.quality}
              onChange={(event) => setOptions({ ...options, quality: event.target.valueAsNumber })}
            />
          </label>
        )}
        {options.format === 'jpeg' && (
          <label>
            Transparency matte
            <input
              aria-label="JPEG matte"
              type="color"
              value={'#' + options.matte.map((n) => n.toString(16).padStart(2, '0')).join('')}
              onChange={(event) =>
                setOptions({
                  ...options,
                  matte: [1, 3, 5].map((at) =>
                    parseInt(event.target.value.slice(at, at + 2), 16),
                  ) as [number, number, number],
                })
              }
            />
          </label>
        )}
        <label>
          <input
            type="checkbox"
            checked={options.embedIcc}
            onChange={(event) => setOptions({ ...options, embedIcc: event.target.checked })}
          />
          Embed sRGB ICC profile
        </label>
      </fieldset>
      <p role="status" data-testid="export-size">
        {busy
          ? 'Encoding image…'
          : current
            ? `Encoded size: ${current.size.toLocaleString()} bytes`
            : 'Preview the encoded size before exporting.'}
      </p>
      {notice && <p role="alert">{notice}</p>}
      <div className="dialog-actions">
        <button onClick={() => void close()}>{busy ? 'Cancel export' : 'Close'}</button>
        <button disabled={busy} onClick={() => void encode(false)}>
          Preview encoded size
        </button>
        <button className="primary" disabled={busy} onClick={() => void encode(true)}>
          Download image
        </button>
      </div>
    </dialog>
  );
}
