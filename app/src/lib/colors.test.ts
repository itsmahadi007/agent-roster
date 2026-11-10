import { describe, it, expect } from "vitest";
import { agentHex, colorName, swatches, usageLevel } from "./colors";
import overview from "./fixtures/overview.json";

describe("usage colors", () => {
  it("come from use alone: calm under 70, amber 70 to 89, red from 90", () => {
    expect(usageLevel(0)).toBe("ok");
    expect(usageLevel(69)).toBe("ok");
    expect(usageLevel(70)).toBe("warn");
    expect(usageLevel(89)).toBe("warn");
    expect(usageLevel(90)).toBe("hot");
    expect(usageLevel(100)).toBe("hot");
  });

  it("a reached limit is red at any use", () => {
    expect(usageLevel(5, true)).toBe("hot");
  });
});

describe("agent palette", () => {
  it("shows the stored colors as cobalt, cyan, jade, violet and pink", () => {
    expect(swatches(overview.palette)).toEqual([
      { value: "cobalt", label: "cobalt", hex: "#4f7cff" },
      { value: "marigold", label: "cyan", hex: "#3cc7e6" },
      { value: "jade", label: "jade", hex: "#2ec4a0" },
      { value: "orchid", label: "violet", hex: "#b86bff" },
      { value: "coral", label: "pink", hex: "#ff6bc4" },
    ]);
  });

  it("maps coral to pink and marigold to cyan for display, by label and by hex", () => {
    expect(colorName("coral")).toBe("pink");
    expect(colorName("marigold")).toBe("cyan");
    expect(agentHex("#ff7a6b")).toBe("#ff6bc4");
    expect(agentHex("#F0B429")).toBe("#3cc7e6");
  });

  it("passes new or unknown colors through", () => {
    expect(colorName("pink")).toBe("pink");
    expect(agentHex("#123456")).toBe("#123456");
    expect(agentHex(null)).toBe("var(--mut)");
    expect(swatches([{ label: "pink", hex: "#ff6bc4" }])).toEqual([{ value: "pink", label: "pink", hex: "#ff6bc4" }]);
  });
});
