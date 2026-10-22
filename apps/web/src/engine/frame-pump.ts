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
  constructor(
    private api: Pick<EngineApi, 'resize' | 'frame'> & Partial<Pick<EngineApi, 'interact'>>,
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
    this.inputs.push(input);
  }
  tick(timestamp: number) {
    if (this.stopped || this.busy) return;
    this.busy = true;
    const resize = this.pendingSize;
    this.pendingSize = undefined;
    void this.dispatch(timestamp, resize);
  }
  private async dispatch(timestamp: number, resize?: PhysicalSize) {
    try {
      if (resize) await this.api.resize(resize);
      if (this.inputs.length) {
        const inputs = this.inputs;
        this.inputs = [];
        await this.api.interact?.(inputs);
      }
      if (this.stopped || this.size.width === 0 || this.size.height === 0) return;
      const frame = await this.api.frame(timestamp);
      if (!this.stopped) this.onFrame(frame);
    } catch (error) {
      if (!this.stopped) {
        this.stopped = true;
        this.onError(String(error));
      }
    } finally {
      this.busy = false;
    }
  }
  stop() {
    this.stopped = true;
  }
}
