// Yield bounded project/file work to queued worker RPCs without nested-timer
// clamping. One channel belongs to this worker's lifetime; no polling when idle.
const channel = new MessageChannel();
const waiting: (() => void)[] = [];
channel.port1.onmessage = () => waiting.shift()?.();
export const yieldProjectTask = () =>
  new Promise<void>((resolve) => {
    waiting.push(resolve);
    channel.port2.postMessage(null);
  });
