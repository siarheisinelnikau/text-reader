use std::{env, fs, path::PathBuf, process::Command};

fn resource_compiler() -> PathBuf {
    let host = env::var("HOST").unwrap_or_default();
    let arch = if host.starts_with("aarch64") {
        "arm64"
    } else if host.starts_with("i686") {
        "x86"
    } else {
        "x64"
    };
    if let Some(bin) = env::var_os("WindowsSdkVerBinPath") {
        let path = PathBuf::from(bin).join(arch).join("rc.exe");
        if path.is_file() {
            return path;
        }
    }
    let root = env::var_os("WindowsSdkDir")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Program Files (x86)\Windows Kits\10"));
    let bin = root.join("bin");
    let mut versions = fs::read_dir(&bin)
        .expect("Install the Windows SDK resource compiler (rc.exe)")
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let version = name
                .split('.')
                .map(str::parse::<u32>)
                .collect::<Result<Vec<_>, _>>()
                .ok()?;
            let compiler = entry.path().join(arch).join("rc.exe");
            compiler.is_file().then_some((version, compiler))
        })
        .collect::<Vec<_>>();
    versions.sort_by(|a, b| b.0.cmp(&a.0));
    versions
        .into_iter()
        .next()
        .expect("No Windows SDK rc.exe was found")
        .1
}

fn main() {
    for path in [
        "assets/windows.rc",
        "assets/text-reader.ico",
        "assets/icon.png",
    ] {
        println!("cargo:rerun-if-changed={path}");
    }
    println!("cargo:rerun-if-env-changed=WindowsSdkVerBinPath");
    println!("cargo:rerun-if-env-changed=WindowsSdkDir");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    assert_eq!(
        env::var("CARGO_CFG_TARGET_ENV").as_deref(),
        Ok("msvc"),
        "TextReader's Windows resource build requires the MSVC toolchain"
    );
    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let output = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("text-reader.res");
    let status = Command::new(resource_compiler())
        .current_dir(manifest)
        .arg("/nologo")
        .arg("/fo")
        .arg(&output)
        .arg("assets/windows.rc")
        .status()
        .expect("Cannot run the Windows resource compiler");
    assert!(
        status.success(),
        "Windows application icon resource compilation failed"
    );
    println!("cargo:rustc-link-arg-bin=text-reader={}", output.display());
}
