import { describe, expect, it, vi } from 'vitest';
import { FramePump } from './frame-pump';
import type { FrameResult } from './types';
const frame: FrameResult = { presented: true, frames: 1, width: 100, height: 100 };
const settle = async () => {
  await new Promise((resolve) => setTimeout(resolve, 0));
};

describe('FramePump ownership and backpressure', () => {
  it('batches ordered navigation before the next frame and retains the latest cursor', async () => {
    const calls: string[] = [];
    const api = {
      resize: vi.fn(),
      interact: vi.fn((inputs) => {
        calls.push('input');
        expect(inputs).toEqual([
          { kind: 'pan', x: 3, y: 4, amount: 0 },
          { kind: 'zoom', x: 10, y: 20, amount: 2 },
          { kind: 'cursor', x: 11, y: 21, amount: 0 },
        ]);
      }),
      frame: vi.fn(() => {
        calls.push('frame');
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
    expect(calls).toEqual(['input', 'frame']);
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
