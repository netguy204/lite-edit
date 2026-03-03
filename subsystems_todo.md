# Subsystems To Document

This file tracks architectural subsystems that have been identified but not yet documented. As work progresses, these should be documented using `ve subsystem discover`.

## Identified Subsystems

### layout
**Status**: Not started

**Scope**:
- `Viewport` - Buffer-to-screen coordinate mapping, visible line calculation, scroll offset management
- `WrapLayout` - Soft line wrapping calculations, coordinate mapping between buffer columns and screen positions
- `Font` / font metrics - Font loading, glyph dimensions, line height, ascent/descent calculations

**Relationship to renderer**: The renderer consumes layout subsystem output (screen coordinates, visible ranges) but does not own any of this logic.

---

### theming
**Status**: Future (not yet implemented)

**Scope**:
- Color constants (BACKGROUND_COLOR, TEXT_COLOR, SELECTION_COLOR, etc.)
- Theme definitions and color palettes
- Color resolution for styled content (named colors, RGB colors, palettes)

**Current state**: Colors are currently hardcoded constants in renderer.rs. Future chunks will extract this into a dedicated theming subsystem.

**Relationship to renderer**: The renderer currently owns color constants but should consume them from a theming subsystem.

---

### platform
**Status**: Future (not yet implemented)

**Scope**:
- `MetalView` - macOS-specific window and Metal surface management
- Platform abstraction layer for macOS (Cocoa/AppKit integration)
- Surface lifecycle, resize handling, drawable management

**Current state**: `MetalView` is currently concrete macOS-only code. Future work should abstract platform-specific rendering surface management behind a platform subsystem.

**Relationship to renderer**: The renderer uses `MetalView` to obtain drawables and device references but does not own the surface management logic.

---

## Documented Subsystems

These subsystems have been documented in `docs/subsystems/`:

- [renderer](docs/subsystems/renderer/OVERVIEW.md) - DOCUMENTED
- [viewport_scroll](docs/subsystems/viewport_scroll/OVERVIEW.md)