import "@testing-library/jest-dom/vitest";
import { vi, beforeEach } from "vitest";

export const invoke = vi.fn();
export const listen = vi.fn(async (..._args: unknown[]) => () => {});

vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invoke(...a) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: (...a: unknown[]) => listen(...a) }));

export const win = {
  minimize: vi.fn(async () => {}),
  toggleMaximize: vi.fn(async () => {}),
  close: vi.fn(async () => {}),
  isMaximized: vi.fn(async () => false),
  onResized: vi.fn(async () => () => {}),
};
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: () => win }));

beforeEach(() => {
  invoke.mockReset();
  listen.mockClear();
  for (const f of Object.values(win)) f.mockClear();
});
