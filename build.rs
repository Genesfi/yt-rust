use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=assets/app.ico");
    println!("cargo:rerun-if-changed=assets/app.rc");

    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os != "windows" {
        return;
    }

    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
    let res_path = out_dir.join("app.res");

    let Some(rc_exe) = find_rc_exe() else {
        println!("cargo:warning=rc.exe not found; skipping Windows EXE icon resource");
        return;
    };

    let status = std::process::Command::new(&rc_exe)
        .args(["/nologo", "/fo"])
        .arg(&res_path)
        .arg("assets/app.rc")
        .status();

    match status {
        Ok(s) if s.success() && res_path.is_file() => {
            println!("cargo:rustc-link-arg-bins={}", res_path.display());
        }
        Ok(s) => {
            println!("cargo:warning=rc.exe exited with status {s}");
        }
        Err(e) => {
            println!("cargo:warning=failed to run rc.exe: {e}");
        }
    }
}

fn find_rc_exe() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path) {
            let cand = dir.join("rc.exe");
            if cand.is_file() {
                return Some(cand);
            }
        }
    }
    let kits_bin = PathBuf::from(r"C:\Program Files (x86)\Windows Kits\10\bin");
    if let Ok(entries) = std::fs::read_dir(kits_bin) {
        let mut versions: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path().join("x64").join("rc.exe"))
            .filter(|p| p.is_file())
            .collect();
        versions.sort();
        if let Some(latest) = versions.pop() {
            return Some(latest);
        }
    }
    None
}
