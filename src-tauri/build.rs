fn main() {
    if std::env::var("TARGET").is_ok_and(|target| target.contains("apple-darwin")) {
        println!("cargo:rustc-link-arg=-Wl,-rpath,/usr/lib/swift");
        if let Ok(output) = std::process::Command::new("xcode-select")
            .arg("-p")
            .output()
        {
            if output.status.success() {
                let developer_dir = String::from_utf8_lossy(&output.stdout).trim().to_string();
                let swift_runtime = if developer_dir.ends_with("CommandLineTools") {
                    format!("{developer_dir}/usr/lib/swift-5.5/macosx")
                } else {
                    format!(
                        "{developer_dir}/Toolchains/XcodeDefault.xctoolchain/usr/lib/swift-5.5/macosx"
                    )
                };
                println!("cargo:rustc-link-arg=-Wl,-rpath,{swift_runtime}");
            }
        }
    }
    tauri_build::build()
}
