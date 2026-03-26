# Desktop Client Framework Research

> Research conducted March 2026. Evaluating Rust-based cross-platform desktop
> frameworks and native options for an Aiki desktop client. Mac is top priority,
> with Linux and Windows support to follow.

## TL;DR

| Framework | Maturity | Rendering | Mac Native Feel | Cross-Platform | License | Best For |
|-----------|----------|-----------|-----------------|----------------|---------|----------|
| **Tauri** | Production (2.x) | System WebView | Good (menus, tray, notifs) | Mac+Linux+Win+Mobile | Apache-2.0/MIT | Web-tech UI + Rust backend |
| **Dioxus** | Pre-1.0 (0.7) | WebView or Blitz native | Limited (DIY platform APIs) | Mac+Linux+Win+Mobile+Web | MIT/Apache-2.0 | All-Rust React-like apps |
| **Iced** | Pre-1.0 (0.14) | wgpu / tiny-skia | Weak (no native menus/tray) | Mac+Linux+Win+Web | MIT | Custom-styled apps |
| **Slint** | Stable (1.15) | FemtoVG/Skia/Software | Moderate (Cupertino style) | Mac+Linux+Win+Embedded+Web | GPL/Royalty-Free/Commercial | Embedded + desktop |
| **GPUI** | Pre-1.0 | Metal/Vulkan/DX (GPU) | Moderate (Mac-first, DIY) | Mac+Linux; Win=alpha | Apache-2.0 | High-perf GPU UIs |
| **SwiftUI** | Mature | Native AppKit/Cocoa | Perfect | Mac only | Proprietary | Mac-only apps |
| **Xilem** | Alpha | Vello (GPU compute) | Weak | Mac+Linux+Win | Apache-2.0 | Future potential |

---

## 1. Tauri

- **Version:** 2.x stable (Oct 2024; actively maintained through March 2026)
- **GitHub:** ~104,700 stars, ~3,500 forks
- **License:** Apache-2.0 / MIT

**Rendering:** Uses the OS's native WebView (WebKit on macOS, WebKitGTK on
Linux, WebView2 on Windows). Frontend is HTML/CSS/JS in that webview; backend
logic is Rust. Apps are typically under 10 MB and idle at ~30-40 MB RAM.

**Cross-platform:** macOS, Linux, Windows — all production-ready. Tauri 2.0
also added iOS and Android (stable but less mature than desktop).

**Performance:** Excellent startup time (under 1 second), very low memory
footprint vs Electron. UI performance is bounded by the system webview — fine
for typical app UIs, not for game-like rendering.

**Mac integration:** Strong. Built-in APIs for:
- Native menu bars (with macOS-specific app menu conventions)
- System tray / status bar icons (template image support)
- Native OS notifications (via `tauri-plugin-notification`)
- Dock icon control (can hide for menu-bar-only apps)
- Window management, file dialogs, etc.

**Community:** Largest Rust GUI community. 17,700+ Discord members, 2,000+
GitHub contributors, extensive plugin ecosystem. Compatible with any JS
frontend framework (React, Vue, Svelte, SolidJS, etc.).

**Notable apps:** Ariadne (Git client), LumenTrack, numerous open-source tools.
Widely adopted for new desktop projects since 2024.

**Pros:**
- Most mature and production-proven Rust desktop framework
- Tiny binaries and low memory usage
- Use any web frontend framework — huge talent pool and UI ecosystem
- Excellent native platform integration (menus, tray, notifications)
- Mobile support (iOS/Android) in 2.0
- Strong security model with fine-grained API permissions
- Very active development and large community

**Cons:**
- UI is in a webview, not truly native widgets
- WebView behavior varies across platforms (especially older Linux distros)
- Requires both Rust and web tech skill sets
- Not suitable for GPU-intensive custom rendering

---

## 2. Dioxus

- **Version:** 0.7.0 (Oct 31, 2025)
- **GitHub:** ~35,500 stars, ~1,600 forks
- **License:** MIT / Apache-2.0

**Rendering:** Hybrid. Desktop defaults to WebView (via `wry`, same as Tauri).
Starting in 0.7, introduced "Dioxus Native" powered by **Blitz**, their own
HTML/CSS renderer. Can choose between webview and native from the CLI. Web
targets compile to WASM.

**Cross-platform:** macOS, Linux, Windows for desktop. iOS/Android in 0.7. Web
via WASM. SSR and LiveView modes also available.

**Performance:** Desktop binaries under 5 MB. Comparable to Tauri in webview
mode. Blitz native renderer still maturing. Subsecond hot-patching in dev.

**Mac integration:** Limited. No built-in APIs for native macOS menu bars,
system tray, or notifications. Would need external Rust crates (`muda` for
menus, `tray-icon` for system tray) or direct platform API calls.

**Community:** Growing rapidly. Backed by a Y Combinator startup (S23 batch)
with full-time engineers. 10M+ crates.io downloads. React-like API familiar to
web developers. Integrates with Tailwind CSS (zero-setup in 0.7).

**Pros:**
- Write everything in Rust with React/JSX-like syntax (RSX)
- Single codebase for web, desktop, mobile, and server
- Subsecond hot-reload
- Choice of renderers (webview or native) in 0.7
- Full-time VC-backed team; rapid development
- Fullstack capabilities (SSR, server functions) built in

**Cons:**
- Pre-1.0 with breaking changes between versions
- Native platform integration not built in — requires manual work
- Blitz native renderer is experimental
- Smaller widget/plugin ecosystem than Tauri
- Fewer production apps to reference

---

## 3. Iced

- **Version:** 0.14 (Dec 7, 2025)
- **GitHub:** ~30,000 stars, ~1,500 forks
- **License:** MIT

**Rendering:** Custom rendering via `wgpu` (GPU-accelerated via
Metal/Vulkan/DX12) with `tiny-skia` (CPU) fallback. Draws its own widgets —
no native OS widgets or webview. Elm-inspired architecture.

**Cross-platform:** macOS, Linux, Windows. Also compiles to WASM for web.

**Performance:** Good GPU-accelerated rendering. Suitable for custom-styled
apps. Binary sizes larger than webview-based approaches due to rendering stack.

**Mac integration:** Weak — the biggest gap for Mac-first development:
- No native menu bar support (community crate `iced_aw` draws menus inside window)
- No built-in system tray
- No native notifications
- `winit` has known macOS-specific window lifecycle bugs
- Accessibility has been an open issue for 4.5+ years

**Community:** Driven significantly by System76's adoption for the **COSMIC
desktop environment** (Pop!_OS). This guarantees continued investment in Linux
desktop features.

**Notable apps:** COSMIC desktop (System76/Pop!_OS), Halloy (IRC client).

**Pros:**
- Pure Rust — no web tech, no JS, no webview
- Elm architecture is clean and type-safe
- Strong GPU-accelerated rendering
- COSMIC adoption guarantees continued development
- 0.14 added reactive rendering, time-travel debugging, hot reloading

**Cons:**
- No native macOS menu bar, system tray, or notifications
- No native widget look on any platform
- Accessibility essentially non-existent
- Pre-1.0 with breaking API changes
- winit macOS-specific bugs

---

## 4. Slint

- **Version:** 1.15.x (Feb 2026) — stable 1.x API
- **GitHub:** ~22,100 stars, ~850 forks
- **License:** GPLv3 / Royalty-Free / Commercial (triple-licensed)

**Rendering:** Custom with multiple backends: FemtoVG (OpenGL ES 2.0), Skia,
or software (CPU-only). Runtime fits under 300 KiB RAM. Uses its own `.slint`
markup language for UI definitions.

**Cross-platform:** macOS, Linux, Windows, WebAssembly, embedded (including bare
metal). Strongest embedded story of any framework here.

**Mac integration:** Moderate.
- **Cupertino style** that mimics macOS look (pure Slint, not native widgets)
- Working on system tray icons and context menus
- Automatic Ctrl-to-Cmd mapping on macOS
- Material 3 style also available

**Licensing details:**
- **Royalty-Free** (free): Requires "AboutSlint" widget + attribution badge
- **GPLv3** (free): Entire app must be GPL
- **Commercial** (paid): No attribution, includes embedded

**Pros:**
- Only framework with a stable 1.x API — no breaking changes
- Declarative `.slint` language enforces clean UI/logic separation
- Excellent tooling (VS Code extension, live preview, Figma integration)
- Professional commercial backing and support
- Multi-language support (Rust, C++, JS, Python)

**Cons:**
- Licensing complexity
- Cupertino style is visual approximation, not native widgets
- Smaller open-source community
- `.slint` DSL is a new language to learn
- winit dependency inherits macOS issues

---

## 5. GPUI (from Zed)

- **Version:** Pre-1.0 (actively developed within Zed monorepo)
- **GitHub:** Zed has ~78,000 stars; GPUI is a crate within it
- **License:** Apache-2.0

**Rendering:** GPU-accelerated hybrid immediate/retained mode. Uses
platform-native GPU APIs: **Metal** on macOS, **Vulkan** on Linux, **DirectX**
on Windows. Renders at 120 FPS. Tailwind-like styling API in Rust.

**Cross-platform:**
- **macOS:** Production-ready (Zed ships on Mac)
- **Linux:** Production-grade (Zed runs on Linux)
- **Windows:** Alpha quality as of early 2026

**Performance:** Highest raw rendering performance of any framework here.
Designed for a code editor rendering text at 120 FPS with complex UI.

**Mac integration:** Good by nature of Zed being Mac-first. Handles Metal
rendering well. Does NOT provide high-level abstractions for menu bars, system
tray, or notifications — Zed handles those through its own platform layer.

**Community:** Growing. Notable third-party projects: gpui-component (60+
widgets by Longbridge), Arbor (agentic coding app), termy (terminal emulator),
Loungy (app launcher).

**Pros:**
- Extreme rendering performance (120 FPS GPU-accelerated)
- Proven in production (powers Zed editor)
- Tailwind-like styling API feels modern
- Growing component ecosystem (gpui-component has 60+ widgets)
- Mac-first DNA
- Apache-2.0 licensed

**Cons:**
- Pre-1.0 with frequent breaking changes
- Sparse documentation — "read the Zed source" is the learning path
- Windows support not production-ready
- Tightly coupled to Zed's needs
- No built-in abstractions for menus, tray, notifications, dialogs
- Steep learning curve

---

## 6. Native Swift / SwiftUI

- **Status:** Apple's official declarative UI framework, mature on all Apple platforms

**Mac integration:** Perfect. SwiftUI IS the native Mac UI framework. Menu bars,
notifications, system tray, Dock, window management, keyboard shortcuts,
accessibility — all first-class.

**Cross-platform:** Mac only. No Linux or Windows support from Apple.
- **SwiftCrossUI** — community project for cross-platform Swift apps. Gained
  Swift.org recognition in Feb 2026. Most mature community option.
- **SwiftOpenUI** — very new (~March 2026), too early to evaluate.

**Pros:**
- Best possible native Mac experience
- First-class Apple ecosystem integration
- Mature, well-documented, huge community
- Accessibility built in

**Cons:**
- Linux and Windows support does not exist from Apple
- Community cross-platform projects are experimental
- Would mean Mac-only client, or separate codebases for Linux/Windows

---

## 7. Xilem (Linebender)

- **Version:** Alpha
- **GitHub:** ~4,900 stars, ~190 forks
- **License:** Apache-2.0

**Rendering:** Custom GPU rendering via **Vello** (high-performance GPU
compute 2D renderer using `wgpu`). Text via Parley/Fontique/Swash.
Accessibility via AccessKit.

**Community:** Small but dedicated. Linebender org. Funded by NLnet grants.
Key Rust GUI community figures involved (original Druid author).

**Pros:**
- Clean modern architecture (Flutter/SwiftUI/Elm hybrid)
- Vello is cutting-edge GPU 2D tech
- AccessKit means accessibility is architecturally prioritized
- Apache-2.0

**Cons:**
- Alpha quality — "lots of things need improvements"
- Expect major breaking changes
- Very small widget set
- No native platform integration
- Not suitable for production yet

---

## Other Notable Frameworks

### egui
~13M crates.io downloads. Immediate-mode UI. Great for tools, debug UIs,
prototypes, and game engine GUIs. Not suited for traditional desktop apps.

### Floem (from Lapce)
~4,100 stars. Reactive signal-based architecture (Leptos-inspired). Powers the
Lapce code editor. Weak accessibility/IME support. Still maturing.

### Makepad
~6,300 stars. GPU-first with shader-based styling. Has its own DSL and studio
IDE. Reached 1.0 in 2025. Unique approach but niche.

---

## Recommendation Analysis

Given Aiki's context (Rust CLI tool, Mac priority, eventual Linux/Windows):

### Tier 1: Strong candidates

**Tauri** — Safest, most production-ready choice. Largest community, best
platform integration, stable API. Trade-off: UI is web tech in a webview.
Given Aiki is a developer tool, a polished web UI (think Linear, Zed's
channel UI) could work well. Two skill sets required (Rust + web frontend).

**GPUI** — Most interesting for a dev-tools company. Proven in Zed (a similar
domain — developer tooling). Extreme performance. Mac-first DNA. However:
pre-1.0 API, sparse docs, Windows is alpha, and no built-in platform
abstractions (menus/tray/notifications). You'd be building on top of what
is effectively Zed's internal framework.

### Tier 2: Worth watching

**Dioxus** — All-Rust with React-like ergonomics and the best "one codebase
everywhere" story. The VC backing and rapid development are encouraging. But
pre-1.0 and limited native Mac integration. Could be the right choice in
6-12 months.

**Slint** — Only stable 1.x API. Professional backing. Good tooling. But the
licensing is complex and the community is smaller.

### Tier 3: Not recommended for this project

**Iced** — Linux-focused (COSMIC). Weak Mac integration. No accessibility.

**SwiftUI** — Perfect Mac experience but no path to Linux/Windows.

**Xilem** — Too early. Alpha quality.

---

## Key Questions for Decision

1. **Is a webview UI acceptable?** If yes, Tauri is the clear winner. If the
   goal is a native-feeling, high-performance UI (like Zed), then GPUI or
   Dioxus are more aligned.

2. **How important is shipping fast vs building for the long term?** Tauri
   ships today. GPUI/Dioxus require more groundwork but could yield a more
   differentiated product.

3. **How much platform integration is needed?** Menu bars, system tray,
   notifications, Dock integration — Tauri has these built in. GPUI/Dioxus
   require building them yourself.

4. **What's the team's frontend experience?** If strong web skills exist, Tauri
   leverages them. If the team is Rust-focused, Dioxus or GPUI avoid the
   JS/HTML/CSS layer entirely.
