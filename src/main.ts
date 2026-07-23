import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import { devicePresentation, summarySections } from "./device-presentation";

// ---------------------------------------------------------------------------
// Types (mirror the Rust DTOs; all camelCase per the app-side serde renames).
// ---------------------------------------------------------------------------

type DeviceType = "mouse" | "keyboard";
type Effect = "static" | "breathing" | "spectrum" | "wave" | "reactive" | "none";

interface Capabilities {
  lighting: boolean;
  dpi: boolean;
  pollingRate: boolean;
  battery: boolean;
  onboard: boolean;
}

interface Battery {
  levelPercent: number;
  charging: boolean;
}

interface DeviceInfo {
  name: string;
  usbPid: number;
  deviceType: DeviceType;
  capabilities: Capabilities;
  connected: boolean;
  dpiMin: number | null;
  dpiMax: number | null;
  battery: Battery | null;
}

interface DeviceState {
  dpiX: number | null;
  dpiY: number | null;
  pollingHz: number | null;
  brightness: number | null;
}

interface DeviceSettingsPayload {
  lighting?: { effect: Effect; color: [number, number, number]; brightness: number };
  dpi?: { x: number; y: number };
  pollingHz?: number;
}

interface Profile {
  name: string;
  devices: Record<string, DeviceSettingsPayload>;
}

// The UI's working state for a device (superset of what a profile stores).
interface Working {
  lighting: { effect: Effect; color: [number, number, number]; brightness: number };
  dpi: { x: number; y: number };
  pollingHz: number;
  stages: number[];
  activeStage: number;
  loaded: boolean;
}

const EFFECTS: { value: Effect; label: string; hint: string }[] = [
  { value: "static", label: "Static", hint: "A single steady colour." },
  { value: "breathing", label: "Breathing", hint: "Fades a single colour in and out." },
  { value: "spectrum", label: "Spectrum", hint: "Cycles smoothly through the whole colour wheel." },
  { value: "wave", label: "Wave", hint: "A moving rainbow across the device." },
  { value: "reactive", label: "Reactive", hint: "Lights up briefly on each key press or click." },
  { value: "none", label: "Off", hint: "Turns lighting off." },
];

const COLOR_EFFECTS: Effect[] = ["static", "breathing", "reactive"];
const POLLING_RATES = [125, 500, 1000];
const MAX_STAGES = 5;
// Devices reject a single-stage cycle, so working state always keeps at least two.
const MIN_STAGES = 2;

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

let devices: DeviceInfo[] = [];
let selectedPid: number | null = null;
const working = new Map<number, Working>();
let profileNames: string[] = [];
let activeProfile: string | null = null;
// PIDs whose live settings are currently being read from hardware. While a
// PID is in here the detail canvas shows a loading skeleton instead of the
// default working values, so we never flash fabricated numbers at the user.
const reading = new Set<number>();

// ---------------------------------------------------------------------------
// Small utilities
// ---------------------------------------------------------------------------

function el<T extends HTMLElement>(sel: string): T {
  const node = document.querySelector<T>(sel);
  if (!node) throw new Error(`missing element: ${sel}`);
  return node;
}

function pidHex(pid: number): string {
  return `0x${pid.toString(16).padStart(4, "0").toUpperCase()}`;
}

function clamp(v: number, lo: number, hi: number): number {
  return Math.min(Math.max(v, lo), hi);
}

function rgbToHex([r, g, b]: [number, number, number]): string {
  const h = (n: number) => n.toString(16).padStart(2, "0");
  return `#${h(r)}${h(g)}${h(b)}`;
}

function hexToRgb(hex: string): [number, number, number] {
  const m = /^#?([0-9a-f]{6})$/i.exec(hex.trim());
  if (!m) return [255, 255, 255];
  const n = parseInt(m[1], 16);
  return [(n >> 16) & 0xff, (n >> 8) & 0xff, n & 0xff];
}

function pctOf255(v: number): number {
  return Math.round((v / 255) * 100);
}

// Paint a range input's filled portion. The accent track grows to the current
// value via a --pct custom property the CSS gradient reads.
function setRangeFill(input: HTMLInputElement): void {
  const min = Number(input.min || "0");
  const max = Number(input.max || "100");
  const val = Number(input.value);
  const pct = max > min ? ((val - min) / (max - min)) * 100 : 0;
  input.style.setProperty("--pct", `${clamp(pct, 0, 100)}%`);
}

function makeEl<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  className?: string,
  text?: string,
): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
}

let toastTimer: number | undefined;
function toast(message: string, kind: "error" | "info" = "error") {
  const t = el<HTMLDivElement>("#toast");
  t.textContent = message;
  t.className = `toast show ${kind === "info" ? "info" : "error"}`;
  window.clearTimeout(toastTimer);
  toastTimer = window.setTimeout(() => {
    t.className = "toast";
  }, 3200);
}

const debounceTimers = new Map<string, number>();
function debounce(key: string, fn: () => void, ms = 160) {
  window.clearTimeout(debounceTimers.get(key));
  debounceTimers.set(key, window.setTimeout(fn, ms));
}

// ---------------------------------------------------------------------------
// Command wrappers
// ---------------------------------------------------------------------------

// Plain-language names for each backend command, used to build user-facing
// error copy. The raw error still goes to the console for debugging.
const CMD_ACTIONS: Record<string, string> = {
  set_lighting: "update lighting",
  set_dpi: "change sensitivity",
  set_dpi_stages: "apply DPI stages",
  set_polling_rate: "change the polling rate",
  read_device_state: "read the device",
  save_profile: "save the profile",
  apply_profile: "apply the profile",
  delete_profile: "delete the profile",
  list_devices: "load devices",
  list_profiles: "load profiles",
  get_active_profile: "load profiles",
  autostart_enabled: "check the launch-at-login setting",
  set_autostart: "change the launch-at-login setting",
};

// Commands that talk to hardware — their most common failure is a dropped
// device, so the recovery hint points there.
const DEVICE_CMDS = new Set([
  "set_lighting",
  "set_dpi",
  "set_dpi_stages",
  "set_polling_rate",
  "read_device_state",
]);

function friendlyError(name: string): string {
  const action = CMD_ACTIONS[name] ?? "complete that action";
  return DEVICE_CMDS.has(name)
    ? `Couldn’t ${action}. The device may have disconnected. Reconnect it and try again.`
    : `Couldn’t ${action}. Please try again.`;
}

async function cmd<T>(name: string, args?: Record<string, unknown>): Promise<T | undefined> {
  try {
    return await invoke<T>(name, args);
  } catch (err) {
    console.error(`${name} failed:`, err);
    toast(friendlyError(name));
    return undefined;
  }
}

// ---------------------------------------------------------------------------
// Working-state helpers
// ---------------------------------------------------------------------------

function defaultWorking(): Working {
  return {
    lighting: { effect: "static", color: [255, 255, 255], brightness: 255 },
    dpi: { x: 800, y: 800 },
    pollingHz: 1000,
    stages: [800, 1600],
    activeStage: 0,
    loaded: false,
  };
}

// A device never cycles a single stage, so seed a distinct second stage around
// the primary DPI whenever we only have one value (device read, profile apply).
function seedStages(primary: number, lo: number, hi: number): number[] {
  let second = primary * 2 <= hi ? primary * 2 : Math.round(primary / 2);
  second = clamp(second, lo, hi);
  if (second === primary) second = primary < hi ? hi : lo;
  return [primary, second];
}

function getWorking(pid: number): Working {
  let w = working.get(pid);
  if (!w) {
    w = defaultWorking();
    working.set(pid, w);
  }
  return w;
}

// Mark the device as "reading" before the first paint so the loading skeleton
// shows immediately, rather than defaults-then-snap.
function markReadingIfNeeded(pid: number | null) {
  if (pid == null) return;
  const dev = devices.find((d) => d.usbPid === pid);
  if (dev && dev.connected && !getWorking(pid).loaded) reading.add(pid);
}

async function ensureLoaded(dev: DeviceInfo) {
  const w = getWorking(dev.usbPid);
  if (w.loaded || !dev.connected) {
    reading.delete(dev.usbPid);
    return;
  }
  w.loaded = true; // mark first so concurrent selects don't double-read
  reading.add(dev.usbPid);
  const state = await cmd<DeviceState>("read_device_state", { pid: dev.usbPid });
  reading.delete(dev.usbPid); // clear whether or not the read succeeded
  if (state) {
    if (state.dpiX != null && state.dpiY != null) {
      w.dpi = { x: state.dpiX, y: state.dpiY };
      w.stages = seedStages(state.dpiX, dev.dpiMin ?? 100, dev.dpiMax ?? 20000);
    }
    if (state.pollingHz != null) w.pollingHz = state.pollingHz;
    if (state.brightness != null) w.lighting.brightness = state.brightness;
  }
  if (selectedPid === dev.usbPid) renderDetail();
}

function workingToSettings(dev: DeviceInfo, w: Working): DeviceSettingsPayload {
  const s: DeviceSettingsPayload = {};
  if (dev.capabilities.lighting) {
    s.lighting = { effect: w.lighting.effect, color: w.lighting.color, brightness: w.lighting.brightness };
  }
  if (dev.capabilities.dpi) s.dpi = { x: w.dpi.x, y: w.dpi.y };
  if (dev.capabilities.pollingRate) s.pollingHz = w.pollingHz;
  return s;
}

// ---------------------------------------------------------------------------
// Device writes (unchanged Tauri command contracts)
// ---------------------------------------------------------------------------

function pushLighting(pid: number) {
  const w = getWorking(pid);
  const [r, g, b] = w.lighting.color;
  debounce(`light-${pid}`, () => {
    void cmd("set_lighting", { pid, effect: w.lighting.effect, r, g, b, brightness: w.lighting.brightness });
  });
}

function pushDpi(pid: number) {
  const w = getWorking(pid);
  debounce(`dpi-${pid}`, () => {
    void cmd("set_dpi", { pid, x: w.dpi.x, y: w.dpi.y });
  });
}

function pushPolling(pid: number) {
  const w = getWorking(pid);
  void cmd("set_polling_rate", { pid, hz: w.pollingHz });
}

// Write the current stage set + active index to the device so the physical DPI
// button always cycles the up-to-date stages. Debounced like the other writes.
function pushStages(pid: number) {
  const w = getWorking(pid);
  debounce(`stages-${pid}`, () => {
    const payload = w.stages.map((v): [number, number] => [v, v]);
    void cmd("set_dpi_stages", { pid, stages: payload, active: w.activeStage });
  });
}

// ---------------------------------------------------------------------------
// Rendering: device rail
// ---------------------------------------------------------------------------

function renderDeviceList() {
  const list = el<HTMLUListElement>("#device-list");
  list.innerHTML = "";
  if (devices.length === 0) {
    const li = makeEl("li", "rail-empty", "No supported devices detected.");
    list.appendChild(li);
    return;
  }
  for (const dev of devices) {
    const li = makeEl("li");
    li.className = `device-item ${dev.connected ? "connected" : "disconnected"}${
      dev.usbPid === selectedPid ? " selected" : ""
    }`;
    li.setAttribute("role", "button");
    li.setAttribute("tabindex", "0");
    li.setAttribute("aria-pressed", String(dev.usbPid === selectedPid));

    const dot = makeEl("span", "dot");
    dot.setAttribute("aria-hidden", "true");

    const meta = makeEl("div", "device-meta");
    meta.appendChild(makeEl("div", "device-name", dev.name));
    meta.appendChild(makeEl("div", "device-sub", railSubLine(dev)));

    li.append(dot, meta);
    const activate = () => selectDevice(dev.usbPid);
    li.addEventListener("click", activate);
    li.addEventListener("keydown", (e) => {
      if (e.key === "Enter" || e.key === " ") {
        e.preventDefault();
        activate();
      }
    });
    list.appendChild(li);
  }
}

function railSubLine(dev: DeviceInfo): string {
  if (!dev.connected) return `${dev.deviceType} · disconnected`;
  const view = devicePresentation(dev);
  if (view.showBattery && dev.battery) {
    return `${dev.deviceType} · ${dev.battery.levelPercent}%${dev.battery.charging ? ", charging" : ""}`;
  }
  return `${dev.deviceType} · connected`;
}

// ---------------------------------------------------------------------------
// Rendering: settings canvas
// ---------------------------------------------------------------------------

function selectDevice(pid: number) {
  selectedPid = pid;
  markReadingIfNeeded(pid);
  renderDeviceList();
  renderDetail();
  const dev = devices.find((d) => d.usbPid === pid);
  if (dev) void ensureLoaded(dev);
}

function renderDetail() {
  const detail = el<HTMLElement>("#detail");

  // Preserve keyboard focus across the full-subtree rebuild. Controls carry a
  // stable data-fkey so we can re-focus the equivalent element afterwards;
  // without this, any async re-render (device read, battery event) would drop
  // a keyboard or screen-reader user back to <body>.
  const active = document.activeElement as HTMLElement | null;
  const fkey = active && detail.contains(active) ? active.getAttribute("data-fkey") : null;
  let caret: [number | null, number | null] | null = null;
  if (fkey && active instanceof HTMLInputElement && active.type === "text") {
    caret = [active.selectionStart, active.selectionEnd];
  }

  const dev = devices.find((d) => d.usbPid === selectedPid);

  detail.innerHTML = "";

  if (!dev) {
    detail.appendChild(renderEmptyState());
    renderSummary(null);
    return;
  }

  detail.appendChild(renderDeviceHeader(dev));

  if (!dev.connected) {
    const note = makeEl("div", "notice");
    note.appendChild(
      makeEl("p", undefined, "This device is remembered but not currently connected. Reconnect it to change its settings."),
    );
    detail.appendChild(note);
    renderSummary(dev);
    return;
  }

  // Reading live settings from hardware — show a skeleton, not fabricated
  // defaults, until the real values arrive.
  if (reading.has(dev.usbPid)) {
    detail.appendChild(renderReadingState());
    renderSummary(dev);
    return;
  }

  detail.appendChild(renderCanvasHint());

  const view = devicePresentation(dev);

  if (dev.capabilities.lighting) detail.appendChild(renderLightingSection(dev));
  if (view.showMousePerformance) detail.appendChild(renderDpiSection(dev));
  if (dev.capabilities.pollingRate) detail.appendChild(renderPollingSection(dev));
  if (view.showBattery) detail.appendChild(renderBatterySection(dev));
  if (view.showKeyboardContext) detail.appendChild(renderKeyboardContext());

  renderSummary(dev);

  if (fkey) {
    const restore = detail.querySelector<HTMLElement>(`[data-fkey="${CSS.escape(fkey)}"]`);
    if (restore) {
      restore.focus();
      if (caret && restore instanceof HTMLInputElement) {
        try {
          restore.setSelectionRange(caret[0], caret[1]);
        } catch {
          /* setSelectionRange is unsupported on some input types; ignore */
        }
      }
    }
  }
}

// Loading skeleton shown while a device's live settings are being read.
function renderReadingState(): HTMLElement {
  const wrap = makeEl("div", "reading");
  const status = makeEl("p", "sr-only", "Reading current settings from the device.");
  status.setAttribute("role", "status");
  wrap.appendChild(status);
  const skeleton = makeEl("div", "skeleton");
  skeleton.setAttribute("aria-hidden", "true");
  for (let i = 0; i < 2; i += 1) {
    const g = makeEl("div", "skeleton-group");
    g.appendChild(makeEl("div", "sk sk-title"));
    g.appendChild(makeEl("div", "sk sk-row"));
    g.appendChild(makeEl("div", "sk sk-row"));
    skeleton.appendChild(g);
  }
  wrap.appendChild(skeleton);
  return wrap;
}

// Sets expectations up front: most controls write to the device live, so the
// user isn't left wondering whether a change without an explicit Apply landed.
function renderCanvasHint(): HTMLElement {
  return makeEl(
    "p",
    "canvas-hint",
    "Lighting, sensitivity, polling, and DPI stages apply to the device as you change them.",
  );
}

function renderEmptyState(): HTMLElement {
  const wrap = makeEl("div", "empty");
  wrap.appendChild(makeEl("p", "empty-title", "Select a device"));
  wrap.appendChild(
    makeEl("p", "empty-sub", "Choose a device from the rail to adjust its lighting, sensitivity, and polling rate."),
  );
  return wrap;
}

function renderDeviceHeader(dev: DeviceInfo): HTMLElement {
  const view = devicePresentation(dev);
  const header = makeEl("header", "detail-header");
  header.appendChild(makeEl("h1", "detail-title", dev.name));
  const sub = makeEl("p", "detail-sub");
  // One separator per metadata line: type · connection, then the USB id as a
  // de-emphasised tag (not a third dotted clause).
  sub.append(
    makeEl("span", undefined, `${dev.deviceType === "mouse" ? "Mouse" : "Keyboard"}`),
    makeEl("span", "sep", "·"),
    makeEl("span", undefined, view.connectionLabel),
    makeEl("span", "detail-pid", pidHex(dev.usbPid)),
  );
  header.appendChild(sub);
  return header;
}

// ---- section scaffolding (grouped settings, not floating cards) ----

function section(title: string, description?: string): HTMLElement {
  const s = makeEl("section", "group");
  const head = makeEl("div", "group-head");
  head.appendChild(makeEl("h2", "group-title", title));
  if (description) head.appendChild(makeEl("p", "group-desc", description));
  s.appendChild(head);
  return s;
}

function row(label: string, control: HTMLElement, controlId?: string): HTMLElement {
  const r = makeEl("div", "row");
  const l = makeEl("label", "row-label", label);
  if (controlId) l.setAttribute("for", controlId);
  const value = makeEl("div", "row-control");
  value.appendChild(control);
  r.append(l, value);
  return r;
}

let controlSeq = 0;
function nextId(prefix: string): string {
  controlSeq += 1;
  return `${prefix}-${controlSeq}`;
}

// ---- lighting ----

function renderLightingSection(dev: DeviceInfo): HTMLElement {
  const w = getWorking(dev.usbPid);
  const s = section("Lighting");

  // Effect chips (toggle group with accessible pressed state).
  const chips = makeEl("div", "chip-row");
  chips.setAttribute("role", "group");
  chips.setAttribute("aria-label", "Lighting effect");
  for (const eff of EFFECTS) {
    const active = w.lighting.effect === eff.value;
    const chip = makeEl("button", `chip${active ? " active" : ""}`, eff.label);
    chip.type = "button";
    chip.title = eff.hint;
    chip.setAttribute("aria-pressed", String(active));
    chip.setAttribute("data-fkey", `effect:${eff.value}`);
    chip.addEventListener("click", () => {
      w.lighting.effect = eff.value;
      pushLighting(dev.usbPid);
      renderDetail();
    });
    chips.appendChild(chip);
  }
  s.appendChild(row("Effect", chips));

  if (COLOR_EFFECTS.includes(w.lighting.effect)) {
    const id = nextId("colour");
    const color = makeEl("input");
    color.type = "color";
    color.id = id;
    color.value = rgbToHex(w.lighting.color);
    color.setAttribute("aria-label", "Lighting colour");
    color.setAttribute("data-fkey", "colour");
    color.addEventListener("input", () => {
      w.lighting.color = hexToRgb(color.value);
      pushLighting(dev.usbPid);
    });
    s.appendChild(row("Colour", color, id));
  }

  const id = nextId("brightness");
  const wrap = makeEl("div", "slider");
  const brightness = makeEl("input", "range");
  brightness.type = "range";
  brightness.id = id;
  brightness.min = "0";
  brightness.max = "255";
  brightness.value = String(w.lighting.brightness);
  brightness.setAttribute("data-fkey", "brightness");
  setRangeFill(brightness);
  const badge = makeEl("span", "value-badge", `${pctOf255(w.lighting.brightness)}%`);
  brightness.addEventListener("input", () => {
    w.lighting.brightness = Number(brightness.value);
    badge.textContent = `${pctOf255(w.lighting.brightness)}%`;
    setRangeFill(brightness);
    pushLighting(dev.usbPid);
  });
  wrap.append(brightness, badge);
  s.appendChild(row("Brightness", wrap, id));

  return s;
}

// ---- DPI / performance ----

function renderDpiSection(dev: DeviceInfo): HTMLElement {
  const w = getWorking(dev.usbPid);
  const lo = dev.dpiMin ?? 100;
  const hi = dev.dpiMax ?? 20000;
  const s = section("Sensitivity", `DPI range ${lo}-${hi}`);

  const id = nextId("dpi");
  const wrap = makeEl("div", "slider");
  const slider = makeEl("input", "range");
  slider.type = "range";
  slider.id = id;
  slider.min = String(lo);
  slider.max = String(hi);
  slider.step = "50";
  slider.value = String(clamp(w.dpi.x, lo, hi));
  slider.setAttribute("data-fkey", "dpi-slider");
  setRangeFill(slider);

  const num = makeEl("input", "num");
  num.type = "number";
  num.min = String(lo);
  num.max = String(hi);
  num.value = String(w.dpi.x);
  num.setAttribute("aria-label", "DPI value");
  num.setAttribute("data-fkey", "dpi-num");

  const apply = (raw: number) => {
    const v = clamp(Math.round(raw), lo, hi);
    w.dpi = { x: v, y: v };
    slider.value = String(v);
    num.value = String(v);
    setRangeFill(slider);
    pushDpi(dev.usbPid);
  };
  slider.addEventListener("input", () => apply(Number(slider.value)));
  num.addEventListener("change", () => apply(Number(num.value)));

  wrap.append(slider, num);
  s.appendChild(row("DPI", wrap, id));
  s.appendChild(renderStagesEditor(dev, w, lo, hi));
  return s;
}

function renderStagesEditor(dev: DeviceInfo, w: Working, lo: number, hi: number): HTMLElement {
  const holder = makeEl("div", "row row-stack");
  holder.appendChild(makeEl("label", "row-label", "DPI stages"));
  const box = makeEl("div", "stages-box");
  box.appendChild(
    makeEl(
      "p",
      "stage-hint",
      "Preset sensitivities you can cycle through with the device’s DPI button. Select the one to make active; changes apply to the device as you make them.",
    ),
  );

  const stages = makeEl("div", "stages");
  w.stages.forEach((val, i) => {
    const r = makeEl("div", "stage-row");

    const radio = makeEl("input");
    radio.type = "radio";
    radio.name = `active-stage-${dev.usbPid}`;
    radio.checked = w.activeStage === i;
    radio.setAttribute("aria-label", `Make stage ${i + 1} the active sensitivity`);
    radio.setAttribute("data-fkey", `stage-radio-${i}`);
    radio.addEventListener("change", () => {
      w.activeStage = i;
      pushStages(dev.usbPid);
    });

    const idx = makeEl("span", "stage-idx", `Stage ${i + 1}`);

    const num = makeEl("input", "num");
    num.type = "number";
    num.min = String(lo);
    num.max = String(hi);
    num.value = String(val);
    num.setAttribute("aria-label", `Stage ${i + 1} DPI`);
    num.setAttribute("data-fkey", `stage-num-${i}`);
    num.addEventListener("change", () => {
      w.stages[i] = clamp(Math.round(Number(num.value)), lo, hi);
      num.value = String(w.stages[i]);
      pushStages(dev.usbPid);
    });

    const remove = makeEl("button", "btn btn-quiet", "Remove");
    remove.type = "button";
    remove.setAttribute("aria-label", `Remove stage ${i + 1}`);
    remove.disabled = w.stages.length <= MIN_STAGES;
    remove.addEventListener("click", () => {
      w.stages.splice(i, 1);
      if (w.activeStage >= w.stages.length) w.activeStage = w.stages.length - 1;
      pushStages(dev.usbPid);
      renderDetail();
    });

    r.append(radio, idx, num, remove);
    stages.appendChild(r);
  });

  const actions = makeEl("div", "stage-actions");
  const add = makeEl("button", "btn", "Add stage");
  add.type = "button";
  add.disabled = w.stages.length >= MAX_STAGES;
  add.addEventListener("click", () => {
    w.stages.push(w.dpi.x);
    pushStages(dev.usbPid);
    renderDetail();
  });

  actions.append(add);
  box.append(stages, actions);
  holder.appendChild(box);
  return holder;
}

// ---- polling ----

function renderPollingSection(dev: DeviceInfo): HTMLElement {
  const w = getWorking(dev.usbPid);
  const s = section("Polling rate", "How often the device reports movement to your PC.");
  const id = nextId("polling");
  const sel = makeEl("select", "select");
  sel.id = id;
  sel.setAttribute("data-fkey", "polling");
  for (const hz of POLLING_RATES) {
    const opt = makeEl("option", undefined, `${hz} Hz`);
    opt.value = String(hz);
    if (hz === w.pollingHz) opt.selected = true;
    sel.appendChild(opt);
  }
  sel.addEventListener("change", () => {
    w.pollingHz = Number(sel.value);
    pushPolling(dev.usbPid);
    toast(`Polling rate set to ${w.pollingHz} Hz`, "info");
  });
  s.appendChild(row("Rate", sel, id));
  return s;
}

// ---- battery ----

function renderBatterySection(dev: DeviceInfo): HTMLElement {
  const s = section("Battery");
  const readout = makeEl("div", "battery-readout");

  const bar = makeEl("div", "battery-bar");
  bar.setAttribute("role", "img");
  const fill = makeEl("div", "battery-fill");
  fill.id = "battery-fill";
  bar.appendChild(fill);

  const text = makeEl("span", "battery-text");
  text.id = "battery-text";

  readout.append(bar, text);
  s.appendChild(readout);
  applyBatteryToCard(dev.battery, fill, text, bar);
  return s;
}

function applyBatteryToCard(
  battery: Battery | null,
  fill: HTMLElement,
  text: HTMLElement,
  bar?: HTMLElement,
) {
  if (!battery) {
    fill.style.width = "0%";
    fill.className = "battery-fill";
    text.textContent = "No reading yet";
    bar?.setAttribute("aria-label", "Battery level not yet read");
    return;
  }
  const low = battery.levelPercent <= 15 && !battery.charging;
  fill.style.width = `${battery.levelPercent}%`;
  fill.className = `battery-fill${low ? " low" : ""}`;
  text.textContent = `${battery.levelPercent}%${battery.charging ? " · charging" : low ? " · low" : ""}`;
  bar?.setAttribute(
    "aria-label",
    `Battery ${battery.levelPercent} percent${battery.charging ? ", charging" : ""}`,
  );
}

function updateBatteryUi(pid: number) {
  if (selectedPid !== pid) return;
  const fill = document.querySelector<HTMLElement>("#battery-fill");
  const text = document.querySelector<HTMLElement>("#battery-text");
  const dev = devices.find((d) => d.usbPid === pid);
  if (fill && text && dev) applyBatteryToCard(dev.battery, fill, text, fill.parentElement ?? undefined);
  renderSummary(dev ?? null);
}

// ---- keyboard context ----

function renderKeyboardContext(): HTMLElement {
  const s = section("Keyboard");
  s.appendChild(
    makeEl(
      "p",
      "group-note",
      "Sensitivity and DPI controls apply to mice only. This keyboard exposes lighting and polling above.",
    ),
  );
  return s;
}

// ---------------------------------------------------------------------------
// Rendering: summary rail (progressive detail; connected device only)
// ---------------------------------------------------------------------------

function renderSummary(dev: DeviceInfo | null) {
  const rail = el<HTMLElement>("#device-summary");
  if (!dev || !dev.connected) {
    rail.hidden = true;
    rail.innerHTML = "";
    return;
  }
  rail.hidden = false;
  rail.innerHTML = "";
  rail.appendChild(makeEl("h2", "rail-title", "Summary"));

  const view = devicePresentation(dev);
  const list = makeEl("dl", "summary-list");
  for (const key of summarySections(dev)) {
    const term = makeEl("dt", "summary-term");
    const desc = makeEl("dd", "summary-desc");
    if (key === "connection") {
      term.textContent = "Connection";
      desc.textContent = `${view.connectionLabel} · connected`;
    } else if (key === "battery") {
      term.textContent = "Battery";
      desc.textContent = dev.battery
        ? `${dev.battery.levelPercent}%${dev.battery.charging ? " · charging" : ""}`
        : "No reading yet";
    } else if (key === "onboard") {
      term.textContent = "Profile storage";
      const where = dev.capabilities.onboard ? "Saved to device memory" : "Saved in app";
      desc.textContent = activeProfile ? `${activeProfile} · ${where}` : where;
    }
    list.append(term, desc);
  }
  rail.appendChild(list);
}

// ---------------------------------------------------------------------------
// Profiles
// ---------------------------------------------------------------------------

function renderProfilesBar() {
  const sel = el<HTMLSelectElement>("#profile-select");
  sel.innerHTML = "";
  const none = makeEl("option", undefined, "No profile");
  none.value = "";
  sel.appendChild(none);
  for (const name of profileNames) {
    const opt = makeEl("option", undefined, name);
    opt.value = name;
    if (name === activeProfile) opt.selected = true;
    sel.appendChild(opt);
  }
}

async function refreshProfiles() {
  profileNames = (await cmd<string[]>("list_profiles")) ?? [];
  activeProfile = (await cmd<string | null>("get_active_profile")) ?? null;
  renderProfilesBar();
}

function applyProfileToWorking(profile: Profile) {
  for (const [pidStr, settings] of Object.entries(profile.devices)) {
    const pid = Number(pidStr);
    const w = getWorking(pid);
    if (settings.lighting) w.lighting = { ...settings.lighting };
    if (settings.dpi) {
      w.dpi = { ...settings.dpi };
      const dev = devices.find((d) => d.usbPid === pid);
      w.stages = seedStages(settings.dpi.x, dev?.dpiMin ?? 100, dev?.dpiMax ?? 20000);
      w.activeStage = 0;
    }
    if (settings.pollingHz != null) w.pollingHz = settings.pollingHz;
  }
}

async function onApplyProfile() {
  const name = el<HTMLSelectElement>("#profile-select").value;
  if (!name) {
    toast("Pick a profile to apply", "info");
    return;
  }
  const profile = await cmd<Profile>("apply_profile", { name });
  if (!profile) return;
  applyProfileToWorking(profile);
  activeProfile = name;
  renderDetail();
  toast(`Applied “${name}”`, "info");
}

async function onSaveProfile() {
  const name = await promptName("Save profile as", activeProfile ?? "");
  if (name == null) return;
  if (profileNames.includes(name)) {
    const replace = await confirmDialog({
      title: `Replace “${name}”?`,
      body: `A profile named “${name}” already exists. Saving will overwrite it with the current settings.`,
      confirmLabel: "Replace profile",
      cancelLabel: "Cancel",
    });
    if (!replace) return;
  }
  const payload: Record<string, DeviceSettingsPayload> = {};
  for (const dev of devices) {
    const w = working.get(dev.usbPid);
    if (w) payload[String(dev.usbPid)] = workingToSettings(dev, w);
  }
  const ok = await cmd("save_profile", { name, devices: payload });
  if (ok === undefined) return;
  await refreshProfiles();
  el<HTMLSelectElement>("#profile-select").value = name;
  toast(`Saved “${name}”`, "info");
}

async function onDeleteProfile() {
  const name = el<HTMLSelectElement>("#profile-select").value;
  if (!name) {
    toast("Pick a profile to delete", "info");
    return;
  }
  const confirmed = await confirmDialog({
    title: `Delete “${name}”?`,
    body: "This permanently removes the profile. This can’t be undone.",
    confirmLabel: "Delete profile",
    cancelLabel: "Keep profile",
    danger: true,
  });
  if (!confirmed) return;
  const ok = await cmd("delete_profile", { name });
  if (ok === undefined) return;
  if (activeProfile === name) activeProfile = null;
  await refreshProfiles();
  toast(`Deleted “${name}”`, "info");
}

// Minimal inline name prompt (WebView2 doesn't reliably support window.prompt).
function promptName(title: string, initial: string): Promise<string | null> {
  return new Promise((resolve) => {
    const overlay = makeEl("div", "modal-overlay");
    const box = makeEl("div", "modal");
    const h = makeEl("h2", "modal-title", title);
    const input = makeEl("input", "modal-input");
    input.type = "text";
    input.value = initial;
    input.setAttribute("aria-label", title);
    const rowEl = makeEl("div", "modal-actions");
    const cancel = makeEl("button", "btn", "Cancel");
    cancel.type = "button";
    const ok = makeEl("button", "btn btn-accent", "Save");
    ok.type = "button";

    const close = (value: string | null) => {
      document.body.removeChild(overlay);
      resolve(value);
    };
    cancel.addEventListener("click", () => close(null));
    ok.addEventListener("click", () => close(input.value.trim() || null));
    input.addEventListener("keydown", (e) => {
      if (e.key === "Enter") close(input.value.trim() || null);
      if (e.key === "Escape") close(null);
    });

    rowEl.append(cancel, ok);
    box.append(h, input, rowEl);
    overlay.appendChild(box);
    document.body.appendChild(overlay);
    input.focus();
    input.select();
  });
}

// Confirmation dialog for destructive or overwriting actions. Names the action
// and its consequence; buttons say what they do (never Yes/No).
function confirmDialog(opts: {
  title: string;
  body: string;
  confirmLabel: string;
  cancelLabel?: string;
  danger?: boolean;
}): Promise<boolean> {
  return new Promise((resolve) => {
    const overlay = makeEl("div", "modal-overlay");
    const box = makeEl("div", "modal");
    box.setAttribute("role", "dialog");
    box.setAttribute("aria-modal", "true");
    const h = makeEl("h2", "modal-title", opts.title);
    const body = makeEl("p", "modal-body", opts.body);
    const rowEl = makeEl("div", "modal-actions");
    const cancel = makeEl("button", "btn", opts.cancelLabel ?? "Cancel");
    cancel.type = "button";
    const ok = makeEl("button", `btn ${opts.danger ? "btn-danger" : "btn-accent"}`, opts.confirmLabel);
    ok.type = "button";

    const close = (value: boolean) => {
      document.removeEventListener("keydown", onKey, true);
      document.body.removeChild(overlay);
      resolve(value);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        close(false);
      }
    };
    cancel.addEventListener("click", () => close(false));
    ok.addEventListener("click", () => close(true));
    overlay.addEventListener("click", (e) => {
      if (e.target === overlay) close(false); // backdrop click cancels
    });
    document.addEventListener("keydown", onKey, true);

    rowEl.append(cancel, ok);
    box.append(h, body, rowEl);
    overlay.appendChild(box);
    document.body.appendChild(overlay);
    ok.focus();
  });
}

// ---------------------------------------------------------------------------
// Data refresh + events
// ---------------------------------------------------------------------------

async function refreshDevices() {
  devices = (await cmd<DeviceInfo[]>("list_devices")) ?? [];
  if (selectedPid == null) {
    const firstConnected = devices.find((d) => d.connected);
    if (firstConnected) selectedPid = firstConnected.usbPid;
  }
  markReadingIfNeeded(selectedPid);
  renderDeviceList();
  renderDetail();
  const sel = devices.find((d) => d.usbPid === selectedPid);
  if (sel) void ensureLoaded(sel);
}

async function initAutostart() {
  const toggle = el<HTMLInputElement>("#autostart-toggle");
  const enabled = (await cmd<boolean>("autostart_enabled")) ?? false;
  toggle.checked = enabled;
  toggle.addEventListener("change", async () => {
    const ok = await cmd("set_autostart", { enabled: toggle.checked });
    if (ok === undefined) toggle.checked = !toggle.checked; // revert on failure
  });
}

window.addEventListener("DOMContentLoaded", () => {
  // Let users dismiss a toast early instead of waiting out the timeout.
  const toastEl = el<HTMLDivElement>("#toast");
  toastEl.title = "Dismiss";
  toastEl.addEventListener("click", () => {
    window.clearTimeout(toastTimer);
    toastEl.className = "toast";
  });

  el<HTMLButtonElement>("#profile-apply").addEventListener("click", () => void onApplyProfile());
  el<HTMLButtonElement>("#profile-save").addEventListener("click", () => void onSaveProfile());
  el<HTMLButtonElement>("#profile-delete").addEventListener("click", () => void onDeleteProfile());

  void refreshDevices();
  void refreshProfiles();
  void initAutostart();

  void listen<{ pid: number }>("device-connected", () => void refreshDevices());
  void listen<{ pid: number }>("device-disconnected", () => void refreshDevices());
  void listen<{ pid: number; levelPercent: number; charging: boolean }>("battery-updated", (event) => {
    const { pid, levelPercent, charging } = event.payload;
    const dev = devices.find((d) => d.usbPid === pid);
    if (dev) {
      dev.battery = { levelPercent, charging };
      renderDeviceList();
      updateBatteryUi(pid);
    } else {
      void refreshDevices();
    }
  });
});
