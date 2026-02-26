// Chunk: docs/chunks/editable_buffer - Main loop + input events + editable buffer
// Chunk: docs/chunks/focus_stack - Focus stack architecture with event propagation
//!
//! Focus target trait definition and focus stack.
//!
//! Focus targets interpret their own input. The buffer's focus target owns chord
//! resolution and produces editor commands. Plugins can provide their own focus
//! targets (for minibuffers, completion menus, etc.) with entirely different
//! input models.
//!
//! This design (from the editor_core_architecture investigation) keeps the core
//! simple: it just delivers events to the active focus target. No global keymap,
//! no event × focus matrix, no god dispatch function.
//!
//! The `FocusStack` enables composable focus targets with event propagation.
//! Events are dispatched top-down: the topmost target gets first crack; if it
//! returns `Handled::No`, the event falls through to the next target. This allows
//! global shortcuts to live at the bottom of the stack while overlays (find bar,
//! selector, dialog) push onto the stack and pop when dismissed.

use crate::context::EditorContext;
use crate::input::{KeyEvent, MouseEvent, ScrollDelta};

/// Result of handling an input event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Handled {
    /// The event was handled by this focus target
    Yes,
    /// The event was not handled (should propagate or be ignored)
    No,
}

/// Identifies what type of focus target is at the top of the stack.
///
/// This enum is used by the renderer to determine which overlay to render.
/// It is intentionally separate from the FocusTarget trait to allow the
/// renderer to make decisions based on focus type without knowing the
/// implementation details of each focus target.
// Chunk: docs/chunks/focus_stack - Focus layer type for rendering decisions
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FocusLayer {
    /// Global shortcut target (bottom of stack, always present)
    GlobalShortcuts,
    /// Normal buffer editing mode
    #[default]
    Buffer,
    /// Selector overlay is active (file picker, command palette, etc.)
    Selector,
    /// Find-in-file strip is active
    FindInFile,
    /// Confirm dialog is active (e.g., abandon unsaved changes?)
    ConfirmDialog,
}

/// A focus target that interprets input events.
///
/// Each focus target interprets its own input. The buffer's focus target
/// handles standard editing commands. Plugin-provided focus targets
/// (minibuffer, completion menu, file picker) handle input with their own logic.
///
/// The core defines this trait; implementations are either built-in (buffer editing)
/// or provided by plugins.
///
/// Focus targets also report their `FocusLayer` type, which is used by the
/// renderer to determine which overlay to render.
// Chunk: docs/chunks/focus_stack - Focus target trait with layer identification
pub trait FocusTarget {
    /// Returns the focus layer type for this target.
    ///
    /// Used by the renderer to determine which overlay to render based on
    /// the top of the focus stack.
    fn layer(&self) -> FocusLayer;
    /// Handle a keyboard event.
    ///
    /// The focus target should:
    /// 1. Interpret the key event (resolve any chords if applicable)
    /// 2. Execute the resulting command by mutating state through `ctx`
    /// 3. Return `Handled::Yes` if the event was consumed
    ///
    /// Mutations through `ctx` automatically accumulate dirty regions.
    fn handle_key(&mut self, event: KeyEvent, ctx: &mut EditorContext) -> Handled;

    // Chunk: docs/chunks/viewport_scrolling - Scroll event handling
    /// Handle a scroll event (trackpad or mouse wheel).
    ///
    /// The focus target should adjust the viewport based on the scroll delta.
    fn handle_scroll(&mut self, delta: ScrollDelta, ctx: &mut EditorContext);

    /// Handle a mouse event (click, drag).
    ///
    /// The focus target should handle cursor placement, selection, etc.
    fn handle_mouse(&mut self, event: MouseEvent, ctx: &mut EditorContext);
}

// =============================================================================
// FocusStack - Ordered collection of focus targets with event propagation
// Chunk: docs/chunks/focus_stack - FocusStack implementation
// =============================================================================

/// An ordered stack of focus targets with top-down event propagation.
///
/// The stack is ordered from bottom (index 0) to top (last element). Events
/// are dispatched top-down: the topmost target gets first crack at handling
/// an event. If it returns `Handled::No`, the event propagates to the next
/// target down the stack.
///
/// # Stack Structure
///
/// A typical stack looks like:
/// ```text
/// Index 0 (bottom): GlobalShortcutTarget - handles Cmd+Q, Cmd+S, etc.
/// Index 1:          BufferFocusTarget - handles buffer editing
/// Index 2 (top):    [optional overlay] - find bar, selector, or dialog
/// ```
///
/// # Event Propagation
///
/// When `dispatch_key` is called:
/// 1. The topmost target's `handle_key` is called
/// 2. If it returns `Handled::Yes`, propagation stops
/// 3. If it returns `Handled::No`, the event propagates to the next target
/// 4. This continues until a target handles the event or the stack is exhausted
///
/// This design allows overlays to handle their specific keys while letting
/// unhandled events (like Cmd+Q for quit) fall through to lower targets.
pub struct FocusStack {
    /// The stack of focus targets, ordered bottom (index 0) to top (last element).
    targets: Vec<Box<dyn FocusTarget>>,
}

impl Default for FocusStack {
    fn default() -> Self {
        Self::new()
    }
}

impl FocusStack {
    /// Creates a new empty focus stack.
    ///
    /// After creation, you should push at least the global shortcuts target
    /// and the buffer target to have a functional editor.
    pub fn new() -> Self {
        Self {
            targets: Vec::new(),
        }
    }

    /// Returns the number of targets in the stack.
    pub fn len(&self) -> usize {
        self.targets.len()
    }

    /// Returns true if the stack is empty.
    pub fn is_empty(&self) -> bool {
        self.targets.is_empty()
    }

    /// Pushes a focus target onto the top of the stack.
    ///
    /// The new target will be the first to receive events during dispatch.
    pub fn push(&mut self, target: Box<dyn FocusTarget>) {
        self.targets.push(target);
    }

    /// Pops the topmost focus target from the stack.
    ///
    /// Returns `None` if the stack is empty.
    ///
    /// # Note
    ///
    /// Be careful not to pop the global shortcuts or buffer targets,
    /// as this would break the editor's basic functionality.
    pub fn pop(&mut self) -> Option<Box<dyn FocusTarget>> {
        self.targets.pop()
    }

    /// Returns a mutable reference to the topmost focus target.
    ///
    /// Returns `None` if the stack is empty.
    pub fn top_mut(&mut self) -> Option<&mut Box<dyn FocusTarget>> {
        self.targets.last_mut()
    }

    /// Returns a reference to the topmost focus target.
    ///
    /// Returns `None` if the stack is empty.
    pub fn top(&self) -> Option<&Box<dyn FocusTarget>> {
        self.targets.last()
    }

    /// Returns the focus layer type of the topmost target.
    ///
    /// This is used by the renderer to determine which overlay to render.
    /// Returns `FocusLayer::Buffer` if the stack is empty (default).
    pub fn top_layer(&self) -> FocusLayer {
        self.targets
            .last()
            .map(|t| t.layer())
            .unwrap_or(FocusLayer::Buffer)
    }

    /// Dispatches a key event top-down through the stack.
    ///
    /// Starting from the topmost target, each target's `handle_key` is called.
    /// If a target returns `Handled::Yes`, propagation stops and `Handled::Yes`
    /// is returned. If all targets return `Handled::No`, `Handled::No` is returned.
    ///
    /// This allows overlays to handle their specific keys while letting
    /// unhandled events (like Cmd+Q) fall through to lower targets.
    pub fn dispatch_key(&mut self, event: KeyEvent, ctx: &mut EditorContext) -> Handled {
        // Dispatch top-down (reverse iteration)
        for target in self.targets.iter_mut().rev() {
            if target.handle_key(event.clone(), ctx) == Handled::Yes {
                return Handled::Yes;
            }
        }
        Handled::No
    }

    /// Dispatches a scroll event top-down through the stack.
    ///
    /// Unlike key events, scroll events are typically only handled by the
    /// topmost target (the buffer or an overlay). We still propagate
    /// top-down for consistency, but in practice the first non-no-op
    /// handler will be the one that takes effect.
    pub fn dispatch_scroll(&mut self, delta: ScrollDelta, ctx: &mut EditorContext) {
        // Dispatch top-down (reverse iteration)
        for target in self.targets.iter_mut().rev() {
            target.handle_scroll(delta, ctx);
            // Scroll events are typically handled by the topmost interested target
            // For now, just call the topmost one and return
            // TODO: Consider adding Handled return to handle_scroll for propagation control
            break;
        }
    }

    /// Dispatches a mouse event top-down through the stack.
    ///
    /// Like scroll events, mouse events are typically only handled by the
    /// topmost target. We call only the topmost target for now.
    pub fn dispatch_mouse(&mut self, event: MouseEvent, ctx: &mut EditorContext) {
        // Dispatch to topmost target only
        if let Some(target) = self.targets.last_mut() {
            target.handle_mouse(event, ctx);
        }
    }
}

// =============================================================================
// Tests for FocusStack
// Chunk: docs/chunks/focus_stack - FocusStack unit tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dirty_region::DirtyRegion;
    use crate::font::FontMetrics;
    use crate::viewport::Viewport;
    use lite_edit_buffer::TextBuffer;

    /// A mock focus target that always handles events.
    struct AlwaysHandles;

    impl FocusTarget for AlwaysHandles {
        fn layer(&self) -> FocusLayer {
            FocusLayer::Buffer
        }

        fn handle_key(&mut self, _event: KeyEvent, _ctx: &mut EditorContext) -> Handled {
            Handled::Yes
        }

        fn handle_scroll(&mut self, _delta: ScrollDelta, _ctx: &mut EditorContext) {}

        fn handle_mouse(&mut self, _event: MouseEvent, _ctx: &mut EditorContext) {}
    }

    /// A mock focus target that never handles events.
    struct NeverHandles;

    impl FocusTarget for NeverHandles {
        fn layer(&self) -> FocusLayer {
            FocusLayer::GlobalShortcuts
        }

        fn handle_key(&mut self, _event: KeyEvent, _ctx: &mut EditorContext) -> Handled {
            Handled::No
        }

        fn handle_scroll(&mut self, _delta: ScrollDelta, _ctx: &mut EditorContext) {}

        fn handle_mouse(&mut self, _event: MouseEvent, _ctx: &mut EditorContext) {}
    }

    /// A mock focus target that tracks whether it was called.
    struct TracksCall {
        called: bool,
        layer: FocusLayer,
        handles: bool,
    }

    impl TracksCall {
        fn new(layer: FocusLayer, handles: bool) -> Self {
            Self {
                called: false,
                layer,
                handles,
            }
        }
    }

    impl FocusTarget for TracksCall {
        fn layer(&self) -> FocusLayer {
            self.layer
        }

        fn handle_key(&mut self, _event: KeyEvent, _ctx: &mut EditorContext) -> Handled {
            self.called = true;
            if self.handles {
                Handled::Yes
            } else {
                Handled::No
            }
        }

        fn handle_scroll(&mut self, _delta: ScrollDelta, _ctx: &mut EditorContext) {}

        fn handle_mouse(&mut self, _event: MouseEvent, _ctx: &mut EditorContext) {}
    }

    fn make_test_context<'a>(
        buffer: &'a mut TextBuffer,
        viewport: &'a mut Viewport,
        dirty_region: &'a mut DirtyRegion,
    ) -> EditorContext<'a> {
        let metrics = FontMetrics {
            advance_width: 8.0,
            line_height: 16.0,
            ascent: 12.0,
            descent: 4.0,
            leading: 0.0,
            point_size: 14.0,
        };
        EditorContext::new(buffer, viewport, dirty_region, metrics, 400.0, 600.0)
    }

    fn make_key_event() -> KeyEvent {
        use crate::input::{Key, Modifiers};
        KeyEvent::new(Key::Char('a'), Modifiers::default())
    }

    #[test]
    fn new_stack_is_empty() {
        let stack = FocusStack::new();
        assert!(stack.is_empty());
        assert_eq!(stack.len(), 0);
    }

    #[test]
    fn push_adds_to_top() {
        let mut stack = FocusStack::new();
        stack.push(Box::new(NeverHandles));
        assert_eq!(stack.len(), 1);
        assert!(!stack.is_empty());

        stack.push(Box::new(AlwaysHandles));
        assert_eq!(stack.len(), 2);

        // Top should be the buffer layer (AlwaysHandles)
        assert_eq!(stack.top_layer(), FocusLayer::Buffer);
    }

    #[test]
    fn pop_returns_top() {
        let mut stack = FocusStack::new();
        stack.push(Box::new(NeverHandles));
        stack.push(Box::new(AlwaysHandles));

        // Pop should return the last pushed
        let popped = stack.pop();
        assert!(popped.is_some());
        assert_eq!(stack.len(), 1);

        // Top should now be GlobalShortcuts (NeverHandles)
        assert_eq!(stack.top_layer(), FocusLayer::GlobalShortcuts);

        // Pop again
        let popped = stack.pop();
        assert!(popped.is_some());
        assert!(stack.is_empty());

        // Pop empty stack
        let popped = stack.pop();
        assert!(popped.is_none());
    }

    #[test]
    fn dispatch_key_top_handles_stops() {
        let mut stack = FocusStack::new();
        stack.push(Box::new(NeverHandles));
        stack.push(Box::new(AlwaysHandles));

        let mut buffer = TextBuffer::new();
        let mut viewport = Viewport::new(16.0);
        let mut dirty = DirtyRegion::None;
        let mut ctx = make_test_context(&mut buffer, &mut viewport, &mut dirty);

        let result = stack.dispatch_key(make_key_event(), &mut ctx);
        assert_eq!(result, Handled::Yes);
    }

    #[test]
    fn dispatch_key_top_unhandled_falls_through() {
        let mut stack = FocusStack::new();
        stack.push(Box::new(AlwaysHandles)); // Bottom handles
        stack.push(Box::new(NeverHandles)); // Top doesn't handle

        let mut buffer = TextBuffer::new();
        let mut viewport = Viewport::new(16.0);
        let mut dirty = DirtyRegion::None;
        let mut ctx = make_test_context(&mut buffer, &mut viewport, &mut dirty);

        // Top doesn't handle, falls through to bottom which handles
        let result = stack.dispatch_key(make_key_event(), &mut ctx);
        assert_eq!(result, Handled::Yes);
    }

    #[test]
    fn dispatch_key_empty_stack_returns_no() {
        let mut stack = FocusStack::new();

        let mut buffer = TextBuffer::new();
        let mut viewport = Viewport::new(16.0);
        let mut dirty = DirtyRegion::None;
        let mut ctx = make_test_context(&mut buffer, &mut viewport, &mut dirty);

        let result = stack.dispatch_key(make_key_event(), &mut ctx);
        assert_eq!(result, Handled::No);
    }

    #[test]
    fn top_layer_returns_default_for_empty_stack() {
        let stack = FocusStack::new();
        assert_eq!(stack.top_layer(), FocusLayer::Buffer);
    }
}
