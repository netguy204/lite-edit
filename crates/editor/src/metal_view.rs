// Chunk: docs/chunks/metal_surface - macOS window + Metal surface foundation
// Chunk: docs/chunks/editable_buffer - Main loop + input events + editable buffer
// Chunk: docs/chunks/mouse_click_cursor - Mouse click cursor positioning
// Chunk: docs/chunks/viewport_scrolling - Scroll event handling
// Chunk: docs/chunks/pty_wakeup_reentrant - EventSender-based event delivery
// Chunk: docs/chunks/unicode_ime_input - NSTextInputClient for IME support
//!
//! Metal-backed NSView implementation
//!
//! This module provides a custom NSView subclass that uses a CAMetalLayer
//! as its backing layer, enabling GPU-accelerated Metal rendering.
//!
//! The view also handles keyboard and mouse input, converting NSEvent events
//! to our Rust-native KeyEvent and MouseEvent types.
//!
//! ## Event Delivery
//!
//! Events can be delivered in two ways:
//! 1. **EventSender** (preferred): Events are sent through an `mpsc` channel
//!    to the drain loop, eliminating `Rc<RefCell<>>` borrow conflicts.
//! 2. **Closures** (legacy): Direct callback closures, kept for backward
//!    compatibility during the transition.
//!
//! ## IME Support
//!
//! The view implements the NSTextInputClient protocol to support Input Method
//! Editors (IME) for CJK languages and other complex input methods. Key events
//! are routed through `interpretKeyEvents:` which invokes the macOS text input
//! system. This enables proper handling of:
//! - Compose sequences (dead keys)
//! - IME composition (marked text)
//! - Unicode hex input (Ctrl+Shift+U sequences)

use std::cell::{Cell, RefCell};

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
// Chunk: docs/chunks/dragdrop_file_paste - ClassType for NSURL::class()
use objc2::{define_class, msg_send, ClassType, DefinedClass, MainThreadOnly};
// Chunk: docs/chunks/dragdrop_file_paste - NSDragOperation and NSDraggingInfo for drag-drop support
// Chunk: docs/chunks/input_keystroke_regression - NSTextInputClient protocol conformance
use objc2_app_kit::{
    NSCursor, NSDragOperation, NSDraggingInfo, NSEvent, NSEventModifierFlags,
    NSPasteboardTypeFileURL, NSTextInputClient, NSView,
};
use objc2_foundation::{MainThreadMarker, NSArray, NSObjectProtocol, NSRect, NSSize, NSURL};
use objc2_metal::MTLDevice;
use objc2_quartz_core::{CALayer, CAMetalLayer};

use crate::event_channel::EventSender;
use crate::input::{Key, KeyEvent, MarkedTextEvent, Modifiers, MouseEvent, MouseEventKind, ScrollDelta, TextInputEvent};

// CGFloat is a type alias for f64 on 64-bit systems
type CGFloat = f64;

// =============================================================================
// MetalView
// =============================================================================

/// Type alias for key event handler callback
pub type KeyHandler = Box<dyn Fn(KeyEvent)>;

/// Type alias for mouse event handler callback
pub type MouseHandler = Box<dyn Fn(MouseEvent)>;

/// Type alias for scroll event handler callback
pub type ScrollHandler = Box<dyn Fn(ScrollDelta)>;

// =============================================================================
// Cursor Regions (Chunk: docs/chunks/cursor_pointer_ui_hints)
// =============================================================================

/// A rectangular region in points (view coordinates) with an associated cursor type.
///
/// Coordinates are in the NSView coordinate system (origin at bottom-left).
/// The x, y values are in points (not pixels), matching the coordinate system
/// used by `addCursorRect:cursor:`.
#[derive(Debug, Clone, Copy)]
pub struct CursorRect {
    /// X coordinate of the rect origin (left edge)
    pub x: f64,
    /// Y coordinate of the rect origin (bottom edge in NSView coords)
    pub y: f64,
    /// Width of the rect
    pub width: f64,
    /// Height of the rect
    pub height: f64,
}

impl CursorRect {
    /// Creates a new cursor rect from coordinates in points.
    pub fn new(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self { x, y, width, height }
    }

    /// Converts this CursorRect to an NSRect for use with addCursorRect.
    fn to_ns_rect(&self) -> NSRect {
        NSRect::new(
            objc2_foundation::NSPoint::new(self.x, self.y),
            NSSize::new(self.width, self.height),
        )
    }
}

/// The cursor type to display for a region.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorKind {
    /// Standard pointer cursor (arrow) for clickable UI elements
    Pointer,
    /// I-beam cursor for text editing areas
    IBeam,
}

/// A collection of cursor regions that map areas of the view to cursor types.
///
/// Regions are applied in order, with later regions taking precedence for
/// overlapping areas. The default cursor (for uncovered areas) is IBeam,
/// matching the primary text editing use case.
#[derive(Debug, Clone, Default)]
pub struct CursorRegions {
    /// Regions with pointer cursor (left rail tiles, tab bar tabs, etc.)
    pub pointer_rects: Vec<CursorRect>,
    /// Regions with I-beam cursor (buffer text area, mini-buffer input, etc.)
    pub ibeam_rects: Vec<CursorRect>,
}

impl CursorRegions {
    /// Creates a new empty CursorRegions collection.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a pointer cursor region.
    pub fn add_pointer(&mut self, rect: CursorRect) {
        self.pointer_rects.push(rect);
    }

    /// Adds an I-beam cursor region.
    pub fn add_ibeam(&mut self, rect: CursorRect) {
        self.ibeam_rects.push(rect);
    }

    /// Clears all regions.
    pub fn clear(&mut self) {
        self.pointer_rects.clear();
        self.ibeam_rects.clear();
    }
}

/// Internal state for MetalView
pub struct MetalViewIvars {
    /// The CAMetalLayer for Metal rendering
    metal_layer: Retained<CAMetalLayer>,
    /// The Metal device
    device: Retained<ProtocolObject<dyn MTLDevice>>,
    /// Current backing scale factor (for retina support)
    scale_factor: Cell<CGFloat>,
    // Chunk: docs/chunks/pty_wakeup_reentrant - EventSender for unified event queue
    /// Event sender for the unified event queue (preferred over closures)
    event_sender: RefCell<Option<EventSender>>,
    /// Key event handler callback (legacy, kept for backward compatibility)
    key_handler: RefCell<Option<KeyHandler>>,
    /// Mouse event handler callback (legacy, kept for backward compatibility)
    mouse_handler: RefCell<Option<MouseHandler>>,
    /// Scroll event handler callback (legacy, kept for backward compatibility)
    scroll_handler: RefCell<Option<ScrollHandler>>,
    // Chunk: docs/chunks/cursor_pointer_ui_hints - Cursor regions for dynamic cursor display
    /// Cursor regions for different cursor types (pointer vs I-beam)
    cursor_regions: RefCell<CursorRegions>,
}

impl Default for MetalViewIvars {
    fn default() -> Self {
        // Get the system default Metal device
        let device = get_default_metal_device()
            .expect("Failed to get Metal device - Metal may not be supported on this system");

        // Create the CAMetalLayer
        let metal_layer = CAMetalLayer::new();

        // Configure the layer
        metal_layer.setDevice(Some(&*device));

        // Use BGRA8 pixel format (standard for display)
        metal_layer.setPixelFormat(objc2_metal::MTLPixelFormat::BGRA8Unorm);

        // We don't need to read back from the drawable
        metal_layer.setFramebufferOnly(true);

        // Initialize with scale factor 1.0 (will be updated when attached to window)
        metal_layer.setContentsScale(1.0);

        Self {
            metal_layer,
            device,
            scale_factor: Cell::new(1.0),
            event_sender: RefCell::new(None),
            key_handler: RefCell::new(None),
            mouse_handler: RefCell::new(None),
            scroll_handler: RefCell::new(None),
            cursor_regions: RefCell::new(CursorRegions::new()),
        }
    }
}

define_class!(
    // SAFETY: MetalView follows Objective-C memory management rules
    // and is only accessed from the main thread
    #[unsafe(super = NSView)]
    #[thread_kind = MainThreadOnly]
    #[ivars = MetalViewIvars]
    #[name = "LiteEditMetalView"]
    pub struct MetalView;

    // SAFETY: NSObjectProtocol is correctly implemented - we inherit from NSView
    unsafe impl NSObjectProtocol for MetalView {}

    // Methods for MetalView - overriding NSView methods
    impl MetalView {
        /// Returns YES to indicate this view wants a layer
        #[unsafe(method(wantsLayer))]
        fn __wants_layer(&self) -> bool {
            true
        }

        /// Returns YES to indicate this view updates the layer
        #[unsafe(method(wantsUpdateLayer))]
        fn __wants_update_layer(&self) -> bool {
            true
        }

        /// Override to provide our CAMetalLayer as the backing layer
        #[unsafe(method_id(makeBackingLayer))]
        fn __make_backing_layer(&self) -> Retained<CALayer> {
            // Return our stored metal layer (upcast to CALayer)
            let metal_layer = &self.ivars().metal_layer;
            // Clone and upcast
            Retained::into_super(metal_layer.clone())
        }

        /// Called when backing properties (like scale factor) change
        #[unsafe(method(viewDidChangeBackingProperties))]
        fn __view_did_change_backing_properties(&self) {
            // Get the new scale factor
            if let Some(window) = self.window() {
                let scale: CGFloat = unsafe { msg_send![&window, backingScaleFactor] };
                self.ivars().scale_factor.set(scale);

                // Update layer scale and drawable size
                let layer = &self.ivars().metal_layer;
                layer.setContentsScale(scale);

                // Update drawable size for the new scale
                self.update_drawable_size_internal();
            }
        }

        /// Called when the view's frame changes
        #[unsafe(method(setFrameSize:))]
        fn __set_frame_size(&self, new_size: NSSize) {
            // Call super
            let _: () = unsafe { msg_send![super(self), setFrameSize: new_size] };

            // Update drawable size for the new dimensions
            self.update_drawable_size_internal();
        }

        /// Returns YES to accept first responder status (receive key events)
        #[unsafe(method(acceptsFirstResponder))]
        fn __accepts_first_responder(&self) -> bool {
            true
        }

        /// Returns YES because our view can become key
        #[unsafe(method(canBecomeKeyView))]
        fn __can_become_key_view(&self) -> bool {
            true
        }

        // Chunk: docs/chunks/terminal_image_paste - acceptsFirstMouse for click-through behavior
        /// Returns true to accept mouse events on first click when window is inactive.
        ///
        /// This enables click-through behavior: when lite-edit is not the key window,
        /// the first click/drag both activates the window AND delivers the event to
        /// the view. This is important for drag-and-drop from other apps so that the
        /// pane under the drop point can receive focus, and for general click-to-focus
        /// behavior where clicking a specific pane should focus that pane immediately.
        #[unsafe(method(acceptsFirstMouse:))]
        fn __accepts_first_mouse(&self, _event: Option<&NSEvent>) -> bool {
            true
        }

        // Chunk: docs/chunks/unicode_ime_input - Route key events through text input system
        /// Handle key down events.
        ///
        /// Key events are handled in two paths:
        ///
        /// 1. **Bypass path**: Keys with Command modifier or special function keys are
        ///    sent directly to the key event handler, bypassing the text input system.
        ///    This ensures that shortcuts like Cmd+S, Cmd+P, Cmd+Q work immediately.
        ///
        /// 2. **Text input path**: All other keys are routed through `interpretKeyEvents:`
        ///    which invokes the macOS text input system. This enables:
        ///    - IME composition for CJK languages
        ///    - Dead key compose sequences (é, ñ, etc.)
        ///    - Unicode hex input (Ctrl+Shift+U on some systems)
        ///
        ///    The text input system then calls NSTextInputClient protocol methods
        ///    (`insertText:`, `setMarkedText:`, etc.) which we implement below.
        // Chunk: docs/chunks/emacs_line_nav - Route Ctrl-modified keys through bypass path
        #[unsafe(method(keyDown:))]
        fn __key_down(&self, event: &NSEvent) {
            let flags = event.modifierFlags();

            // Check if this is a "bypass" key that should skip the text input system.
            // These include:
            // - Keys with Command modifier (shortcuts like Cmd+S, Cmd+Q)
            // - Keys with Control modifier (Emacs bindings like Ctrl+A, Ctrl+E)
            // - Escape key (cancel operations, exit modes)
            // - Function keys (F1-F12)
            // - Navigation keys without modifiers that we handle specially
            let key_code = event.keyCode();
            let is_function_key = matches!(key_code,
                0x7A..=0x7F | // F1-F4 and some system keys
                0x60..=0x6F | // F5-F12 and other function keys
                0x72         // Insert/Help
            );
            let is_escape = key_code == 0x35;
            let has_command = flags.contains(NSEventModifierFlags::Command);
            let has_control = flags.contains(NSEventModifierFlags::Control);

            // Bypass the text input system for command shortcuts, control shortcuts, and function keys.
            // Control-modified keys (Emacs bindings) must bypass interpretKeyEvents because Cocoa
            // translates them to Cocoa selectors that may not match our expected commands. For example,
            // Ctrl+A becomes moveToBeginningOfParagraph: instead of moveToBeginningOfLine:.
            // By routing Ctrl+key through convert_key_event() directly, we preserve the full key+modifiers
            // and let resolve_command() handle the mapping to editor commands.
            if has_command || has_control || is_escape || is_function_key {
                if let Some(key_event) = self.convert_key_event(event) {
                    let sender = self.ivars().event_sender.borrow();
                    if let Some(sender) = sender.as_ref() {
                        let _ = sender.send_key(key_event);
                    } else {
                        drop(sender);
                        let handler = self.ivars().key_handler.borrow();
                        if let Some(handler) = handler.as_ref() {
                            handler(key_event);
                        }
                    }
                }
                return;
            }

            // Route through the text input system for all other keys.
            // This will invoke NSTextInputClient methods (insertText:, setMarkedText:, etc.)
            // based on the current input method.
            let event_array = NSArray::from_slice(&[event]);
            self.interpretKeyEvents(&event_array);
        }
    }

    // Chunk: docs/chunks/input_keystroke_regression - NSTextInputClient protocol conformance
    // SAFETY: NSTextInputClient protocol methods are in this impl block so objc2 can
    // register them with the Objective-C runtime during class creation.
    unsafe impl NSTextInputClient for MetalView {
        // Chunk: docs/chunks/unicode_ime_input - NSTextInputClient: insertText:replacementRange:
        /// Called by the text input system to insert final (committed) text.
        ///
        /// This is invoked when:
        /// - User types regular characters
        /// - IME commits composed text
        /// - Paste operations occur
        /// - Dictation produces text
        ///
        /// The `string` parameter is an `NSString` or `NSAttributedString` containing
        /// the text to insert. The `replacement_range` indicates which existing text
        /// to replace (or `NSNotFound` for insertion at cursor).
        #[unsafe(method(insertText:replacementRange:))]
        fn __insert_text(&self, string: &objc2::runtime::AnyObject, _replacement_range: objc2_foundation::NSRange) {
            // Convert the string to Rust. The string can be NSString or NSAttributedString,
            // but we can use the description method to get the text content.
            let text: Retained<objc2_foundation::NSString> = unsafe { msg_send![string, description] };
            let text_str = text.to_string();

            if text_str.is_empty() {
                return;
            }

            // Send the text input event
            let sender = self.ivars().event_sender.borrow();
            if let Some(sender) = sender.as_ref() {
                let _ = sender.send_insert_text(TextInputEvent::new(text_str));
            }
        }

        // Chunk: docs/chunks/unicode_ime_input - NSTextInputClient: setMarkedText:selectedRange:replacementRange:
        /// Called by the text input system during IME composition.
        ///
        /// "Marked text" is uncommitted composition text, displayed with an underline.
        /// This is called as the user types in an IME, showing candidate characters.
        ///
        /// # Arguments
        /// * `string` - The marked text (NSString or NSAttributedString)
        /// * `selected_range` - The selected portion within the marked text (for cursor positioning)
        /// * `replacement_range` - Which existing text to replace (or NSNotFound)
        #[unsafe(method(setMarkedText:selectedRange:replacementRange:))]
        fn __set_marked_text(
            &self,
            string: &objc2::runtime::AnyObject,
            selected_range: objc2_foundation::NSRange,
            _replacement_range: objc2_foundation::NSRange,
        ) {
            // Convert the string to Rust
            let text: Retained<objc2_foundation::NSString> = unsafe { msg_send![string, description] };
            let text_str = text.to_string();

            // Convert NSRange to Rust range
            let selected_start = selected_range.location as usize;
            let selected_end = selected_start + selected_range.length as usize;

            // Send the marked text event
            let sender = self.ivars().event_sender.borrow();
            if let Some(sender) = sender.as_ref() {
                let _ = sender.send_set_marked_text(MarkedTextEvent::with_selection(
                    text_str,
                    selected_start..selected_end,
                ));
            }
        }

        // Chunk: docs/chunks/unicode_ime_input - NSTextInputClient: unmarkText
        /// Called by the text input system to clear marked text without committing.
        ///
        /// This is called when the user cancels IME composition (e.g., presses Escape)
        /// or when focus changes away from the text field.
        #[unsafe(method(unmarkText))]
        fn __unmark_text(&self) {
            let sender = self.ivars().event_sender.borrow();
            if let Some(sender) = sender.as_ref() {
                let _ = sender.send_unmark_text();
            }
        }

        // Chunk: docs/chunks/unicode_ime_input - NSTextInputClient: hasMarkedText
        /// Returns whether the view currently has marked text.
        ///
        /// The text input system calls this to determine the composition state.
        /// For now, we return NO since we don't track marked text state in the view.
        /// The actual marked text state is in TextBuffer.
        ///
        /// TODO: Consider adding a callback to query the buffer's marked text state.
        #[unsafe(method(hasMarkedText))]
        fn __has_marked_text(&self) -> bool {
            false
        }

        // Chunk: docs/chunks/unicode_ime_input - NSTextInputClient: markedRange
        /// Returns the range of marked text in the document.
        ///
        /// Returns `{NSNotFound, 0}` when there's no marked text.
        #[unsafe(method(markedRange))]
        fn __marked_range(&self) -> objc2_foundation::NSRange {
            // Return NSNotFound to indicate no marked text
            // NSNotFound is defined as NSIntegerMax, which is isize::MAX
            objc2_foundation::NSRange {
                location: usize::MAX,
                length: 0,
            }
        }

        // Chunk: docs/chunks/unicode_ime_input - NSTextInputClient: selectedRange
        /// Returns the range of selected text in the document.
        ///
        /// Returns `{NSNotFound, 0}` when there's no selection.
        /// The text input system uses this to know where to insert text.
        ///
        /// TODO: Consider adding a callback to query the buffer's selection state.
        #[unsafe(method(selectedRange))]
        fn __selected_range(&self) -> objc2_foundation::NSRange {
            // Return empty range at "position 0" - we don't track document position here
            objc2_foundation::NSRange {
                location: 0,
                length: 0,
            }
        }

        // Chunk: docs/chunks/unicode_ime_input - NSTextInputClient: validAttributesForMarkedText
        /// Returns the text attributes supported for marked text display.
        ///
        /// We return an empty array since we handle styling ourselves.
        #[unsafe(method_id(validAttributesForMarkedText))]
        fn __valid_attributes_for_marked_text(&self) -> Retained<NSArray<objc2_foundation::NSString>> {
            NSArray::new()
        }

        // Chunk: docs/chunks/unicode_ime_input - NSTextInputClient: attributedSubstringForProposedRange:actualRange:
        /// Returns the attributed substring for a given range.
        ///
        /// The text input system calls this for candidate window positioning and
        /// reconversion. For now, we return nil.
        #[unsafe(method_id(attributedSubstringForProposedRange:actualRange:))]
        fn __attributed_substring_for_proposed_range(
            &self,
            _range: objc2_foundation::NSRange,
            _actual_range: *mut objc2_foundation::NSRange,
        ) -> Option<Retained<objc2_foundation::NSAttributedString>> {
            None
        }

        // Chunk: docs/chunks/unicode_ime_input - NSTextInputClient: firstRectForCharacterRange:actualRange:
        /// Returns the screen rect for a character range (for IME candidate window positioning).
        ///
        /// The text input system calls this to position the IME candidate window near
        /// the composition point. For now, we return a rect near the top-left of the view.
        ///
        /// TODO: Return accurate cursor position for proper candidate window placement.
        #[unsafe(method(firstRectForCharacterRange:actualRange:))]
        fn __first_rect_for_character_range(
            &self,
            _range: objc2_foundation::NSRange,
            _actual_range: *mut objc2_foundation::NSRange,
        ) -> NSRect {
            // Return a rect relative to the screen
            // For now, return the window's frame origin + some offset
            // This is a fallback - a proper implementation would return the cursor position
            if let Some(window) = self.window() {
                let view_frame = self.frame();
                let window_frame = window.frame();

                // Convert view origin to screen coordinates
                // Place candidate window near top-left of view as fallback
                NSRect::new(
                    objc2_foundation::NSPoint::new(
                        window_frame.origin.x + view_frame.origin.x + 50.0,
                        window_frame.origin.y + view_frame.origin.y + view_frame.size.height - 50.0,
                    ),
                    NSSize::new(0.0, self.ivars().scale_factor.get() * 16.0),
                )
            } else {
                NSRect::new(
                    objc2_foundation::NSPoint::new(0.0, 0.0),
                    NSSize::new(0.0, 16.0),
                )
            }
        }

        // Chunk: docs/chunks/unicode_ime_input - NSTextInputClient: characterIndexForPoint:
        /// Returns the character index for a screen point.
        ///
        /// The text input system calls this for mouse-based candidate selection.
        /// We return NSNotFound since we don't implement position-to-index mapping here.
        #[unsafe(method(characterIndexForPoint:))]
        fn __character_index_for_point(&self, _point: objc2_foundation::NSPoint) -> usize {
            // NSNotFound is defined as NSIntegerMax
            usize::MAX
        }

        // Chunk: docs/chunks/unicode_ime_input - NSTextInputClient: doCommandBySelector:
        /// Handle action commands from the text input system.
        ///
        /// This is called for commands like Enter, Tab, Delete that the text input system
        /// doesn't consume as text. We convert these to KeyEvent and send them.
        #[unsafe(method(doCommandBySelector:))]
        fn __do_command_by_selector(&self, selector: objc2::runtime::Sel) {
            // Convert the selector to a key event if it's a recognized command
            // The selector name is a C string, so we convert it to a Rust &str
            let sel_name_cstr = selector.name();
            let sel_name = sel_name_cstr.to_str().unwrap_or("");

            let key = match sel_name {
                "insertNewline:" => Some(Key::Return),
                "insertTab:" => Some(Key::Tab),
                "insertBacktab:" => Some(Key::Tab), // With Shift modifier
                "deleteBackward:" => Some(Key::Backspace),
                "deleteForward:" => Some(Key::Delete),
                "moveLeft:" => Some(Key::Left),
                "moveRight:" => Some(Key::Right),
                "moveUp:" => Some(Key::Up),
                "moveDown:" => Some(Key::Down),
                "moveToBeginningOfLine:" => Some(Key::Home),
                "moveToEndOfLine:" => Some(Key::End),
                "moveToBeginningOfDocument:" => Some(Key::Home), // Cmd+Home
                "moveToEndOfDocument:" => Some(Key::End),        // Cmd+End
                "pageUp:" => Some(Key::PageUp),
                "pageDown:" => Some(Key::PageDown),
                // Selection variants (would need Shift modifier)
                "moveLeftAndModifySelection:" => Some(Key::Left),
                "moveRightAndModifySelection:" => Some(Key::Right),
                "moveUpAndModifySelection:" => Some(Key::Up),
                "moveDownAndModifySelection:" => Some(Key::Down),
                "moveToBeginningOfLineAndModifySelection:" => Some(Key::Home),
                "moveToEndOfLineAndModifySelection:" => Some(Key::End),
                // Cancel
                "cancelOperation:" | "cancel:" => Some(Key::Escape),
                // Ignore other selectors
                _ => None,
            };

            if let Some(key) = key {
                // Determine modifiers based on selector name
                let modifiers = if sel_name.contains("ModifySelection") {
                    Modifiers {
                        shift: true,
                        ..Modifiers::default()
                    }
                } else if sel_name == "insertBacktab:" {
                    Modifiers {
                        shift: true,
                        ..Modifiers::default()
                    }
                } else {
                    Modifiers::default()
                };

                let key_event = KeyEvent::new(key, modifiers);
                let sender = self.ivars().event_sender.borrow();
                if let Some(sender) = sender.as_ref() {
                    let _ = sender.send_key(key_event);
                } else {
                    drop(sender);
                    let handler = self.ivars().key_handler.borrow();
                    if let Some(handler) = handler.as_ref() {
                        handler(key_event);
                    }
                }
            }
        }
    }

    impl MetalView {
        /// Handle flags changed events (modifier key changes)
        #[unsafe(method(flagsChanged:))]
        fn __flags_changed(&self, _event: &NSEvent) {
            // For future use: capture modifier key state changes
            // Currently not needed since we capture modifiers with each key event
        }

        // Chunk: docs/chunks/mouse_click_cursor - NSView mouseDown: override - receives macOS mouse events
        // Chunk: docs/chunks/pty_wakeup_reentrant - Prefer EventSender over closure
        /// Handle mouse down events
        #[unsafe(method(mouseDown:))]
        fn __mouse_down(&self, event: &NSEvent) {
            if let Some(mouse_event) = self.convert_mouse_event(event, MouseEventKind::Down) {
                let sender = self.ivars().event_sender.borrow();
                if let Some(sender) = sender.as_ref() {
                    let _ = sender.send_mouse(mouse_event);
                } else {
                    drop(sender);
                    let handler = self.ivars().mouse_handler.borrow();
                    if let Some(handler) = handler.as_ref() {
                        handler(mouse_event);
                    }
                }
            }
        }

        // Chunk: docs/chunks/mouse_drag_selection - Mouse drag selection
        // Chunk: docs/chunks/pty_wakeup_reentrant - Prefer EventSender over closure
        /// Handle mouse dragged events
        #[unsafe(method(mouseDragged:))]
        fn __mouse_dragged(&self, event: &NSEvent) {
            if let Some(mouse_event) = self.convert_mouse_event(event, MouseEventKind::Moved) {
                let sender = self.ivars().event_sender.borrow();
                if let Some(sender) = sender.as_ref() {
                    let _ = sender.send_mouse(mouse_event);
                } else {
                    drop(sender);
                    let handler = self.ivars().mouse_handler.borrow();
                    if let Some(handler) = handler.as_ref() {
                        handler(mouse_event);
                    }
                }
            }
        }

        // Chunk: docs/chunks/mouse_drag_selection - Mouse drag selection
        // Chunk: docs/chunks/pty_wakeup_reentrant - Prefer EventSender over closure
        /// Handle mouse up events
        #[unsafe(method(mouseUp:))]
        fn __mouse_up(&self, event: &NSEvent) {
            if let Some(mouse_event) = self.convert_mouse_event(event, MouseEventKind::Up) {
                let sender = self.ivars().event_sender.borrow();
                if let Some(sender) = sender.as_ref() {
                    let _ = sender.send_mouse(mouse_event);
                } else {
                    drop(sender);
                    let handler = self.ivars().mouse_handler.borrow();
                    if let Some(handler) = handler.as_ref() {
                        handler(mouse_event);
                    }
                }
            }
        }

        // Chunk: docs/chunks/viewport_scrolling - macOS scrollWheel event handler
        // Chunk: docs/chunks/pty_wakeup_reentrant - Prefer EventSender over closure
        /// Handle scroll wheel events (trackpad, mouse wheel)
        #[unsafe(method(scrollWheel:))]
        fn __scroll_wheel(&self, event: &NSEvent) {
            if let Some(scroll_delta) = self.convert_scroll_event(event) {
                let sender = self.ivars().event_sender.borrow();
                if let Some(sender) = sender.as_ref() {
                    let _ = sender.send_scroll(scroll_delta);
                } else {
                    drop(sender);
                    let handler = self.ivars().scroll_handler.borrow();
                    if let Some(handler) = handler.as_ref() {
                        handler(scroll_delta);
                    }
                }
            }
        }

        // Chunk: docs/chunks/ibeam_cursor - I-beam cursor over editable area
        // Chunk: docs/chunks/cursor_pointer_ui_hints - Dynamic cursor regions
        /// Sets up cursor rects based on stored cursor regions.
        ///
        /// This method is called by the framework when cursor rects need to be
        /// recalculated (e.g., after window resize or `invalidateCursorRectsForView`).
        /// It uses the cursor regions stored via `set_cursor_regions` to set up
        /// different cursor types for different areas:
        ///
        /// - Pointer (arrow) cursor for clickable UI elements (left rail, tabs, etc.)
        /// - I-beam cursor for text editing areas (buffer, mini-buffer)
        ///
        /// Regions are applied in order with I-beam regions added last, so they
        /// take precedence in overlapping areas. This ensures the text editing
        /// cursor appears over the primary editing region.
        #[unsafe(method(resetCursorRects))]
        fn __reset_cursor_rects(&self) {
            // Clear existing cursor rects
            self.discardCursorRects();

            let regions = self.ivars().cursor_regions.borrow();

            // Add pointer cursor rects first (clickable UI elements)
            let arrow_cursor = NSCursor::arrowCursor();
            for rect in &regions.pointer_rects {
                self.addCursorRect_cursor(rect.to_ns_rect(), &arrow_cursor);
            }

            // Add I-beam cursor rects last (takes precedence in overlapping areas)
            let ibeam_cursor = NSCursor::IBeamCursor();
            for rect in &regions.ibeam_rects {
                self.addCursorRect_cursor(rect.to_ns_rect(), &ibeam_cursor);
            }

            // If no regions are defined, fall back to I-beam for entire bounds
            // (maintains backwards compatibility with existing behavior)
            if regions.pointer_rects.is_empty() && regions.ibeam_rects.is_empty() {
                self.addCursorRect_cursor(self.bounds(), &ibeam_cursor);
            }
        }

        // Chunk: docs/chunks/dragdrop_file_paste - NSDraggingDestination protocol implementation
        /// Called when a drag operation enters the view.
        ///
        /// Returns `NSDragOperation::Copy` to indicate we accept file drops.
        /// This makes the drag cursor show a copy badge.
        #[unsafe(method(draggingEntered:))]
        fn __dragging_entered(&self, _sender: &ProtocolObject<dyn NSDraggingInfo>) -> NSDragOperation {
            NSDragOperation::Copy
        }

        // Chunk: docs/chunks/dragdrop_file_paste - NSDraggingDestination protocol implementation
        // Chunk: docs/chunks/terminal_image_paste - Extract drop position for pane-aware routing
        /// Called when user releases the drag over our view.
        ///
        /// Extracts file URLs from the pasteboard and sends them via the event channel,
        /// along with the drop position for pane-aware routing.
        /// Returns `true` on success, `false` if no files were dropped.
        #[unsafe(method(performDragOperation:))]
        fn __perform_drag_operation(&self, sender: &ProtocolObject<dyn NSDraggingInfo>) -> bool {
            // Get the pasteboard from the drag info
            let pasteboard = sender.draggingPasteboard();

            // Try to read file URLs from the pasteboard
            // We need to get the class object for NSURL
            let url_class = NSURL::class();
            let class_array = NSArray::from_slice(&[url_class]);

            // Read file URLs from the pasteboard
            // SAFETY: We're passing the correct class type (NSURL) and no options
            let urls: Option<Retained<NSArray>> = unsafe {
                pasteboard.readObjectsForClasses_options(&class_array, None)
            };

            let Some(urls) = urls else {
                return false.into();
            };

            // Convert URLs to file paths
            let mut paths: Vec<String> = Vec::new();
            for i in 0..urls.len() {
                // Get each URL object. Since we requested NSURL class, these are NSURL instances.
                // SAFETY: We requested NSURL class, so the returned objects are NSURL instances.
                let obj = urls.objectAtIndex(i);
                let url: &NSURL = unsafe { &*(&*obj as *const _ as *const NSURL) };

                // Get the file path from the URL
                if let Some(path) = url.path() {
                    paths.push(path.to_string());
                }
            }

            if paths.is_empty() {
                return false.into();
            }

            // Extract drop position from NSDraggingInfo
            // draggingLocation returns NSPoint in window coordinates
            let location_in_window: objc2_foundation::NSPoint = sender.draggingLocation();

            // Convert to view coordinates (same pattern as mouse events)
            let location_in_view: objc2_foundation::NSPoint =
                unsafe { msg_send![self, convertPoint: location_in_window, fromView: std::ptr::null::<NSView>()] };

            // Get scale factor and view frame for coordinate conversion
            let scale = self.ivars().scale_factor.get();
            let frame = self.frame();

            // Convert to pixels and flip Y coordinate from bottom-left (NSView) to top-left (screen)
            // This matches the coordinate system used by resolve_pane_hit
            let position = (
                location_in_view.x * scale,
                (frame.size.height - location_in_view.y) * scale,
            );

            // Send the file drop event via the event sender with position
            let event_sender_guard = self.ivars().event_sender.borrow();
            if let Some(event_sender) = event_sender_guard.as_ref() {
                let _ = event_sender.send_file_drop(paths, position);
            }

            true.into()
        }
    }
);

impl MetalView {
    /// Creates a new MetalView with the given frame
    // Chunk: docs/chunks/dragdrop_file_paste - Register for file URL drag types
    pub fn new(mtm: MainThreadMarker, frame: NSRect) -> Retained<Self> {
        // Create the view with default ivars (which initializes Metal)
        let this = mtm.alloc::<Self>();
        let this = this.set_ivars(MetalViewIvars::default());

        let this: Retained<Self> = unsafe { msg_send![super(this), initWithFrame: frame] };

        // Enable layer backing
        this.setWantsLayer(true);

        // Register for file URL drag types (drag-and-drop support)
        // This allows files to be dropped onto the view
        let file_url_type = unsafe { NSPasteboardTypeFileURL };
        let drag_types = NSArray::from_slice(&[file_url_type]);
        this.registerForDraggedTypes(&drag_types);

        this
    }

    /// Returns the Metal device used by this view
    pub fn device(&self) -> &ProtocolObject<dyn MTLDevice> {
        self.ivars().device.as_ref()
    }

    /// Returns the CAMetalLayer for rendering
    pub fn metal_layer(&self) -> &CAMetalLayer {
        &self.ivars().metal_layer
    }

    /// Returns the current scale factor (1.0 for standard, 2.0 for Retina)
    pub fn scale_factor(&self) -> f64 {
        self.ivars().scale_factor.get()
    }

    /// Syncs the backing scale factor from the view's window.
    ///
    /// `viewDidChangeBackingProperties` may not fire synchronously when the
    /// view is added to a window via `setContentView`. Call this explicitly
    /// after attaching the view to a window to ensure the scale factor,
    /// layer contentsScale, and drawable size are correct before creating
    /// scale-dependent resources (font, glyph atlas, etc.).
    pub fn sync_backing_properties(&self) {
        if let Some(window) = self.window() {
            let scale: CGFloat = unsafe { msg_send![&window, backingScaleFactor] };
            self.ivars().scale_factor.set(scale);
            self.ivars().metal_layer.setContentsScale(scale);
            self.update_drawable_size_internal();
        }
    }

    /// Updates the drawable size based on current frame and scale factor
    pub fn update_drawable_size(&self) {
        self.update_drawable_size_internal();
    }

    /// Internal method to update drawable size (called from ObjC overrides)
    fn update_drawable_size_internal(&self) {
        let frame = self.frame();
        let scale = self.ivars().scale_factor.get();

        // Calculate the drawable size in pixels (accounting for retina)
        let width = frame.size.width * scale;
        let height = frame.size.height * scale;

        if width > 0.0 && height > 0.0 {
            // NSSize is the same as CGSize
            let drawable_size = NSSize::new(width, height);
            self.ivars().metal_layer.setDrawableSize(drawable_size);
        }
    }

    // Chunk: docs/chunks/pty_wakeup_reentrant - EventSender replaces individual closures
    /// Sets the event sender for unified event delivery.
    ///
    /// When set, all input events (key, mouse, scroll) are sent through this
    /// sender rather than the individual handler closures. This is the preferred
    /// approach as it eliminates `Rc<RefCell<>>` borrow conflicts.
    ///
    /// # Arguments
    /// * `sender` - The EventSender to use for event delivery
    pub fn set_event_sender(&self, sender: EventSender) {
        *self.ivars().event_sender.borrow_mut() = Some(sender);
    }

    /// Sets the key event handler callback (legacy)
    ///
    /// The handler will be called for each keyDown event, with the
    /// NSEvent converted to our Rust-native KeyEvent type.
    ///
    /// Note: If an EventSender is set via `set_event_sender`, it takes
    /// precedence over this handler.
    pub fn set_key_handler(&self, handler: impl Fn(KeyEvent) + 'static) {
        *self.ivars().key_handler.borrow_mut() = Some(Box::new(handler));
    }

    // Chunk: docs/chunks/mouse_click_cursor - Mouse handler callback registration (parallel to set_key_handler)
    /// Sets the mouse event handler callback (legacy)
    ///
    /// The handler will be called for each mouseDown event, with the
    /// NSEvent converted to our Rust-native MouseEvent type.
    ///
    /// Note: If an EventSender is set via `set_event_sender`, it takes
    /// precedence over this handler.
    pub fn set_mouse_handler(&self, handler: impl Fn(MouseEvent) + 'static) {
        *self.ivars().mouse_handler.borrow_mut() = Some(Box::new(handler));
    }

    // Chunk: docs/chunks/viewport_scrolling - Scroll handler registration
    /// Sets the scroll event handler callback (legacy)
    ///
    /// The handler will be called for each scrollWheel event, with the
    /// NSEvent converted to our Rust-native ScrollDelta type.
    ///
    /// Note: If an EventSender is set via `set_event_sender`, it takes
    /// precedence over this handler.
    pub fn set_scroll_handler(&self, handler: impl Fn(ScrollDelta) + 'static) {
        *self.ivars().scroll_handler.borrow_mut() = Some(Box::new(handler));
    }

    // Chunk: docs/chunks/cursor_pointer_ui_hints - Set cursor regions for dynamic cursor display
    /// Sets the cursor regions for this view.
    ///
    /// Call this whenever the UI layout changes (e.g., after rendering) to update
    /// which areas of the view display which cursor type. This replaces any
    /// previously set regions.
    ///
    /// After updating the regions, this method calls `invalidateCursorRectsForView`
    /// to trigger the system to call `resetCursorRects`, which applies the new
    /// cursor mappings.
    ///
    /// # Arguments
    /// * `regions` - The cursor regions mapping areas to cursor types
    ///
    /// # Cursor Regions
    /// - Pointer regions: Clickable UI elements (left rail tiles, tab bar tabs,
    ///   selector items, close buttons, etc.)
    /// - I-beam regions: Text editing areas (buffer content area, mini-buffer
    ///   input field, etc.)
    pub fn set_cursor_regions(&self, regions: CursorRegions) {
        *self.ivars().cursor_regions.borrow_mut() = regions;

        // Trigger the system to recalculate cursor rects
        if let Some(window) = self.window() {
            window.invalidateCursorRectsForView(self);
        }
    }

    /// Converts an NSEvent to our KeyEvent type
    fn convert_key_event(&self, event: &NSEvent) -> Option<KeyEvent> {
        let modifiers = self.convert_modifiers(event);
        let key = self.convert_key(event)?;
        Some(KeyEvent::new(key, modifiers))
    }

    // Chunk: docs/chunks/mouse_click_cursor - NSEvent to MouseEvent conversion with scale factor handling
    /// Converts an NSEvent to our MouseEvent type
    ///
    /// # Arguments
    /// * `event` - The NSEvent containing mouse information
    /// * `kind` - The kind of mouse event (Down, Up, Moved)
    ///
    /// # Returns
    /// A MouseEvent with position in pixel coordinates (view space, origin at top-left
    /// after y-flip applied by consumer) and modifier flags.
    fn convert_mouse_event(&self, event: &NSEvent, kind: MouseEventKind) -> Option<MouseEvent> {
        // Get location in window coordinates
        let location_in_window = event.locationInWindow();

        // Convert to view coordinates
        // NSView's convertPoint:fromView: with nil converts from window coordinates
        let location_in_view: objc2_foundation::NSPoint =
            unsafe { msg_send![self, convertPoint: location_in_window, fromView: std::ptr::null::<NSView>()] };

        // Get the scale factor for pixel conversion
        let scale = self.ivars().scale_factor.get();

        // NSEvent coordinates are in points. Convert to pixels by multiplying by scale.
        // Note: NSView uses bottom-left origin. The consumer (buffer_target) will flip
        // the y-coordinate using view_height.
        let position = (
            location_in_view.x * scale,
            location_in_view.y * scale, // This is from bottom-left, will be flipped later
        );

        let modifiers = self.convert_modifiers(event);

        // Chunk: docs/chunks/word_double_click_select - Double-click word selection
        // Extract click count for double-click detection
        let click_count = event.clickCount() as u32;

        Some(MouseEvent {
            kind,
            position,
            modifiers,
            click_count,
        })
    }

    // Chunk: docs/chunks/scroll_wheel_speed - Line height constant for scroll conversion
    /// Default line height for mouse wheel scroll conversion.
    /// Mouse wheel events report line-based deltas; we convert to pixels
    /// using this constant. Matches typical editor line heights.
    const DEFAULT_LINE_HEIGHT_PX: f64 = 20.0;

    /// Converts an NSEvent scroll wheel event to our ScrollDelta type
    ///
    // Chunk: docs/chunks/viewport_scrolling - NSEvent to ScrollDelta conversion
    // Chunk: docs/chunks/pane_hover_scroll - Extract mouse position for hover-scroll targeting
    // Chunk: docs/chunks/scroll_wheel_speed - Mouse wheel vs trackpad delta handling
    /// macOS scroll wheel events provide delta values in points. For trackpads
    /// with "natural scrolling" enabled (the default), scrolling down (content
    /// moves up) produces positive deltaY values.
    ///
    /// We convert the delta to our ScrollDelta type with the convention:
    /// - Positive dy = scroll down (content moves up, scroll_offset increases)
    /// - Negative dy = scroll up (content moves down, scroll_offset decreases)
    ///
    /// The mouse position at the time of the scroll event is also captured to
    /// enable hover-scroll behavior in multi-pane layouts.
    ///
    /// ## Device-specific handling
    ///
    /// macOS distinguishes between precise (trackpad) and non-precise (mouse wheel)
    /// scroll events via `hasPreciseScrollingDeltas()`:
    /// - **Trackpad**: Returns `true`, deltas are pixel-level (e.g., 15.3, -8.7)
    /// - **Mouse wheel**: Returns `false`, deltas are line-based (e.g., 1.0, -3.0)
    ///
    /// For mouse wheel events, we multiply by `DEFAULT_LINE_HEIGHT_PX` to convert
    /// line-based deltas to pixel-based deltas, matching typical editor behavior
    /// (approximately one line of text per tick).
    fn convert_scroll_event(&self, event: &NSEvent) -> Option<ScrollDelta> {
        // NSEvent scrolling delta methods
        // scrollingDeltaX/Y return CGFloat (f64 on 64-bit)
        // These are "precise" deltas that work with trackpads and mice
        let raw_dx = event.scrollingDeltaX();
        let raw_dy = event.scrollingDeltaY();

        // Check if this is a precise (trackpad) or non-precise (mouse wheel) event
        // For mouse wheel events, multiply by line height to convert from line-based
        // to pixel-based deltas
        let (dx, dy) = if event.hasPreciseScrollingDeltas() {
            // Trackpad: use deltas as-is (already in pixels)
            (raw_dx, raw_dy)
        } else {
            // Mouse wheel: convert line-based deltas to pixels
            (
                raw_dx * Self::DEFAULT_LINE_HEIGHT_PX,
                raw_dy * Self::DEFAULT_LINE_HEIGHT_PX,
            )
        };

        // Skip events with no scroll delta
        if dx == 0.0 && dy == 0.0 {
            return None;
        }

        // Extract mouse position from the scroll event
        // Get location in window coordinates
        let location_in_window = event.locationInWindow();

        // Convert to view coordinates
        let location_in_view: objc2_foundation::NSPoint =
            unsafe { msg_send![self, convertPoint: location_in_window, fromView: std::ptr::null::<NSView>()] };

        // Get the scale factor and view bounds for coordinate conversion
        let scale = self.ivars().scale_factor.get();
        let frame = self.frame();

        // Convert to pixels and flip Y coordinate from bottom-left to top-left origin
        // NSView uses bottom-left origin, but our event system uses top-left
        let x_px = location_in_view.x * scale;
        let y_px = (frame.size.height - location_in_view.y) * scale;

        // macOS "natural scrolling" (default on trackpads) inverts the direction:
        // - Moving fingers down on trackpad = negative deltaY = content scrolls up
        // - Moving fingers up on trackpad = positive deltaY = content scrolls down
        //
        // Our convention is:
        // - Positive dy = scroll down (show content further in the document)
        //
        // So we negate the delta to match our convention.
        Some(ScrollDelta::with_position(-dx, -dy, x_px, y_px))
    }

    /// Converts NSEvent modifier flags to our Modifiers type
    fn convert_modifiers(&self, event: &NSEvent) -> Modifiers {
        let flags = event.modifierFlags();

        Modifiers {
            shift: flags.contains(NSEventModifierFlags::Shift),
            command: flags.contains(NSEventModifierFlags::Command),
            option: flags.contains(NSEventModifierFlags::Option),
            control: flags.contains(NSEventModifierFlags::Control),
        }
    }

    /// Converts an NSEvent to our Key type
    // Chunk: docs/chunks/line_nav_keybindings - Control-key handling with charactersIgnoringModifiers
    fn convert_key(&self, event: &NSEvent) -> Option<Key> {
        // First check key code for special keys
        let key_code = event.keyCode();

        // macOS virtual key codes for special keys
        const KEY_RETURN: u16 = 0x24;
        const KEY_TAB: u16 = 0x30;
        const KEY_DELETE: u16 = 0x33; // Backspace
        const KEY_ESCAPE: u16 = 0x35;
        const KEY_FORWARD_DELETE: u16 = 0x75;
        const KEY_LEFT_ARROW: u16 = 0x7B;
        const KEY_RIGHT_ARROW: u16 = 0x7C;
        const KEY_DOWN_ARROW: u16 = 0x7D;
        const KEY_UP_ARROW: u16 = 0x7E;
        const KEY_HOME: u16 = 0x73;
        const KEY_END: u16 = 0x77;
        const KEY_PAGE_UP: u16 = 0x74;
        const KEY_PAGE_DOWN: u16 = 0x79;
        // Chunk: docs/chunks/terminal_input_encoding - Terminal input encoding
        // Function keys and Insert
        const KEY_INSERT: u16 = 0x72; // Help key on Mac, used as Insert
        const KEY_F1: u16 = 0x7A;
        const KEY_F2: u16 = 0x78;
        const KEY_F3: u16 = 0x63;
        const KEY_F4: u16 = 0x76;
        const KEY_F5: u16 = 0x60;
        const KEY_F6: u16 = 0x61;
        const KEY_F7: u16 = 0x62;
        const KEY_F8: u16 = 0x64;
        const KEY_F9: u16 = 0x65;
        const KEY_F10: u16 = 0x6D;
        const KEY_F11: u16 = 0x67;
        const KEY_F12: u16 = 0x6F;

        match key_code {
            KEY_RETURN => return Some(Key::Return),
            KEY_TAB => return Some(Key::Tab),
            KEY_DELETE => return Some(Key::Backspace),
            KEY_ESCAPE => return Some(Key::Escape),
            KEY_FORWARD_DELETE => return Some(Key::Delete),
            KEY_LEFT_ARROW => return Some(Key::Left),
            KEY_RIGHT_ARROW => return Some(Key::Right),
            KEY_DOWN_ARROW => return Some(Key::Down),
            KEY_UP_ARROW => return Some(Key::Up),
            KEY_HOME => return Some(Key::Home),
            KEY_END => return Some(Key::End),
            KEY_PAGE_UP => return Some(Key::PageUp),
            KEY_PAGE_DOWN => return Some(Key::PageDown),
            // Chunk: docs/chunks/terminal_input_encoding - Terminal input encoding
            KEY_INSERT => return Some(Key::Insert),
            KEY_F1 => return Some(Key::F1),
            KEY_F2 => return Some(Key::F2),
            KEY_F3 => return Some(Key::F3),
            KEY_F4 => return Some(Key::F4),
            KEY_F5 => return Some(Key::F5),
            KEY_F6 => return Some(Key::F6),
            KEY_F7 => return Some(Key::F7),
            KEY_F8 => return Some(Key::F8),
            KEY_F9 => return Some(Key::F9),
            KEY_F10 => return Some(Key::F10),
            KEY_F11 => return Some(Key::F11),
            KEY_F12 => return Some(Key::F12),
            _ => {}
        }

        // For character keys, we need to get the correct character representation.
        //
        // When the Control modifier is held, macOS's event.characters() returns
        // the *interpreted* control character rather than the underlying key.
        // For example:
        //   - Ctrl+A → characters() returns '\x01' (SOH control character)
        //   - Ctrl+E → characters() returns '\x05' (ENQ control character)
        //
        // When the Option modifier is held, macOS's event.characters() returns
        // the *composed* Unicode character for that Option+key combination.
        // For example:
        //   - Option+D → characters() returns 'ð' (eth, U+00F0)
        //   - Option+B → characters() returns '∫' (integral sign)
        //
        // Both cases require charactersIgnoringModifiers() to recover the base key
        // ('d', 'e', etc.) so that modifier-keyed commands like Ctrl+A, Option+D
        // are correctly routed in resolve_command rather than falling through to
        // InsertChar with the composed/control character.
        //
        // Chunk: docs/chunks/word_forward_delete - Option modifier needs base char like Control
        let flags = event.modifierFlags();
        let characters = if flags.contains(NSEventModifierFlags::Control)
            || flags.contains(NSEventModifierFlags::Option)
        {
            event.charactersIgnoringModifiers()?
        } else {
            // Normal case: use characters() which accounts for Shift state
            event.characters()?
        };
        let chars: Vec<char> = characters.to_string().chars().collect();

        if chars.len() == 1 {
            let ch = chars[0];
            // Filter out control characters that we already handled above.
            // Note: When Control is active, we've already used charactersIgnoringModifiers
            // so ch will be the base character ('a', 'e', etc.), not a control character.
            if ch.is_control() && ch != '\t' && ch != '\r' && ch != '\n' {
                return None;
            }
            Some(Key::Char(ch))
        } else {
            None
        }
    }
}

// =============================================================================
// Helper Functions
// =============================================================================

/// Gets the system default Metal device
fn get_default_metal_device() -> Option<Retained<ProtocolObject<dyn MTLDevice>>> {
    // MTLCreateSystemDefaultDevice is a C function we need to call
    extern "C" {
        fn MTLCreateSystemDefaultDevice() -> *mut ProtocolObject<dyn MTLDevice>;
    }

    let ptr = unsafe { MTLCreateSystemDefaultDevice() };
    if ptr.is_null() {
        None
    } else {
        // SAFETY: We just checked that ptr is non-null, and MTLCreateSystemDefaultDevice
        // returns a retained object
        Some(unsafe { Retained::from_raw(ptr).unwrap() })
    }
}

// =============================================================================
// Tests (Chunk: docs/chunks/cursor_pointer_ui_hints)
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cursor_rect_new() {
        let rect = CursorRect::new(10.0, 20.0, 100.0, 50.0);
        assert_eq!(rect.x, 10.0);
        assert_eq!(rect.y, 20.0);
        assert_eq!(rect.width, 100.0);
        assert_eq!(rect.height, 50.0);
    }

    #[test]
    fn test_cursor_rect_to_ns_rect() {
        let cursor_rect = CursorRect::new(10.0, 20.0, 100.0, 50.0);
        let ns_rect = cursor_rect.to_ns_rect();
        assert_eq!(ns_rect.origin.x, 10.0);
        assert_eq!(ns_rect.origin.y, 20.0);
        assert_eq!(ns_rect.size.width, 100.0);
        assert_eq!(ns_rect.size.height, 50.0);
    }

    #[test]
    fn test_cursor_regions_new_is_empty() {
        let regions = CursorRegions::new();
        assert!(regions.pointer_rects.is_empty());
        assert!(regions.ibeam_rects.is_empty());
    }

    #[test]
    fn test_cursor_regions_add_pointer() {
        let mut regions = CursorRegions::new();
        regions.add_pointer(CursorRect::new(0.0, 0.0, 56.0, 600.0));
        assert_eq!(regions.pointer_rects.len(), 1);
        assert!(regions.ibeam_rects.is_empty());
    }

    #[test]
    fn test_cursor_regions_add_ibeam() {
        let mut regions = CursorRegions::new();
        regions.add_ibeam(CursorRect::new(56.0, 0.0, 944.0, 568.0));
        assert!(regions.pointer_rects.is_empty());
        assert_eq!(regions.ibeam_rects.len(), 1);
    }

    #[test]
    fn test_cursor_regions_clear() {
        let mut regions = CursorRegions::new();
        regions.add_pointer(CursorRect::new(0.0, 0.0, 56.0, 600.0));
        regions.add_ibeam(CursorRect::new(56.0, 0.0, 944.0, 568.0));
        assert_eq!(regions.pointer_rects.len(), 1);
        assert_eq!(regions.ibeam_rects.len(), 1);

        regions.clear();
        assert!(regions.pointer_rects.is_empty());
        assert!(regions.ibeam_rects.is_empty());
    }

    #[test]
    fn test_cursor_regions_multiple_rects() {
        let mut regions = CursorRegions::new();
        // Left rail
        regions.add_pointer(CursorRect::new(0.0, 0.0, 56.0, 600.0));
        // Tab bar
        regions.add_pointer(CursorRect::new(56.0, 568.0, 944.0, 32.0));
        // Buffer content area
        regions.add_ibeam(CursorRect::new(56.0, 0.0, 944.0, 568.0));

        assert_eq!(regions.pointer_rects.len(), 2);
        assert_eq!(regions.ibeam_rects.len(), 1);

        // Verify the rects were stored correctly
        assert_eq!(regions.pointer_rects[0].width, 56.0); // left rail
        assert_eq!(regions.pointer_rects[1].height, 32.0); // tab bar
        assert_eq!(regions.ibeam_rects[0].x, 56.0); // buffer content
    }

    #[test]
    fn test_cursor_kind_equality() {
        assert_eq!(CursorKind::Pointer, CursorKind::Pointer);
        assert_eq!(CursorKind::IBeam, CursorKind::IBeam);
        assert_ne!(CursorKind::Pointer, CursorKind::IBeam);
    }

    #[test]
    fn test_cursor_regions_default() {
        let regions = CursorRegions::default();
        assert!(regions.pointer_rects.is_empty());
        assert!(regions.ibeam_rects.is_empty());
    }
}
