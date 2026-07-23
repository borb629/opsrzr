# opsrzr Design System

The durable design contract for opsrzr's desktop settings interface. Machine
tokens live in [`docs/design-tokens.json`](docs/design-tokens.json); this
document explains intent, so future work extends the system instead of
drifting from it.

Grounded in [`PRODUCT.md`](PRODUCT.md): **quietly luxurious, composed, and
precise** — a premium hardware companion, deliberately unlike Razer's
gaming-software visual language.

## Core idea: the soft instrument panel

The device is the subject; the interface is the companion. A stable three-zone
desktop layout keeps the device present while giving configuration a wide,
scannable canvas:

```
┌──────── top bar: brand · profiles · launch-at-login ────────┐
│ device rail │       settings canvas        │ summary rail   │
│ (selection) │  (grouped, capability-gated) │ (progressive)  │
└─────────────┴──────────────────────────────┴────────────────┘
```

The rail is a persistent list (never a stretched-phone drawer). The summary
rail appears only for a selected, connected device — progressive detail, not a
permanent dashboard.

## Colour — earn every colour

Colour marks **status, selection, and real actions**, never atmosphere. Roles:

| Token | Value | Role |
| --- | --- | --- |
| `--bg` | `#12161c` | Deep smoke — app background (not pure black); lifts panels tonally |
| `--surface` | `#20262f` | Graphite — panels, groups |
| `--surface-raised` | `#2a313b` | Raised graphite — controls, selected rows |
| `--divider` | `#313945` | Thin structural dividers (elevation, not shadow) |
| `--text` | `#e9edef` | Porcelain — primary text |
| `--text-muted` | `#a7afb8` | Muted — labels/metadata (AA on every surface) |
| `--accent` | `#5b8cff` | Moonlight blue — selection + real actions |
| `--accent-ink` | `#0a1020` | Ink on accent (AA ≥ 4.5:1) |
| `--active-hardware` | `#8da15e` | Muted olive — live-hardware status (connected, charge) |
| `--warn` | `#d98a5b` | Amber — low battery / error edges |
| `--focus` | `#9eb7ff` | Focus ring |

Blue is reserved for interaction (selected effect chip, primary buttons, focus).
Olive is reserved for hardware truth (the connected dot, the battery fill).
Nothing uses colour decoratively, and no status is communicated by colour
alone — every dot, battery bar, and toast carries a text label.

## Elevation — tonal, not glowing

Hierarchy comes from three tonal steps (`--bg` < `--surface` <
`--surface-raised`) plus 1px dividers. There are **no** drop-shadow stacks,
gradients, or glows used as hierarchy. Groups are bordered containers with thin
internal row dividers — grouped settings, not a field of floating cards.

## Typography

Windows-native variable-font hierarchy, no web fonts:

```
"Segoe UI Variable Text", "Segoe UI Variable", Aptos, "Segoe UI", system-ui, sans-serif
```

- Device title: 22px, `wght` 640, tight tracking (−0.01em).
- Group titles / rail titles: 11–13px, uppercase, 0.05–0.09em tracking, muted.
- Body / controls: 14px, 1.5 line-height.
- Numeric values (DPI, %, battery): `tabular-nums` for stable alignment.

Weight is expressed through the `wght` variation axis so the hierarchy stays
crisp on Segoe UI Variable and degrades gracefully elsewhere.

## Components

- **Device rail item** — dot (olive/muted) + name + `type · status` subline.
  Keyboard-operable (`role=button`, `tabindex=0`, Enter/Space), `aria-pressed`
  reflects selection.
- **Group** — bordered surface container; a head (title + optional description)
  then rows separated by top dividers.
- **Row** — `label | control` grid; collapses to stacked label-over-control
  under 640px. Controls: range slider + tabular badge, numeric input, native
  colour input, select, and a pill **toggle group** for effects (`aria-pressed`).
- **Range slider** — custom track: an accent fill grows to the current value
  (a `--pct` custom property set in `main.ts`) over a raised-graphite remainder;
  a 16px accent thumb scales slightly on hover/press. Replaces `accent-color`.
- **Switch** — the one binary setting (launch-at-login) is a pill toggle: a
  native checkbox restyled with `appearance: none`, accent when checked, knob
  sliding across. Focus uses the shared `:focus-visible` ring.
- **Selected row** — accent *wash* over raised graphite with a bluer edge
  (`--accent-wash` / `--accent-line`); selection is a real accent use, never a
  coloured side-stripe.
- **Battery** — olive fill bar + text (`NN% · charging` / `· low` /
  `No reading yet`); `role=img` with an `aria-label` mirroring the text.
- **Summary rail** — a `<dl>` of connection / battery / onboard-storage,
  driven by `summarySections()`; battery term only appears for battery-capable
  devices.
- **Modal** — inline profile-name prompt (WebView2 lacks reliable
  `window.prompt`); Enter confirms, Escape cancels.

## Navigation & focus

- Single visible focus treatment: `:focus-visible` → 2px `--focus` ring with
  offset. Applied uniformly to items, chips, inputs, and buttons.
- The settings canvas is a labelled `main` landmark; rail and summary are
  labelled `aside`s; the device list references its heading.
- Icon-only / colour controls carry accessible names (colour input, battery
  bar, stage radios, remove buttons).

## Capability gating (the rule the UI obeys)

Rendering is driven entirely by reported type + capabilities via
`devicePresentation()` / `summarySections()` (unit-tested in
`src/device-presentation.test.ts`):

- Battery surfaces **only** when `capabilities.battery`.
- Sensitivity / DPI-stages **only** for `deviceType === "mouse"` with
  `capabilities.dpi`.
- Lighting and polling stay capability-gated.
- **No pairing or key-remapping surface** exists — the backend DTO exposes
  neither, and the UI never fabricates one.

## Accessibility (WCAG 2.2 AA)

- Text meets AA contrast on every surface (porcelain ≈ 12:1; muted ≥ 7:1;
  ink-on-accent ≥ 4.5:1).
- `prefers-reduced-motion: reduce` collapses all transitions/animations.
- `prefers-contrast: more` raises muted text, raised surfaces, and dividers.
- Status is never colour-only. Keyboard focus is always visible. Hit targets
  are comfortable (≥ ~32px).

## Responsive

- **≥ 1024px** — three zones side by side (260px rail · fluid canvas · 280px
  summary).
- **< 1024px** — zones stack; the rail becomes a horizontal, scrollable strip;
  the summary drops below the canvas.
- **< 640px** — rows stack label-over-control and the profile controls wrap;
  **all** controls remain reachable.

## Anti-references (from PRODUCT.md) and how this design avoids each

1. **Razer Synapse's RGB-heavy, crowded gaming aesthetic** → calm graphite
   surfaces, one restrained accent, generous spacing; RGB appears only inside
   the small colour control it configures.
2. **Generic AI/SaaS dashboard patterns** (oversized metric cards, gradient
   decoration, repeated pills, glow-as-hierarchy) → grouped settings with thin
   dividers instead of metric cards; zero gradients; the only pills are a
   functional effect toggle-group; elevation is tonal, never glow.
3. **A phone interface stretched into a desktop window** → a true three-zone
   desktop layout with a persistent rail and a wide canvas; it stacks
   intentionally at breakpoints rather than being a scaled-up column.
4. **Harsh pure-black, low-contrast gray text, colour-only status** → smoke
   `#161b22` (not black), porcelain/muted text verified at AA, and every status
   paired with text.
