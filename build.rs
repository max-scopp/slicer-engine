fn main() {
    // Print cargo instructions
    println!(
        "cargo:rustc-env=PROFILE={}",
        std::env::var("PROFILE").unwrap()
    );

    // Platform-specific settings
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();

    match target_os.as_str() {
        "windows" => {
            println!("cargo:rustc-env=PLATFORM_NAME=windows");
            println!("cargo:rustc-env=PLATFORM_ARCH={}", target_arch);
        }
        "macos" => {
            println!("cargo:rustc-env=PLATFORM_NAME=macos");
            println!("cargo:rustc-env=PLATFORM_ARCH={}", target_arch);
        }
        "unknown" if target_arch == "wasm32" => {
            println!("cargo:rustc-env=PLATFORM_NAME=wasm");
            println!("cargo:rustc-env=PLATFORM_ARCH=wasm32");
        }
        _ => {
            println!("cargo:rustc-env=PLATFORM_NAME=unknown");
            println!("cargo:rustc-env=PLATFORM_ARCH={}", target_arch);
        }
    }
}
