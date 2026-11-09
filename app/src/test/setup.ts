import "@testing-library/jest-dom/vitest";
import { vi, beforeEach } from "vitest";

export const invoke = vi.fn();
export const listen = vi.fn(async (..._args: unknown[]) => () => {});

vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invoke(...a) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: (...a: unknown[]) => listen(...a) }));

beforeEach(() => {
  invoke.mockReset();
  listen.mockClear();
});
