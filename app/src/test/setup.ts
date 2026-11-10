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

export const updater = {
  check: vi.fn(async (): Promise<unknown> => null),
  relaunch: vi.fn(async () => {}),
  getVersion: vi.fn(async () => "0.2.0"),
};
vi.mock("@tauri-apps/plugin-updater", () => ({ check: () => updater.check() }));
vi.mock("@tauri-apps/plugin-process", () => ({ relaunch: () => updater.relaunch() }));
vi.mock("@tauri-apps/api/app", () => ({ getVersion: () => updater.getVersion() }));

beforeEach(() => {
  invoke.mockReset();
  listen.mockClear();
  for (const f of Object.values(win)) f.mockClear();
  updater.check.mockReset().mockResolvedValue(null);
  updater.relaunch.mockClear();
});
