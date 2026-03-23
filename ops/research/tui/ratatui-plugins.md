# Ratatui Ecosystem Plugins & Community Crates

**Date**: 2026-03-23
**Context**: Evaluated for the TUI rewrite (Elm/TEA, `Viewport::Inline`, `Vec<Line>` output, ratatui 0.30)

---

## Adopted

| Crate | Version | Recent DL | Why |
|-------|---------|-----------|-----|
| **ratatui-macros** | 0.7.0 | 1.6M | Ergonomic `line![]`, `span![]`, `text![]`, `row![]`, `constraint![]` macros. Lives in the ratatui mono-repo. Since our architecture outputs `Vec<Line>`, this directly reduces boilerplate in every view function and component. Zero adoption risk. |
| **ansi-to-tui** | 8.0.1 | 892K | Converts ANSI-escaped strings → ratatui `Text`. Now maintained under the ratatui GitHub org. Needed when rendering subprocess output (agent logs, build output) that contains ANSI color codes. |

---

## Add When Needed

| Crate | Version | Recent DL | Stars | When to add |
|-------|---------|-----------|-------|-------------|
| **tui-markdown** | 0.3.7 | 77K | 93 | When rendering markdown content (task descriptions, LLM output). By joshka (ratatui maintainer), targets `^0.30.0`. Has optional `syntect` syntax highlighting and `ansi-to-tui` integration. |
| **tui-input** | 0.15.0 | 191K | 184 | When adding interactive text input fields (filter, search, prompts). Mature since 2021, supports crossterm and termion, optional ratatui `^0.30.0` support. |

---

## Evaluated & Skipped

### Not relevant to our architecture

| Crate | Version | Recent DL | Stars | Why skipped |
|-------|---------|-----------|-------|-------------|
| **tui-scrollview** | 0.6.2 | 119K | 173 | Virtual scrollable viewport widget. Not needed — `Viewport::Inline` lets the terminal handle scrolling. Would only matter for fixed-height regions with content overflow. |
| **tui-popup** | 0.7.2 | 24K | 173 | Popup/modal dialog widget. Requires alternate screen / full-screen context. Incompatible with `Viewport::Inline`. |
| **tachyonfx** | 0.25.0 | 32K | 1,175 | Shader-like visual effects and animations. Impressive but irrelevant for a line-based inline TUI. Effects need frame-by-frame rendering which conflicts with our model. |
| **ratatui-image** | — | — | — | Terminal image rendering (Sixel/Kitty/iTerm2). Not needed — we removed PNG rendering in the rewrite. |

### Wrong abstraction level

| Crate | Version | Recent DL | Stars | Why skipped |
|-------|---------|-----------|-------|-------------|
| **terminput** | 0.5.13 | 19K | 17 | Backend-agnostic input event abstraction. Only useful for multiple terminal backends. We just use crossterm. Low adoption. |
| **rat-salsa** | 4.0.3 | 929 | 56 | Parallel ecosystem with its own event loops, focus handling, and widget state management. Very opinionated, low adoption. Would fight our Elm architecture. |
| **rat-widget** | 3.2.1 | 2.8K | — | Part of the rat-salsa ecosystem. Comprehensive data-input widgets but tied to rat-salsa's custom event model. |

### TEA frameworks — too immature or too opinionated

| Crate | Version | Recent DL | Stars | Why skipped |
|-------|---------|-----------|-------|-------------|
| **envision** | 0.7.0 | 234 | 0 | TEA framework with headless testing. Targets `^0.29` (not 0.30), 56K lines for v0.7.0. Too immature and too large. The headless testing angle is interesting but we already have insta snapshots. Worth watching. |
| **tuirealm** | 3.3.0 | 30K | 903 | Most established framework. React-inspired with its own component model, message passing, mount/unmount lifecycle. Would conflict with our Elm architecture. Unclear on ratatui 0.30 compat. |
| **tears** | 0.8.0 | 162 | 7 | TEA framework targeting `^0.30`. Only 162 downloads. Too immature. |
| **ratatui-elm** | 1.2.1 | 73 | 29 | Simple Elm architecture wrapper. Only 73 downloads. Too immature. |
| **ftui-runtime** | 0.2.1 | 3.5K | 221 | FrankenTUI — not built on ratatui at all, its own rendering stack. Despite "Elm-style" marketing, completely different ecosystem. |

### Other notable crates (not needed now)

| Crate | Version | Recent DL | Stars | Notes |
|-------|---------|-----------|-------|-------|
| **tui-textarea** | 0.7.0 | 359K | — | Multi-line text editor widget. Likely targets ~0.29. Would only need if adding multi-line editing. |
| **ratatui-textarea** | 0.8.0 | 9.8K | — | Official ratatui fork of tui-textarea. Likely targets 0.30. Same caveat. |
| **tui-widget-list** | 0.15.0 | 27K | — | Heterogeneous widget list (each row can be different). Could be useful for subtask tables if built-in `Table`/`List` feels constraining, but probably not needed. |
| **tui-scrollbar** | 0.2.2 | 573K | — | Fractional thumb scrollbar. Not needed for inline rendering. |
| **tui-term** | 0.3.2 | 224K | — | Pseudoterminal widget. Not needed. |
| **tui-prompts** | 0.6.1 | 30K | — | Interactive prompts (part of tui-widgets). Not needed — we don't have interactive prompts in the TUI. |
| **edtui** | — | — | — | Vim-inspired editor widget. Not relevant. |
| **ratzilla** | — | — | — | Ratatui → WebAssembly for browser rendering. Not relevant. |

---

## Bottom Line

Our lightweight Elm loop + `ratatui-macros` + `ansi-to-tui` is the right stack. The TEA frameworks are all either too immature (<500 downloads) or too opinionated (fight our architecture). The ecosystem is strongest at the library/crate level, not the framework level — which aligns with ratatui's own vision ("a library, not a framework").
