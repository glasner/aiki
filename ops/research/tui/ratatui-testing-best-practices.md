# Ratatui Testing Best Practices

## Testing Philosophy

Ratatui's immediate-mode rendering model makes testing straightforward: your view function is a pure projection of state → UI. This means you can test state logic independently from rendering, and test rendering by asserting on buffer contents without a real terminal.

The testing pyramid for a Ratatui app:

1. **State/logic unit tests** — no terminal, no buffer, just feed messages and assert on model
2. **Widget unit tests** — render to a `Buffer`, assert on cells
3. **Integration tests** — render full UI via `TestBackend`, assert on screen output
4. **Snapshot tests** — capture rendered output, compare against saved snapshots
5. **PTY integration tests** — (optional) run in a real pseudo-terminal for terminal-specific behavior

---

## 1. State Logic Tests

Your update/message-handling function should be testable with zero terminal dependencies. This is the fastest, most stable layer of tests.

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pressing_j_moves_selection_down() {
        let mut app = App::new(vec!["alpha", "beta", "gamma"]);
        assert_eq!(app.selected, 0);

        app.update(Message::MoveDown);
        assert_eq!(app.selected, 1);

        app.update(Message::MoveDown);
        assert_eq!(app.selected, 2);
    }

    #[test]
    fn selection_wraps_at_bottom() {
        let mut app = App::new(vec!["alpha", "beta"]);
        app.update(Message::MoveDown);
        app.update(Message::MoveDown);
        assert_eq!(app.selected, 0);
    }

    #[test]
    fn mode_transitions() {
        let mut app = App::new(vec![]);
        assert!(matches!(app.mode, AppMode::Normal));

        app.update(Message::EnterEdit);
        assert!(matches!(app.mode, AppMode::Editing));

        app.update(Message::Escape);
        assert!(matches!(app.mode, AppMode::Normal));
    }
}
```

**Key principle**: if your state logic is tangled with rendering or event handling, refactor it out. The update function should take a message and mutate state — nothing else.

---

## 2. Widget Unit Tests (Buffer-Level)

The officially recommended approach for testing individual widgets. Create a `Buffer`, render the widget into it, and assert on cell contents directly.

```rust
use ratatui::{buffer::Buffer, layout::Rect, widgets::Widget};

#[test]
fn my_widget_renders_title() {
    let widget = MyWidget::new("Hello");
    let area = Rect::new(0, 0, 20, 3);
    let mut buf = Buffer::empty(area);

    widget.render(area, &mut buf);

    // Assert specific cells
    let content: String = (0..5).map(|x| buf[(x, 0)].symbol().to_string()).collect();
    assert_eq!(content, "Hello");
}

#[test]
fn my_widget_respects_area_bounds() {
    let widget = MyWidget::new("This is a long string");
    let area = Rect::new(0, 0, 10, 1); // Only 10 columns wide
    let mut buf = Buffer::empty(area);

    widget.render(area, &mut buf);

    // Should not panic, content should be truncated or wrapped
}
```

**Why Buffer over TestBackend for widgets**: it's faster, has no terminal lifecycle overhead, and isolates the widget completely. TestBackend is intended for full-screen integration tests.

---

## 3. Integration Tests with TestBackend

Use `TestBackend` when testing the full terminal UI — layout, multiple widgets, and state interaction together.

```rust
use ratatui::{backend::TestBackend, Terminal};

#[test]
fn full_ui_renders_correctly() -> Result<(), Box<dyn std::error::Error>> {
    let backend = TestBackend::new(40, 10);
    let mut terminal = Terminal::new(backend)?;
    let mut app = App::new(sample_data());

    terminal.draw(|frame| render(frame, &mut app))?;

    terminal.backend().assert_buffer_lines([
        "╭──────────────────────────────────────╮",
        "│ Item 1                               │",
        "│ Item 2                               │",
        "│ Item 3                               │",
        "│                                      │",
        "│                                      │",
        "│                                      │",
        "│                                      │",
        "│                            1/3       │",
        "╰──────────────────────────────────────╯",
    ]);

    Ok(())
}
```

**Simulating user interaction in integration tests:**

```rust
#[test]
fn scrolling_updates_visible_items() -> Result<(), Box<dyn std::error::Error>> {
    let backend = TestBackend::new(40, 10);
    let mut terminal = Terminal::new(backend)?;
    let mut app = App::new(sample_data());

    // Initial render
    terminal.draw(|frame| render(frame, &mut app))?;

    // Simulate keypress
    app.update(Message::MoveDown);

    // Re-render and assert
    terminal.draw(|frame| render(frame, &mut app))?;

    // Assert that selection moved
    let buf = terminal.backend().buffer().clone();
    // Check that the highlight indicator moved to row 2
    assert_eq!(buf[(1, 2)].symbol(), ">");

    Ok(())
}
```

---

## 4. Snapshot Testing with insta

The recommended approach for UI regression testing. Render to `TestBackend`, convert to string, and snapshot.

### Setup

Add to `Cargo.toml`:

```toml
[dev-dependencies]
insta = "1"
```

### Writing Snapshot Tests

```rust
use insta::assert_snapshot;
use ratatui::{backend::TestBackend, Terminal};

#[test]
fn snapshot_main_screen() {
    let backend = TestBackend::new(80, 20);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut app = App::new(sample_data());

    terminal.draw(|frame| render(frame, &mut app)).unwrap();

    // Convert buffer to string for snapshot
    let view = terminal.backend().to_string();
    assert_snapshot!(view);
}

#[test]
fn snapshot_editing_mode() {
    let backend = TestBackend::new(80, 20);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut app = App::new(sample_data());
    app.update(Message::EnterEdit);

    terminal.draw(|frame| render(frame, &mut app)).unwrap();

    let view = terminal.backend().to_string();
    assert_snapshot!(view);
}
```

### Workflow

```bash
# Run tests — new snapshots are created as "pending"
cargo test

# Review pending snapshots interactively
cargo insta review

# Accept all pending snapshots (use with caution)
cargo insta accept
```

### Practical Guidelines

- **Use a consistent terminal size** (e.g., 80x20) across all snapshot tests for reproducible results.
- **Color assertion is not yet supported** — snapshots capture text content and layout only, not styling.
- **Review snapshots after significant UI changes** to avoid constant CI failures. Treat snapshot updates as part of your PR review process.
- **Name snapshots descriptively** — `insta` auto-names them from the test function, but you can pass explicit names: `assert_snapshot!("main_screen_with_3_items", view)`.

---

## 5. PTY Integration Tests (Optional)

For testing terminal-specific behavior that `TestBackend` cannot cover (Sixel graphics, TTY detection, escape sequences), use `ratatui-testlib`.

```toml
[dev-dependencies]
ratatui-testlib = "0.1"
```

```rust
use ratatui_testlib::{TuiTestHarness, Result};
use portable_pty::CommandBuilder;

#[test]
fn app_starts_and_shows_welcome() -> Result<()> {
    let mut harness = TuiTestHarness::new(80, 24)?;
    let cmd = CommandBuilder::new("./target/debug/my-tui-app");
    harness.spawn(cmd)?;

    // Wait for initial render
    harness.wait_for(|state| {
        state.contents().contains("Welcome")
    })?;

    // Send input
    harness.send_text("hello")?;

    let contents = harness.screen_contents();
    assert!(contents.contains("hello"));

    Ok(())
}
```

**When to use this**: only when you need to test things like terminal resize behavior, graphics protocol output, or end-to-end behavior in a real PTY. For most apps, `TestBackend` + `insta` is sufficient.

---

## Anti-Patterns to Avoid

### Don't reconstruct data on every frame in tests

```rust
// BAD — rebuilds the item list on every draw call
terminal.draw(|frame| {
    let items: Vec<ListItem> = (0..15000)
        .map(|i| ListItem::new(format!("Row {i}")))
        .collect();
    let list = List::new(items);
    frame.render_widget(list, frame.area());
})?;

// GOOD — build once, reference during render
let items: Vec<ListItem> = (0..15000)
    .map(|i| ListItem::new(format!("Row {i}")))
    .collect();
terminal.draw(|frame| {
    let list = List::new(items.clone());
    frame.render_widget(list, frame.area());
})?;
```

### Don't use the deprecated Buffer API

```rust
// BAD — Buffer::get(x, y) is deprecated
let cell = buf.get(5, 3);

// GOOD — use index syntax
let cell = buf[(5, 3)];
```

### Don't test rendering and state logic in the same test

```rust
// BAD — mixing concerns
#[test]
fn test_everything() {
    let mut app = App::new(data());
    app.update(Message::MoveDown);
    let backend = TestBackend::new(80, 20);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|f| render(f, &mut app)).unwrap();
    assert_eq!(app.selected, 1); // state assertion
    terminal.backend().assert_buffer_lines([...]); // render assertion
}

// GOOD — separate tests
#[test]
fn move_down_increments_selection() {
    let mut app = App::new(data());
    app.update(Message::MoveDown);
    assert_eq!(app.selected, 1);
}

#[test]
fn selected_item_is_highlighted() {
    let mut app = App::new(data());
    app.selected = 1;
    let backend = TestBackend::new(80, 20);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|f| render(f, &mut app)).unwrap();
    terminal.backend().assert_buffer_lines([...]);
}
```

### Guard against out-of-bounds in widget tests

```rust
// Prevent panics when testing with small areas
fn render(self, area: Rect, buf: &mut Buffer) {
    let area = area.intersection(*buf.area());
    if area.is_empty() {
        return;
    }
    // ... render logic
}
```

---

## Test Organization

```
src/
├── app.rs          # App state + update logic
├── ui.rs           # Rendering functions
├── widgets/
│   ├── mod.rs
│   └── my_widget.rs
└── main.rs

tests/
├── state_tests.rs       # Pure state/logic tests (no terminal)
├── widget_tests.rs      # Buffer-level widget tests
├── integration_tests.rs # Full UI with TestBackend
└── snapshots/           # insta snapshot files (auto-generated)
```

**Guideline**: state tests should be the majority and the fastest. Widget buffer tests cover rendering correctness. Integration/snapshot tests are the safety net for layout regressions. PTY tests are only for terminal-specific edge cases.
