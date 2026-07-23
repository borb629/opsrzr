// Pure presentation rules: given a device's reported type + capabilities,
// decide what the UI is allowed to show. Kept free of DOM and Tauri so the
// capability gating is unit-tested in isolation (device-presentation.test.ts)
// and the DOM renderers in main.ts consume it.
//
// Only reported capabilities drive the UI — there is no pairing or
// keyboard-remapping surface because the backend DTO exposes neither.

export interface PresentableDevice {
  deviceType: "mouse" | "keyboard";
  connected: boolean;
  battery: { levelPercent: number; charging: boolean } | null;
  capabilities: { dpi: boolean; battery: boolean };
}

export interface DevicePresentation {
  /** Battery surfaces appear only for battery-capable devices. */
  showBattery: boolean;
  /** DPI / sensitivity controls appear only for DPI-capable mice. */
  showMousePerformance: boolean;
  /** Keyboard-context framing appears for keyboards. */
  showKeyboardContext: boolean;
  /** Human-readable connection descriptor (never color-only status). */
  connectionLabel: string;
}

export function devicePresentation(device: PresentableDevice): DevicePresentation {
  const showBattery = device.capabilities.battery;
  return {
    showBattery,
    showMousePerformance: device.deviceType === "mouse" && device.capabilities.dpi,
    showKeyboardContext: device.deviceType === "keyboard",
    // Battery capability implies a wireless device; wired devices report none.
    connectionLabel: showBattery ? "Wireless device" : "USB wired",
  };
}

export function summarySections(device: PresentableDevice): string[] {
  return ["connection", ...(devicePresentation(device).showBattery ? ["battery"] : []), "onboard"];
}
