import { describe, expect, it, vi } from "vitest";
import { must, nth } from "./testing";
import { BacklogError, FrameSender } from "./sender";

const frame = (tag: number) => Uint8Array.from([tag]).buffer;
const tagOf = (f: ArrayBuffer) => new Uint8Array(f)[0];

describe("FrameSender", () => {
  it("pushes frames in order, one at a time, even when pushes take different times", async () => {
    const order: number[] = [];
    let concurrent = 0;
    let maxConcurrent = 0;
    const push = vi.fn(async (_id: string, f: ArrayBuffer) => {
      concurrent++;
      maxConcurrent = Math.max(maxConcurrent, concurrent);
      await new Promise((r) => setTimeout(r, tagOf(f) === 0 ? 20 : 1));
      order.push(must(tagOf(f)));
      concurrent--;
    });
    const sender = new FrameSender("s1", push);
    for (let i = 0; i < 5; i++) sender.send(frame(i));
    expect(sender.backlog).toBe(5);
    await sender.close();
    expect(order).toEqual([0, 1, 2, 3, 4]);
    expect(maxConcurrent).toBe(1);
    expect(push.mock.calls.every(([id]) => id === "s1")).toBe(true);
    expect(sender.sent).toBe(5);
    expect(sender.backlog).toBe(0);
  });

  it("drops new frames beyond the backlog limit and reports it once", async () => {
    let release: () => void = () => undefined;
    const gate = new Promise<void>((r) => (release = r));
    const onError = vi.fn();
    const sender = new FrameSender("s", () => gate, { maxBacklog: 2, onError });
    for (let i = 0; i < 5; i++) sender.send(frame(i));
    expect(sender.dropped).toBe(3);
    expect(onError).toHaveBeenCalledTimes(1);
    expect(nth(onError.mock.calls, 0)[0]).toBeInstanceOf(BacklogError);
    release();
    await sender.close();
    expect(sender.sent).toBe(2);
  });

  it("keeps going after a failed push and reports only the first failure", async () => {
    const onError = vi.fn();
    const push = vi.fn(async (_id: string, f: ArrayBuffer) => {
      if (must(tagOf(f)) < 2) throw new Error(`push ${tagOf(f)} failed`);
    });
    const sender = new FrameSender("s", push, { onError });
    for (let i = 0; i < 4; i++) sender.send(frame(i));
    await sender.close();
    expect(sender.failed).toBe(2);
    expect(sender.sent).toBe(2);
    expect(onError).toHaveBeenCalledTimes(1);
    expect((nth(onError.mock.calls, 0)[0] as Error).message).toBe("push 0 failed");
  });

  it("ignores frames after close()", async () => {
    const push = vi.fn(async () => undefined);
    const sender = new FrameSender("s", push);
    await sender.close();
    sender.send(frame(1));
    await sender.close();
    expect(push).not.toHaveBeenCalled();
    expect(sender.dropped).toBe(1);
  });
});
