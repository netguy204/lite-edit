// Chunk: docs/chunks/metal_surface - macOS window + Metal surface foundation
// Chunk: docs/chunks/editable_buffer - Main loop + input events + editable buffer
// Chunk: docs/chunks/mouse_click_cursor - Mouse click cursor positioning
//!
//! Metal-backed NSView implementation
//!
//! This module provides a custom NSView subclass that uses a CAMetalLayer
//! as its backing layer, enabling GPU-accelerated Metal rendering.
//!
//! The view also handles keyboard and mouse input, converting NSEvent events
//! to our Rust-native KeyEvent and MouseEvent types.

use std::cell::{Cell, RefCell};

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::{define_class, msg_send, DefinedClass, MainThreadOnly};
use objc2_app_kit::{NSEvent, NSEventModifierFlags, NSView};
use objc2_foundation::{MainThreadMarker, NSObjectProtocol, NSRect, NSSize};
use objc2_metal::MTLDevice;
use objc2_quartz_core::{CALayer, CAMetalLayer};

use crate::input::{Key, KeyEvent, Modifiers, MouseEvent, MouseEventKind};

// CGFloat is a type alias for f64 on 64-bit systems
type CGFloat = f64;

// =============================================================================
// MetalView
// =============================================================================

/// Type alias for key event handler callback
pub type KeyHandler = Box<dyn Fn(KeyEvent)>;

/// Type alias for mouse event handler callback
pub type MouseHandler = Box<dyn Fn(MouseEvent)>;

/// Internal state for MetalView
pub struct MetalViewIvars {
    /// The CAMetalLayer for Metal rendering
    metal_layer: Retained<CAMetalLayer>,
    /// The Metal device
    device: Retained<ProtocolObject<dyn MTLDevice>>,
    /// Current backing scale factor (for retina support)
    scale_factor: Cell<CGFloat>,
    /// Key event handler callback
    key_handler: RefCell<Option<KeyHandler>>,
    /// Mouse event handler callback
    mouse_handler: RefCell<Option<MouseHandler>>,
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
            key_handler: RefCell::new(None),
            mouse_handler: RefCell::new(None),
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

        /// Handle key down events
        #[unsafe(method(keyDown:))]
        fn __key_down(&self, event: &NSEvent) {
            if let Some(key_event) = self.convert_key_event(event) {
                let handler = self.ivars().key_handler.borrow();
                if let Some(handler) = handler.as_ref() {
                    handler(key_event);
                }
            }
        }

        /// Handle flags changed events (modifier key changes)
        #[unsafe(method(flagsChanged:))]
        fn __flags_changed(&self, _event: &NSEvent) {
            // For future use: capture modifier key state changes
            // Currently not needed since we capture modifiers with each key event
        }

        /// Handle mouse down events
        #[unsafe(method(mouseDown:))]
        fn __mouse_down(&self, event: &NSEvent) {
            if let Some(mouse_event) = self.convert_mouse_event(event, MouseEventKind::Down) {
                let handler = self.ivars().mouse_handler.borrow();
                if let Some(handler) = handler.as_ref() {
                    handler(mouse_event);
                }
            }
        }

        // Chunk: docs/chunks/mouse_drag_selection - Mouse drag selection
        /// Handle mouse dragged events
        #[unsafe(method(mouseDragged:))]
        fn __mouse_dragged(&self, event: &NSEvent) {
            if let Some(mouse_event) = self.convert_mouse_event(event, MouseEventKind::Moved) {
                let handler = self.ivars().mouse_handler.borrow();
                if let Some(handler) = handler.as_ref() {
                    handler(mouse_event);
                }
            }
        }

        // Chunk: docs/chunks/mouse_drag_selection - Mouse drag selection
        /// Handle mouse up events
        #[unsafe(method(mouseUp:))]
        fn __mouse_up(&self, event: &NSEvent) {
            if let Some(mouse_event) = self.convert_mouse_event(event, MouseEventKind::Up) {
                let handler = self.ivars().mouse_handler.borrow();
                if let Some(handler) = handler.as_ref() {
                    handler(mouse_event);
                }
            }
        }
    }
);

impl MetalView {
    /// Creates a new MetalView with the given frame
    pub fn new(mtm: MainThreadMarker, frame: NSRect) -> Retained<Self> {
        // Create the view with default ivars (which initializes Metal)
        let this = mtm.alloc::<Self>();
        let this = this.set_ivars(MetalViewIvars::default());

        let this: Retained<Self> = unsafe { msg_send![super(this), initWithFrame: frame] };

        // Enable layer backing
        this.setWantsLayer(true);

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

    /// Sets the key event handler callback
    ///
    /// The handler will be called for each keyDown event, with the
    /// NSEvent converted to our Rust-native KeyEvent type.
    pub fn set_key_handler(&self, handler: impl Fn(KeyEvent) + 'static) {
        *self.ivars().key_handler.borrow_mut() = Some(Box::new(handler));
    }

    /// Sets the mouse event handler callback
    ///
    /// The handler will be called for each mouseDown event, with the
    /// NSEvent converted to our Rust-native MouseEvent type.
    pub fn set_mouse_handler(&self, handler: impl Fn(MouseEvent) + 'static) {
        *self.ivars().mouse_handler.borrow_mut() = Some(Box::new(handler));
    }

    /// Converts an NSEvent to our KeyEvent type
    fn convert_key_event(&self, event: &NSEvent) -> Option<KeyEvent> {
        let modifiers = self.convert_modifiers(event);
        let key = self.convert_key(event)?;
        Some(KeyEvent::new(key, modifiers))
    }

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

        Some(MouseEvent {
            kind,
            position,
            modifiers,
        })
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
        // To correctly handle Ctrl+key combinations like Ctrl+A and Ctrl+E for
        // Emacs-style line navigation, we use charactersIgnoringModifiers() when
        // Control is active. This returns the unmodified base character ('a', 'e',
        // etc.) regardless of what control character macOS would normally produce.
        let flags = event.modifierFlags();
        let characters = if flags.contains(NSEventModifierFlags::Control) {
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
