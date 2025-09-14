fn main() {
    tauri_build::build();

    // Link to the LAME library
    println!("cargo:rustc-link-lib=mp3lame");

    // Add library search path for common locations
    if cfg!(target_os = "macos") {
        // Homebrew path
        println!("cargo:rustc-link-search=/opt/homebrew/lib");
        println!("cargo:rustc-link-search=/usr/local/lib");
    } else if cfg!(target_os = "linux") {
        println!("cargo:rustc-link-search=/usr/lib");
        println!("cargo:rustc-link-search=/usr/local/lib");
    } else if cfg!(target_os = "windows") {
        // You might need to adjust this path based on your LAME installation
        println!("cargo:rustc-link-search=C:/lame/lib");
    }
}
