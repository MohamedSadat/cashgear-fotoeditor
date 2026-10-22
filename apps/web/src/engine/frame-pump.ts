import type { EngineApi, FrameResult, PhysicalSize } from './types';
import type { Interaction } from './types';

// One transaction in flight, irrespective of how quickly the UI produces ticks.
// Resizes arriving during a transaction replace older pending sizes.
export class FramePump {
  private busy = false;
  private stopped = false;
  private size: PhysicalSize = { width: 0, height: 0 };
  private pendingSize: PhysicalSize | undefined;
  private inputs: Interaction[] = [];
  private epoch = 0;
  private epochTime = 0;
  private presentedEpoch = -1;
  private resizedEpoch = -1;
  private armed = false;
  private immediateScheduled = false;
  private painting = false;
  private previewTimer: ReturnType<typeof setTimeout> | undefined;
  constructor(
    private api: Pick<EngineApi, 'resize' | 'frame'>,
    private onFrame: (frame: FrameResult) => void,
    private onError: (message: string) => void,
  ) {}
  resize(size: PhysicalSize) {
    if (this.stopped) return;
    this.size = size;
    this.pendingSize = size;
  }
  input(input: Interaction) {
    if (this.stopped) return;
    if (input.kind === 'cursor') this.inputs = this.inputs.filter((i) => i.kind !== 'cursor');
    if (this.inputs.length >= 4096) {
      this.inputs = [{ kind: 'stroke-cancel', x: 0, y: 0, amount: -1 }];
      return;
    }
    this.inputs.push(input);
    if (input.kind !== 'cursor') this.scheduleInput();
  }
  tick(timestamp: number) {
    if (this.stopped) return;
    ++this.epoch;
    this.epochTime = performance.timeOrigin + timestamp;
    this.armed = true;
    this.flush(timestamp);
    clearTimeout(this.previewTimer);
    const epoch = this.epoch;
    // Reserve the first half of a 60 Hz frame for newly delivered raw input.
    // If no dab needed presentation, keep the active preview moving afterward.
    this.previewTimer = setTimeout(() => {
      if (
        this.epoch === epoch &&
        this.painting &&
        !this.pendingSize &&
        this.presentedEpoch < epoch &&
        this.resizedEpoch < epoch
      )
        this.flush(performance.now(), true);
    }, 8);
  }
  private scheduleInput() {
    if (this.immediateScheduled || !this.armed || this.stopped) return;
    this.immediateScheduled = true;
    queueMicrotask(() => {
      this.immediateScheduled = false;
      if (
        this.inputs.length &&
        !this.pendingSize &&
        this.presentedEpoch < this.epoch &&
        this.resizedEpoch < this.epoch
      )
        this.flush(performance.now());
    });
  }
  private flush(timestamp: number, preview = false) {
    if (this.stopped || this.busy) return;
    this.busy = true;
    const resize = this.pendingSize;
    this.pendingSize = undefined;
    void this.dispatch(timestamp, resize, preview);
  }
  private async dispatch(timestamp: number, resize?: PhysicalSize, preview = false) {
    try {
      if (resize) {
        await this.api.resize(resize);
        this.resizedEpoch = this.epoch;
      }
      if (this.stopped || this.size.width === 0 || this.size.height === 0) return;
      const inputs = this.inputs;
      this.inputs = [];
      const frame = await (preview
        ? this.api.frame(timestamp, inputs, true)
        : this.api.frame(timestamp, inputs));
      if (frame.snapshot && this.inputs.some((input) => input.kind.startsWith('stroke-')))
        frame.snapshot = { ...frame.snapshot, painting: true };
      this.painting = frame.snapshot?.painting ?? false;
      if (frame.presented)
        this.presentedEpoch =
          frame.presentedAt !== undefined && frame.presentedAt < this.epochTime
            ? this.epoch - 1
            : this.epoch;
      if (!this.stopped) this.onFrame(frame);
    } catch (error) {
      if (!this.stopped) {
        this.stopped = true;
        this.onError(String(error));
      }
    } finally {
      this.busy = false;
      if (this.inputs.length) this.scheduleInput();
    }
  }
  stop() {
    this.stopped = true;
    clearTimeout(this.previewTimer);
  }
}
