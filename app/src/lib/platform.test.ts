import { describe, it, expect } from "vitest";
import { chromeFor } from "./platform";

describe("chromeFor", () => {
  it("knows each system", () => {
    expect(chromeFor("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15")).toBe("mac");
    expect(chromeFor("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Edg/131.0")).toBe("windows");
    expect(chromeFor("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/605.1.15")).toBe("none");
  });
});
