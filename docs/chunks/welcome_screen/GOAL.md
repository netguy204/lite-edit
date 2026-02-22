---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
- crates/editor/src/welcome_screen.rs
- crates/editor/src/renderer.rs
- crates/editor/src/workspace.rs
- crates/editor/src/lib.rs
code_references:
  - ref: crates/editor/src/welcome_screen.rs
    implements: "Welcome screen module with ASCII logo, hotkey definitions, geometry calculation, and glyph buffer"
  - ref: crates/editor/src/welcome_screen.rs#WelcomeScreenGeometry
    implements: "Computed geometry struct for centering welcome content in viewport"
  - ref: crates/editor/src/welcome_screen.rs#calculate_welcome_geometry
    implements: "Calculates centered positioning for welcome screen content"
  - ref: crates/editor/src/welcome_screen.rs#WelcomeScreenGlyphBuffer
    implements: "Metal vertex/index buffer management for rendering welcome screen glyphs"
  - ref: crates/editor/src/welcome_screen.rs#WelcomeScreenGlyphBuffer::update
    implements: "Generates glyph quads for logo, title, and hotkey table with colored text"
  - ref: crates/editor/src/workspace.rs#Editor::should_show_welcome_screen
    implements: "Visibility check: show welcome screen when active File tab has empty buffer"
  - ref: crates/editor/src/renderer.rs#Renderer::draw_welcome_screen
    implements: "Renders welcome screen content using Metal glyph pipeline"
narrative: null
investigation: null
subsystems: []
friction_entries: []
bug_type: null
depends_on: []
created_after:
- terminal_cmd_backspace
- terminal_paste_render
- terminal_viewport_init
---

# Chunk Goal

## Minor Goal

Display a Vim-style welcome/intro screen on empty tabs (including the initial tab when the editor starts). The welcome screen shows:

1. **A feather ASCII art logo** — a multi-line colored ASCII art feather that serves as the lite-edit brand mark, rendered in Catppuccin Mocha accent colors (lavender, mauve, blue gradients).

2. **Editor name and tagline** — "lite-edit" rendered prominently below the logo, with a short tagline.

3. **Hotkey reference table** — a categorized list of the editor's key shortcuts, styled with dimmed labels and bright key combos. Categories include:
   - File operations (Cmd+S save, Cmd+P file picker, Cmd+N new workspace, Cmd+T new tab)
   - Navigation (Cmd+] / Cmd+[ workspace cycling, Cmd+Shift+] / Cmd+Shift+[ tab cycling, Cmd+1..9 workspace switch)
   - Editing (Cmd+F find, Cmd+W close tab, Cmd+Shift+W close workspace)
   - Terminal (Cmd+Shift+T new terminal tab)
   - Application (Cmd+Q quit)

The welcome screen content is **centered both horizontally and vertically** within the buffer viewport area. It replaces the empty buffer content and disappears as soon as the user starts typing or opens a file into that tab.

This is a polish/UX feature that makes the editor feel complete and professional on first launch, similar to Vim's intro screen or VS Code's welcome tab.

## Success Criteria

- When the editor launches with no file arguments, the initial empty tab displays the welcome screen instead of a blank buffer
- The welcome screen shows a multi-line feather ASCII art logo using colored text (Catppuccin Mocha palette accents)
- The welcome screen lists all current hotkeys in a readable, categorized format
- The welcome screen is vertically and horizontally centered in the content viewport
- Typing any character or opening a file into the tab dismisses the welcome screen and transitions to normal buffer editing
- Creating a new empty tab (Cmd+T) also shows the welcome screen
- The welcome screen renders using the existing glyph/text rendering pipeline (no special shader work needed — it's just styled text in the buffer area)