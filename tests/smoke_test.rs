// Chunk: docs/chunks/metal_surface - macOS window + Metal surface foundation
//!
//! Smoke test for the Metal surface implementation
//!
//! This test verifies that the basic Metal and AppKit initialization
//! code paths work without panicking. Full visual verification must
//! be done manually.

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_foundation::MainThreadMarker;
use objc2_metal::MTLDevice;

/// Test that we can get a Metal device
#[test]
fn test_metal_device_available() {
    extern "C" {
        fn MTLCreateSystemDefaultDevice() -> *mut ProtocolObject<dyn MTLDevice>;
    }

    let ptr = unsafe { MTLCreateSystemDefaultDevice() };
    assert!(!ptr.is_null(), "Metal device should be available on macOS");

    // Clean up
    let _device: Retained<ProtocolObject<dyn MTLDevice>> =
        unsafe { Retained::from_raw(ptr).unwrap() };
}

/// Test that we can create a Metal command queue
#[test]
fn test_metal_command_queue() {
    extern "C" {
        fn MTLCreateSystemDefaultDevice() -> *mut ProtocolObject<dyn MTLDevice>;
    }

    let ptr = unsafe { MTLCreateSystemDefaultDevice() };
    assert!(!ptr.is_null());

    let device: Retained<ProtocolObject<dyn MTLDevice>> =
        unsafe { Retained::from_raw(ptr).unwrap() };

    let command_queue = device.newCommandQueue();
    assert!(
        command_queue.is_some(),
        "Should be able to create a command queue"
    );
}

/// Test that we can access the main thread marker
/// Note: This test must run on the main thread to pass
#[test]
#[ignore = "Requires main thread - run with --ignored to test"]
fn test_main_thread_marker() {
    let mtm = MainThreadMarker::new();
    assert!(
        mtm.is_some(),
        "Should be on main thread when running this test"
    );
}
