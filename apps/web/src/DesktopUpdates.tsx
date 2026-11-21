import { useEffect, useState } from 'react';
import { invoke, Resource } from '@tauri-apps/api/core';

interface UpdateCheck {
  current: string;
  available: { id: number; version: string; bytes: number } | null;
}

const release = (id: number) => new Resource(id).close().catch(() => {});

export function DesktopUpdates({
  prepareInstall,
  onClose,
}: {
  prepareInstall: () => Promise<void>;
  onClose: () => void;
}) {
  const [check, setCheck] = useState<UpdateCheck>();
  const [status, setStatus] = useState('Checking for updates…');
  const [installing, setInstalling] = useState(false);
  useEffect(() => {
    let closed = false;
    const request = invoke<UpdateCheck>('check_desktop_update');
    void request
      .then((result) => {
        if (closed) return;
        setCheck(result);
        setStatus(result.available ? '' : 'You have the latest version.');
      })
      .catch((error) => {
        if (!closed) setStatus(String(error));
      });
    return () => {
      closed = true;
      void request
        .then((result) => {
          if (result.available) return release(result.available.id);
        })
        .catch(() => {});
    };
  }, []);
  const install = async () => {
    if (!check?.available || installing) return;
    setInstalling(true);
    let packageId: number | undefined;
    try {
      setStatus('Preparing to restart…');
      await prepareInstall();
      setStatus('Downloading and verifying the installer…');
      packageId = await invoke<number>('download_desktop_update', { id: check.available.id });
      setStatus('Installing the update…');
      await invoke('install_desktop_update', { id: packageId });
    } catch (error) {
      setStatus(String(error));
    } finally {
      if (packageId !== undefined) await release(packageId);
      setInstalling(false);
    }
  };
  return (
    <dialog
      ref={(node) => {
        if (node && !node.open) node.showModal();
      }}
      aria-labelledby="desktop-updates-heading"
      onCancel={(event) => {
        event.preventDefault();
        if (!installing) onClose();
      }}
    >
      <h2 id="desktop-updates-heading">CashGear FotoEditor updates</h2>
      {check && <p>Installed version {check.current}</p>}
      {check?.available && (
        <p>
          Version {check.available.version} is available (
          {(check.available.bytes / 1048576).toFixed(1)} MiB).
        </p>
      )}
      {status && <p role="status">{status}</p>}
      <div className="dialog-actions">
        <button disabled={installing} onClick={onClose}>
          Close
        </button>
        {check?.available && (
          <button className="primary" disabled={installing} onClick={() => void install()}>
            Install and restart
          </button>
        )}
      </div>
    </dialog>
  );
}
