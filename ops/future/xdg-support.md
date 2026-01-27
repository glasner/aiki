# XDG Base Directory Support

## Summary

Add optional XDG Base Directory Specification compliance for Linux users who prefer standard paths over `~/.aiki/`.

## Current State

Aiki uses `~/.aiki/` for global state, with `AIKI_HOME` environment variable override for tests and special environments.

## Motivation

The [XDG Base Directory Specification](https://specifications.freedesktop.org/basedir-spec/basedir-spec-latest.html) defines standard locations for application data on Linux:

- `$XDG_CONFIG_HOME` (~/.config/) - configuration files
- `$XDG_DATA_HOME` (~/.local/share/) - data files
- `$XDG_CACHE_HOME` (~/.cache/) - cache files

Benefits:
- Reduces home directory clutter
- Easier to back up config vs data separately
- Follows Linux conventions that power users expect
- Some tools (like `dotfiles` managers) assume XDG compliance

## Proposed Design

### Priority Order

1. `AIKI_HOME` - explicit override (highest priority)
2. XDG paths (if on Linux and XDG vars are set)
3. `~/.aiki/` - default fallback

### Path Mapping

| Data Type | XDG Path | Default Path |
|-----------|----------|--------------|
| Sessions | `$XDG_DATA_HOME/aiki/sessions/` | `~/.aiki/sessions/` |
| Global JJ repo | `$XDG_DATA_HOME/aiki/jj/` | `~/.aiki/jj/` |
| Config (future) | `$XDG_CONFIG_HOME/aiki/` | `~/.aiki/config/` |
| Cache (future) | `$XDG_CACHE_HOME/aiki/` | `~/.aiki/cache/` |

### Platform Behavior

- **Linux**: Check XDG variables, use if set
- **macOS**: Ignore XDG, use `~/.aiki/` (macOS convention)
- **Windows**: Use `%APPDATA%\aiki\` or similar (TBD)

## Implementation Notes

Use the `dirs` crate which handles platform-specific paths:

```rust
use dirs::{data_dir, config_dir, cache_dir};

fn aiki_data_dir() -> PathBuf {
    if let Ok(home) = std::env::var("AIKI_HOME") {
        return PathBuf::from(home);
    }

    // dirs::data_dir() returns:
    // - Linux: $XDG_DATA_HOME or ~/.local/share
    // - macOS: ~/Library/Application Support
    // - Windows: %APPDATA%
    data_dir()
        .map(|p| p.join("aiki"))
        .unwrap_or_else(|| PathBuf::from("~/.aiki"))
}
```

## Open Questions

1. **macOS**: Should we use `~/Library/Application Support/aiki/` (macOS standard) or `~/.aiki/` (simpler)? Leaning toward `~/.aiki/` for discoverability.

2. **Migration**: If someone enables XDG after using `~/.aiki/`, do we migrate? Probably not - let them move files manually or start fresh.

## Priority

Low. The `AIKI_HOME` override covers the main use cases (tests, containers). XDG is a nice-to-have for Linux purists.

## References

- [XDG Base Directory Specification](https://specifications.freedesktop.org/basedir-spec/basedir-spec-latest.html)
- [dirs crate](https://docs.rs/dirs/latest/dirs/)
