// Chunk: docs/chunks/metal_surface - macOS window + Metal surface foundation

fn main() {
    // Link macOS frameworks required for Metal rendering
    println!("cargo:rustc-link-lib=framework=AppKit");
    println!("cargo:rustc-link-lib=framework=Metal");
    println!("cargo:rustc-link-lib=framework=QuartzCore");
    println!("cargo:rustc-link-lib=framework=Foundation");

    // Ensure we rebuild if build.rs changes
    println!("cargo:rerun-if-changed=build.rs");
}
