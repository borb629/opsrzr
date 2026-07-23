import { describe, expect, it } from "vitest";
import { devicePresentation, summarySections } from "./device-presentation";

const wiredKeyboard = {
  deviceType: "keyboard" as const,
  connected: true,
  battery: null,
  capabilities: { dpi: false, battery: false },
};

describe("device presentation", () => {
  it("hides battery and mouse-only controls for a wired keyboard", () => {
    expect(devicePresentation(wiredKeyboard)).toMatchObject({
      showBattery: false,
      showMousePerformance: false,
      showKeyboardContext: true,
      connectionLabel: "USB wired",
    });
    expect(summarySections(wiredKeyboard)).not.toContain("battery");
  });

  it("shows battery context for a battery-capable mouse", () => {
    const wirelessMouse = {
      ...wiredKeyboard,
      deviceType: "mouse" as const,
      battery: { levelPercent: 61, charging: true },
      capabilities: { dpi: true, battery: true },
    };
    expect(devicePresentation(wirelessMouse)).toMatchObject({
      showBattery: true,
      showMousePerformance: true,
      connectionLabel: "Wireless device",
    });
    expect(summarySections(wirelessMouse)).toContain("battery");
  });

  it("does not add a battery section when a device lacks the battery capability", () => {
    expect(summarySections(wiredKeyboard)).toEqual(["connection", "onboard"]);
  });

  it("keeps battery content out of wired-device summaries", () => {
    expect(summarySections(wiredKeyboard)).not.toContain("battery");
  });
});
