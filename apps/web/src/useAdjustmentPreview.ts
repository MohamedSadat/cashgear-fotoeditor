import { useEffect, useRef, useState } from 'react';
import type {
  Adjustment,
  AdjustmentPreviewTarget,
  AdjustmentPreviewTiming,
  EngineApi,
} from './engine/types';

export function useAdjustmentPreview(
  api: EngineApi | undefined,
  enabled: boolean,
  target: AdjustmentPreviewTarget | undefined,
  adjustment: Adjustment,
  source: string,
) {
  const [pending, setPending] = useState(false);
  const [timing, setTiming] = useState<AdjustmentPreviewTiming>();
  const [error, setError] = useState('');
  const [completed, setCompleted] = useState('');
  const epoch = useRef(0);
  const running = useRef<symbol | undefined>(undefined);
  const queue = useRef<
    | { api: EngineApi; target: AdjustmentPreviewTarget; adjustment: Adjustment; epoch: number }
    | undefined
  >(undefined);
  const targetKey = JSON.stringify(target);
  useEffect(() => {
    ++epoch.current;
    running.current = undefined;
    queue.current = undefined;
    if (!enabled) {
      setPending(false);
      setTiming(undefined);
      setError('');
    }
    return () => {
      ++epoch.current;
      running.current = undefined;
      queue.current = undefined;
      if (enabled) void api?.endAdjustmentPreview().catch(() => {});
    };
  }, [api, enabled, targetKey, source]);
  useEffect(() => {
    if (!api || !enabled || !target) return;
    queue.current = { api, target, adjustment, epoch: epoch.current };
    if (running.current) return;
    const owner = Symbol('preview request queue');
    running.current = owner;
    void (async () => {
      while (queue.current && running.current === owner) {
        const request = queue.current;
        queue.current = undefined;
        if (request.epoch !== epoch.current) continue;
        setPending(true);
        try {
          const result = await request.api.previewAdjustment(request.adjustment, request.target);
          if (running.current !== owner) return;
          if (request.epoch === epoch.current) {
            setTiming(result);
            setCompleted(JSON.stringify(request.adjustment));
            setError('');
          }
        } catch (e) {
          if (running.current !== owner) return;
          if (request.epoch !== epoch.current) continue;
          if (/Finish or cancel the current operation first/.test(String(e))) {
            queue.current ??= request;
            await new Promise((resolve) => setTimeout(resolve, 50));
          } else if (!/Preview cancelled|Operation cancelled|source changed/.test(String(e)))
            setError(String(e));
        }
      }
      if (running.current === owner) {
        running.current = undefined;
        setPending(false);
      }
    })();
  }, [api, enabled, targetKey, source, adjustment]);
  return { pending, timing, error, completed };
}
