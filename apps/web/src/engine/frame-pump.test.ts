import { describe, expect, it, vi } from 'vitest';
import { FramePump } from './frame-pump';
import type { DocumentSnapshot, FrameResult } from './types';
const frame: FrameResult = { presented: true, frames: 1, width: 100, height: 100 };
const settle = async () => {
  await new Promise((resolve) => setTimeout(resolve, 0));
};

describe('FramePump ownership and backpressure', () => {
  it('reports queued main-thread edits as busy across an older idle worker reply', async () => {
    let complete!: (value: FrameResult) => void;
    const onFrame = vi.fn();
    const api = {
      resize: vi.fn(),
      frame: vi.fn().mockImplementation(
        () =>
          new Promise<FrameResult>((resolve) => {
            complete = resolve;
          }),
      ),
    };
    const pump = new FramePump(api, onFrame, vi.fn());
    pump.resize({ width: 100, height: 100 });
    pump.tick(0);
    await settle();
    pump.input({ kind: 'stroke-begin', x: 0, y: 0, amount: 0 });
    complete({ ...frame, snapshot: { painting: false } as DocumentSnapshot });
    await settle();
    expect(onFrame.mock.calls[0][0].snapshot.painting).toBe(true);
    pump.stop();
  });
  it('presents an active preview later in an unused frame slot and stops its timer', async () => {
    vi.useFakeTimers();
    const active = { ...frame, snapshot: { painting: true } as DocumentSnapshot };
    const api = {
      resize: vi.fn(),
      frame: vi
        .fn()
        .mockResolvedValueOnce({ ...active, presented: false })
        .mockResolvedValueOnce({ ...active, presented: false })
        .mockResolvedValue(active),
    };
    const pump = new FramePump(api, vi.fn(), vi.fn());
    try {
      pump.resize({ width: 100, height: 100 });
      pump.tick(0);
      await vi.advanceTimersByTimeAsync(0);
      pump.tick(16);
      await vi.advanceTimersByTimeAsync(7);
      expect(api.frame).toHaveBeenCalledTimes(2);
      await vi.advanceTimersByTimeAsync(1);
      expect(api.frame.mock.calls[2][2]).toBe(true);
      pump.input({ kind: 'stroke-sample', x: 1, y: 2, amount: 0 });
      await vi.advanceTimersByTimeAsync(0);
      expect(api.frame).toHaveBeenCalledTimes(3);
      pump.stop();
      await vi.advanceTimersByTimeAsync(32);
      expect(api.frame).toHaveBeenCalledTimes(3);
    } finally {
      pump.stop();
      vi.useRealTimers();
    }
  });
  it('lets new input use the preview slot before its timer', async () => {
    vi.useFakeTimers();
    const active = { ...frame, snapshot: { painting: true } as DocumentSnapshot };
    const api = {
      resize: vi.fn(),
      frame: vi
        .fn()
        .mockResolvedValueOnce({ ...active, presented: false })
        .mockResolvedValueOnce({ ...active, presented: false })
        .mockResolvedValue(active),
    };
    const pump = new FramePump(api, vi.fn(), vi.fn());
    try {
      pump.resize({ width: 100, height: 100 });
      pump.tick(0);
      await vi.advanceTimersByTimeAsync(0);
      pump.tick(16);
      await vi.advanceTimersByTimeAsync(4);
      pump.input({ kind: 'stroke-sample', x: 1, y: 2, amount: 0 });
      await vi.advanceTimersByTimeAsync(4);
      expect(api.frame).toHaveBeenCalledTimes(3);
      expect(api.frame.mock.calls[2]).toHaveLength(2);
    } finally {
      pump.stop();
      vi.useRealTimers();
    }
  });
  it('retains the submission slot while a GPU completion reply crosses a RAF boundary', async () => {
    let complete!: (value: FrameResult) => void;
    const api = {
      resize: vi.fn(),
      frame: vi
        .fn()
        .mockImplementationOnce(
          () =>
            new Promise<FrameResult>((resolve) => {
              complete = resolve;
            }),
        )
        .mockResolvedValue({ ...frame, presentedAt: performance.timeOrigin + 120 }),
    };
    const pump = new FramePump(api, vi.fn(), vi.fn());
    pump.resize({ width: 100, height: 100 });
    pump.tick(100);
    await settle();
    pump.input({ kind: 'stroke-sample', x: 1, y: 2, amount: 0 });
    pump.tick(116);
    expect(api.frame).toHaveBeenCalledTimes(1);
    complete({ ...frame, presentedAt: performance.timeOrigin + 110 });
    await settle();
    await settle();
    expect(api.frame).toHaveBeenCalledTimes(2);
  });
  it('flushes edits after an idle poll and permits only one presentation per RAF epoch', async () => {
    const api = {
      resize: vi.fn(),
      frame: vi
        .fn()
        .mockResolvedValueOnce({ ...frame, presented: false })
        .mockResolvedValueOnce({ ...frame, presented: false })
        .mockResolvedValue(frame),
    };
    const pump = new FramePump(api, vi.fn(), vi.fn());
    pump.resize({ width: 100, height: 100 });
    pump.tick(0);
    await settle();
    pump.tick(16);
    await settle();
    pump.input({ kind: 'stroke-sample', x: 1, y: 2, amount: 0 });
    await settle();
    expect(api.frame).toHaveBeenCalledTimes(3);
    pump.input({ kind: 'stroke-end', x: 2, y: 3, amount: 0 });
    await settle();
    expect(api.frame).toHaveBeenCalledTimes(3);
    pump.tick(32);
    await settle();
    expect(api.frame).toHaveBeenCalledTimes(4);
  });
  it('retains stroke samples and release while a previous frame is in flight', async () => {
    let complete!: (result: FrameResult) => void;
    const api = {
      resize: vi.fn(),
      frame: vi
        .fn()
        .mockImplementationOnce(
          () =>
            new Promise<FrameResult>((resolve) => {
              complete = resolve;
            }),
        )
        .mockResolvedValue(frame),
    };
    const pump = new FramePump(api, vi.fn(), vi.fn());
    pump.resize({ width: 100, height: 100 });
    const begin = { kind: 'stroke-begin' as const, x: 10, y: 20, amount: 0 };
    pump.input(begin);
    pump.tick(1);
    await settle();
    const point = { kind: 'stroke-sample' as const, x: 30, y: 40, amount: 0 };
    const end = { kind: 'stroke-end' as const, x: 30, y: 40, amount: 0 };
    pump.input(point);
    pump.input(end);
    pump.tick(2);
    expect(api.frame).toHaveBeenCalledTimes(1);
    complete(frame);
    await settle();
    pump.tick(3);
    await settle();
    expect(api.frame.mock.calls).toEqual([
      [1, [begin]],
      [3, [point, end]],
    ]);
  });
  it('batches ordered input in one frame RPC and retains the latest cursor', async () => {
    const calls: string[] = [];
    const api = {
      resize: vi.fn(),
      frame: vi.fn((_timestamp, inputs) => {
        calls.push('frame');
        expect(inputs).toEqual([
          { kind: 'pan', x: 3, y: 4, amount: 0 },
          { kind: 'zoom', x: 10, y: 20, amount: 2 },
          { kind: 'cursor', x: 11, y: 21, amount: 0 },
        ]);
        return frame;
      }),
    };
    const pump = new FramePump(api, vi.fn(), vi.fn());
    pump.resize({ width: 100, height: 100 });
    pump.input({ kind: 'cursor', x: 1, y: 2, amount: 0 });
    pump.input({ kind: 'pan', x: 3, y: 4, amount: 0 });
    pump.input({ kind: 'zoom', x: 10, y: 20, amount: 2 });
    pump.input({ kind: 'cursor', x: 11, y: 21, amount: 0 });
    pump.tick(1);
    await settle();
    expect(calls).toEqual(['frame']);
  });
  it('never overlaps RPC transactions and applies only the latest pending resize', async () => {
    let complete!: (result: FrameResult) => void;
    const api = {
      resize: vi.fn(),
      frame: vi
        .fn()
        .mockImplementationOnce(
          () =>
            new Promise<FrameResult>((resolve) => {
              complete = resolve;
            }),
        )
        .mockResolvedValue(frame),
    };
    const pump = new FramePump(api, vi.fn(), vi.fn());
    pump.resize({ width: 100, height: 100 });
    pump.tick(0);
    await settle();
    pump.resize({ width: 200, height: 100 });
    pump.resize({ width: 300, height: 200 });
    for (let tick = 1; tick < 50; tick++) pump.tick(tick);
    expect(api.frame).toHaveBeenCalledTimes(1);
    complete(frame);
    await settle();
    pump.tick(50);
    await settle();
    expect(api.resize.mock.calls).toEqual([
      [{ width: 100, height: 100 }],
      [{ width: 300, height: 200 }],
    ]);
    expect(api.frame).toHaveBeenCalledTimes(2);
  });
  it('pauses at zero size and resumes after resize', async () => {
    const api = { resize: vi.fn(), frame: vi.fn().mockResolvedValue(frame) };
    const pump = new FramePump(api, vi.fn(), vi.fn());
    pump.resize({ width: 0, height: 100 });
    pump.tick(0);
    await settle();
    expect(api.resize).toHaveBeenCalledWith({ width: 0, height: 100 });
    expect(api.frame).not.toHaveBeenCalled();
    pump.resize({ width: 100, height: 100 });
    pump.tick(1);
    await settle();
    expect(api.frame).toHaveBeenCalledTimes(1);
  });
  it('ignores a late result after disposal', async () => {
    let complete!: (result: FrameResult) => void;
    const api = {
      resize: vi.fn(),
      frame: vi.fn(
        () =>
          new Promise<FrameResult>((resolve) => {
            complete = resolve;
          }),
      ),
    };
    const onFrame = vi.fn();
    const pump = new FramePump(api, onFrame, vi.fn());
    pump.resize({ width: 100, height: 100 });
    pump.tick(0);
    await settle();
    pump.stop();
    complete(frame);
    await settle();
    pump.tick(1);
    expect(onFrame).not.toHaveBeenCalled();
    expect(api.frame).toHaveBeenCalledTimes(1);
  });
  it('reports a failed RPC once and stops producing frames', async () => {
    const api = { resize: vi.fn(), frame: vi.fn().mockRejectedValue(new Error('device lost')) };
    const onError = vi.fn();
    const pump = new FramePump(api, vi.fn(), onError);
    pump.resize({ width: 100, height: 100 });
    pump.tick(0);
    await settle();
    pump.tick(1);
    expect(onError).toHaveBeenCalledOnce();
    expect(onError).toHaveBeenCalledWith('Error: device lost');
    expect(api.frame).toHaveBeenCalledOnce();
  });
});
