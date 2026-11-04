// Frame sender (ARCHITECTURE §4, step 4). Owner: D1 (T-012).
//
// Hands capture frames to Rust one at a time, in order, while a session is active. Rust assigns
// sequence numbers on arrival, so order matters more than speed: each push starts only after the
// previous one settled. If Rust falls behind by more than `maxBacklog` frames (default about 3 s)
// new frames are dropped and reported once, instead of growing memory without bound.

/** Sends one frame for a session (see `pushFrame` in `ipc.ts`). */
export type PushFn = (sessionId: string, frame: ArrayBuffer) => Promise<void>;

export interface FrameSenderOptions {
  /** Frames allowed in flight before new ones are dropped. 94 frames ≈ 3 s. */
  maxBacklog?: number;
  /** First failure of a push, and the first drop. Called at most once each per sender. */
  onError?: (error: unknown) => void;
}

export class BacklogError extends Error {
  constructor(readonly dropped: number) {
    super("Audio is arriving faster than it can be sent; some was dropped.");
    this.name = "BacklogError";
  }
}

export class FrameSender {
  private tail: Promise<void> = Promise.resolve();
  private inFlight = 0;
  private reportedPushError = false;
  private reportedDrop = false;
  private closed = false;
  /** Frames handed to Rust successfully. */
  sent = 0;
  /** Frames dropped because of the backlog limit or after `close()`. */
  dropped = 0;
  /** Frames whose push failed. */
  failed = 0;

  constructor(
    readonly sessionId: string,
    private readonly push: PushFn,
    private readonly options: FrameSenderOptions = {},
  ) {}

  /** Frames waiting or in flight. */
  get backlog(): number {
    return this.inFlight;
  }

  send(frame: ArrayBuffer): void {
    if (this.closed) {
      this.dropped++;
      return;
    }
    if (this.inFlight >= (this.options.maxBacklog ?? 94)) {
      this.dropped++;
      if (!this.reportedDrop) {
        this.reportedDrop = true;
        this.options.onError?.(new BacklogError(this.dropped));
      }
      return;
    }
    this.inFlight++;
    this.tail = this.tail.then(async () => {
      try {
        await this.push(this.sessionId, frame);
        this.sent++;
      } catch (e) {
        this.failed++;
        if (!this.reportedPushError) {
          this.reportedPushError = true;
          this.options.onError?.(e);
        }
      } finally {
        this.inFlight--;
      }
    });
  }

  /** Stops accepting frames and resolves when every accepted frame has been pushed. */
  async close(): Promise<void> {
    this.closed = true;
    await this.tail;
  }
}
