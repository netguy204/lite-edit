---
status: ACTIVE
ticket: null
parent_chunk: null
code_paths:
- crates/editor/src/editor_state.rs
- crates/editor/src/main.rs
code_references:
  - ref: crates/editor/src/editor_state.rs#EditorState::should_quit
    implements: "Quit flag field set by Cmd+Q"
  - ref: crates/editor/src/editor_state.rs#EditorState::handle_key
    implements: "Intercepts Cmd+Q before delegating to focus target"
  - ref: crates/editor/src/main.rs#EditorController::handle_key
    implements: "Checks quit flag and triggers app termination"
  - ref: crates/editor/src/main.rs#EditorController::terminate_app
    implements: "Calls NSApplication::terminate for clean macOS shutdown"
narrative: null
investigation: null
subsystems: []
friction_entries: []
bug_type: null
depends_on: []
created_after:
- kill_line
- line_nav_keybindings
---

# Cmd+Q Quit

## Minor Goal

Add Cmd+Q to cleanly close the editor. This is the standard macOS quit shortcut. Currently there is no keyboard-driven way to exit the application. Pressing Cmd+Q should terminate the app by calling `NSApplication::terminate`, matching platform conventions.

## Success Criteria

- **Key binding**: Map `Key::Char('q')` with `mods.command && !mods.control` to a quit action in `resolve_command` (or handle it before command resolution, since quitting is an app-level concern rather than a buffer command).

- **App termination**: Pressing Cmd+Q calls `NSApplication::terminate:` (or equivalent) to cleanly shut down the macOS application, matching standard platform behavior.

- **Integration with focus system**: The quit action may need to propagate differently than buffer commands. Options include:
  1. Adding a `Quit` variant to `Handled` (e.g., `Handled::Quit`)
  2. Adding a `Quit` variant to `Command` and having `execute_command` set a flag on `EditorContext`
  3. Handling Cmd+Q at the `AppState` level before dispatching to focus targets
  
  The planning phase should choose the approach that best fits the architecture.

- **Clean shutdown**: No resource leaks — the app exits the same way it would if the window close button were pressed.

- **Unit test**: Verify that the Cmd+Q key event is recognized and produces the quit action (the actual `NSApplication::terminate` call cannot be tested in unit tests, but command resolution can be).