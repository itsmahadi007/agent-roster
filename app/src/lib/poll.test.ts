import { describe, it, expect, vi, afterEach } from "vitest";
import { startPoll } from "./poll";

afterEach(() => vi.useRealTimers());

describe("startPoll", () => {
  it("poll_skips_while_a_request_is_in_flight", async () => {
    vi.useFakeTimers();
    const fn = vi.fn(() => new Promise<void>(() => {}));
    const stop = startPoll(fn, 1000);
    await vi.advanceTimersByTimeAsync(3500);
    expect(fn).toHaveBeenCalledTimes(1);
    stop();
  });

  it("polls again after each call finishes and stops when asked", async () => {
    vi.useFakeTimers();
    const fn = vi.fn(async () => {});
    const stop = startPoll(fn, 1000);
    await vi.advanceTimersByTimeAsync(2500);
    expect(fn).toHaveBeenCalledTimes(3);
    stop();
    await vi.advanceTimersByTimeAsync(5000);
    expect(fn).toHaveBeenCalledTimes(3);
  });

  it("keeps polling after a failed call", async () => {
    vi.useFakeTimers();
    const fn = vi.fn(async () => {
      throw new Error("tool hung");
    });
    const stop = startPoll(fn, 1000);
    await vi.advanceTimersByTimeAsync(1500);
    expect(fn).toHaveBeenCalledTimes(2);
    stop();
  });
});
