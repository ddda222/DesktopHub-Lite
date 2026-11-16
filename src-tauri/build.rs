fn main() {
    println!("cargo:rerun-if-changed=D:/codex/DesktopHub-work/desktophub-lite-dist");
    tauri_build::build()
}
