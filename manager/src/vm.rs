//! QEMU backend: builds the boot disk, spawns qemu, and pipes its serial
//! output back to the gui.

use crate::fat;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Accel {
    Tcg,
    Kvm,
    Whpx,
}

impl Accel {
    pub fn label(self) -> &'static str {
        match self {
            Accel::Tcg => "TCG (software, everywhere)",
            Accel::Kvm => "KVM (linux, needs /dev/kvm)",
            Accel::Whpx => "WHPX (windows, needs hyper-v)",
        }
    }
}

#[derive(Clone)]
pub struct VmConfig {
    pub name: String,
    pub ram_mb: u16,
    pub vcpus: u8,
    pub accel: Accel,
}

impl Default for VmConfig {
    fn default() -> Self {
        Self {
            name: "mochivm".into(),
            ram_mb: 512,
            vcpus: 1,
            accel: Accel::Tcg,
        }
    }
}

pub struct Backend {
    child: Option<Child>,
    rx: Option<Receiver<String>>,
    run_dir: PathBuf,
}

impl Backend {
    pub fn new(run_dir: PathBuf) -> Self {
        Self {
            child: None,
            rx: None,
            run_dir,
        }
    }

    pub fn start(
        &mut self,
        cfg: &VmConfig,
        efi: &Path,
        code: &Path,
        vars: &Path,
    ) -> Result<(), String> {
        self.stop();
        std::fs::create_dir_all(&self.run_dir).map_err(|e| format!("mkdir: {e}"))?;

        let disk = self.run_dir.join("disk.img");
        let vars_copy = self.run_dir.join("vars.fd");

        let efi_bytes = std::fs::read(efi).map_err(|e| format!("read efi {efi:?}: {e}"))?;
        fat::write_fat32_disk(&efi_bytes, &disk)?;
        std::fs::copy(vars, &vars_copy).map_err(|e| format!("copy ovmf vars: {e}"))?;

        let mut cmd = Command::new(qemu_bin());
        cmd.arg("-machine")
            .arg("q35")
            .arg("-m")
            .arg(format!("{}M", cfg.ram_mb))
            .arg("-smp")
            .arg(cfg.vcpus.to_string());
        match cfg.accel {
            Accel::Tcg => {
                cmd.arg("-accel").arg("tcg").arg("-cpu").arg("max");
            }
            Accel::Kvm => {
                cmd.arg("-enable-kvm").arg("-cpu").arg("host");
            }
            Accel::Whpx => {
                cmd.arg("-accel").arg("whpx").arg("-cpu").arg("max");
            }
        }
        cmd.arg("-drive")
            .arg(format!(
                "if=pflash,format=raw,readonly=on,file={}",
                code.display()
            ))
            .arg("-drive")
            .arg(format!("if=pflash,format=raw,file={}", vars_copy.display()))
            .arg("-drive")
            .arg(format!("if=virtio,format=raw,file={}", disk.display()))
            .arg("-serial")
            .arg("stdio")
            .arg("-monitor")
            .arg("none")
            .arg("-display")
            .arg("none")
            .arg("-no-reboot")
            .arg("-no-shutdown")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = cmd
            .spawn()
            .map_err(|e| format!("failed to start qemu ({e}) - is qemu-system-x86_64 on PATH?"))?;

        let (tx, rx) = channel();
        let stdout = child.stdout.take().ok_or("no stdout")?;
        let stderr = child.stderr.take().ok_or("no stderr")?;
        spawn_reader(stdout, tx.clone());
        spawn_reader(stderr, tx);

        self.child = Some(child);
        self.rx = Some(rx);
        Ok(())
    }

    pub fn stop(&mut self) {
        if let Some(mut c) = self.child.take() {
            let _ = c.kill();
            let _ = c.wait();
            self.rx = None;
        }
    }

    pub fn is_running(&mut self) -> bool {
        match &mut self.child {
            Some(c) => matches!(c.try_wait(), Ok(None)),
            None => false,
        }
    }

    /// Drain any pending serial output since the last poll.
    pub fn drain_log(&self) -> String {
        let mut out = String::new();
        if let Some(rx) = &self.rx {
            while let Ok(s) = rx.try_recv() {
                out.push_str(&s);
            }
        }
        out
    }
}

impl Drop for Backend {
    fn drop(&mut self) {
        self.stop();
    }
}

fn spawn_reader<R: Read + Send + 'static>(mut r: R, tx: Sender<String>) {
    thread::spawn(move || {
        let mut buf = [0u8; 1024];
        loop {
            match r.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    let s = String::from_utf8_lossy(&buf[..n]).into_owned();
                    if tx.send(s).is_err() {
                        break;
                    }
                }
            }
        }
    });
}

fn qemu_bin() -> PathBuf {
    if let Ok(p) = std::env::var("MOCHIVM_QEMU") {
        if !p.is_empty() {
            return PathBuf::from(p);
        }
    }
    if cfg!(windows) {
        for p in [
            "C:\\Program Files\\qemu\\qemu-system-x86_64.exe",
            "C:\\Program Files (x86)\\qemu\\qemu-system-x86_64.exe",
            "C:\\qemu\\qemu-system-x86_64.exe",
        ] {
            let b = PathBuf::from(p);
            if b.exists() {
                return b;
            }
        }
    }
    PathBuf::from("qemu-system-x86_64")
}
