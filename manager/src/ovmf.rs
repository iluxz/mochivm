//! OVMF (UEFI firmware) discovery + download.

use std::path::PathBuf;
use std::process::Command;

const OVMF_URL: &str = "https://github.com/rust-osdev/ovmf-prebuilt/releases/download/edk2-stable202605-r1/edk2-stable202605-r1-bin.tar.xz";

/// Search common locations for OVMF code/vars pairs.
pub fn detect() -> Option<(PathBuf, PathBuf)> {
    let exe_dir = std::env::current_exe().ok()?.parent()?.to_path_buf();

    let mut candidates: Vec<(PathBuf, PathBuf)> = Vec::new();
    if cfg!(windows) {
        candidates.push((
            exe_dir.join("ovmf/x64/code.fd"),
            exe_dir.join("ovmf/x64/vars.fd"),
        ));
    } else {
        candidates.push((
            PathBuf::from("/usr/share/OVMF/OVMF_CODE_4M.fd"),
            PathBuf::from("/usr/share/OVMF/OVMF_VARS_4M.fd"),
        ));
        candidates.push((
            PathBuf::from("/usr/share/OVMF/OVMF_CODE.fd"),
            PathBuf::from("/usr/share/OVMF/OVMF_VARS.fd"),
        ));
    }

    candidates
        .into_iter()
        .find(|(c, v)| c.exists() && v.exists())
}

/// Download + extract the ovmf-prebuilt release next to the executable
/// (creates `<exe_dir>/ovmf/x64/{code,vars}.fd`).
pub fn download() -> Result<(PathBuf, PathBuf), String> {
    let exe_dir = std::env::current_exe()
        .map_err(|e| format!("current_exe: {e}"))?
        .parent()
        .ok_or("no exe parent dir")?
        .to_path_buf();

    let target = exe_dir.join("ovmf");
    let tmp = std::env::temp_dir().join("mochivm-ovmf-bin.tar.xz");

    std::fs::create_dir_all(&target).map_err(|e| format!("mkdir: {e}"))?;

    let status = Command::new("curl")
        .args(["-sL", "-o"])
        .arg(&tmp)
        .arg(OVMF_URL)
        .status()
        .map_err(|e| format!("curl not found: {e}"))?;
    if !status.success() {
        return Err("curl download failed".into());
    }

    let status = Command::new("tar")
        .args(["-xf"])
        .arg(&tmp)
        .arg("-C")
        .arg(&target)
        .status()
        .map_err(|e| format!("tar not found: {e}"))?;
    if !status.success() {
        return Err("tar extract failed".into());
    }

    let code = find_file(&target, "code.fd").ok_or("code.fd not in archive")?;
    let vars = find_file(&target, "vars.fd").ok_or("vars.fd not in archive")?;
    Ok((code, vars))
}

/// Recursively find a file by name under `root` (the ovmf-prebuilt archives
/// nest the files under a versioned directory).
fn find_file(root: &PathBuf, name: &str) -> Option<PathBuf> {
    for entry in std::fs::read_dir(root).ok()? {
        let path = entry.ok()?.path();
        if path.is_dir() {
            if let Some(found) = find_file(&path, name) {
                return Some(found);
            }
        } else if path.file_name().and_then(|n| n.to_str()) == Some(name) {
            return Some(path);
        }
    }
    None
}
