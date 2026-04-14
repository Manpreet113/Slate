use crate::preflight;
use crate::system;
use crate::tui::{self, InstallMsg, UserInfo};
use anyhow::{bail, Context, Result};
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::thread::sleep;
use std::time::Duration;

/// The entry point for `slate install`
pub fn forge() -> Result<()> {
    // 1. Discover devices
    let devices = system::list_block_devices().context("Failed to list block devices")?;
    if devices.is_empty() {
        bail!("No block devices found for installation!");
    }

    // 2. Run the Ratatui Installer
    tui::run_installer(devices, background_installer)?;

    Ok(())
}

/// The actual installation logic running in a background thread
fn background_installer(device: system::BlockDevice, user_info: UserInfo, tx: Sender<InstallMsg>) {
    let dev_path = &device.path;

    let res = (|| -> Result<()> {
        tx.send(InstallMsg::Log(
            "Initializing Preflight Checks...".to_string(),
        ))?;
        tx.send(InstallMsg::Progress(2))?;

        // 1. Preflight
        let preflight_logs = preflight::run_checks(dev_path)?;
        for log in preflight_logs {
            tx.send(InstallMsg::Log(log))?;
        }
        tx.send(InstallMsg::Progress(10))?;
        
        // 1.5 Optimize Mirrors & Pacman
        optimize_mirrors(&tx)?;
        tx.send(InstallMsg::Progress(15))?;

        // 2. Partitioning
        tx.send(InstallMsg::Log("Partitioning disk...".to_string()))?;
        cleansing(dev_path, &tx)?;
        tx.send(InstallMsg::Progress(20))?;

        // 3. Btrfs Setup
        let mut guard = MountGuard::new(&tx);
        subvolume_dance(dev_path, &mut guard, &tx)?;
        tx.send(InstallMsg::Progress(30))?;

        // 4. Injection (Pacstrap)
        injection(&tx)?;
        tx.send(InstallMsg::Progress(65))?;

        // 5. Save User Info
        tx.send(InstallMsg::Log("Saving user configuration...".to_string()))?;
        let user_info_path = Path::new("/mnt/root/user_info.json");
        let user_info_json = serde_json::to_string(&user_info)?;
        fs::write(user_info_path, user_info_json)?;

        // 6. Chroot Stage
        tx.send(InstallMsg::Log("Entering Chroot Stage...".to_string()))?;
        run_cmd_captured("arch-chroot", &["/mnt", "slate", "chroot-stage"], &tx)?;

        let _ = fs::remove_file(user_info_path);
        tx.send(InstallMsg::Progress(100))?;
        tx.send(InstallMsg::Finished)?;

        Ok(())
    })();

    if let Err(e) = res {
        let msg = sanitize_for_tui(&format!("{:#}", e));
        let _ = tx.send(InstallMsg::Error(msg));
    }
}

fn cleansing(device: &str, tx: &Sender<InstallMsg>) -> Result<()> {
    run_cmd_captured("sgdisk", &["--zap-all", device], tx)?;
    run_cmd_captured("sgdisk", &["-n", "1:0:+1G", "-t", "1:ef00", device], tx)?;
    run_cmd_captured("sgdisk", &["-n", "2:0:0", "-t", "2:8300", device], tx)?;

    let efi_part = resolve_partition(device, 1);
    wait_for_partition(&efi_part, tx)?;
    run_cmd_captured("mkfs.vfat", &["-F32", "-n", "EFI", &efi_part], tx)?;

    let root_part = resolve_partition(device, 2);
    wait_for_partition(&root_part, tx)?;
    run_cmd_captured("mkfs.btrfs", &["-f", "-L", "Arch", &root_part], tx)?;

    Ok(())
}

fn wait_for_partition(part_path: &str, tx: &Sender<InstallMsg>) -> Result<()> {
    for _ in 0..50 {
        if Path::new(part_path).exists() {
            return Ok(());
        }
        sleep(Duration::from_millis(200));
    }
    let _ = tx.send(InstallMsg::Log(format!(
        "[Err] Partition node not ready: {}",
        part_path
    )));
    bail!("Partition node not ready: {}", part_path)
}

fn subvolume_dance(device: &str, guard: &mut MountGuard, tx: &Sender<InstallMsg>) -> Result<()> {
    let root_part = resolve_partition(device, 2);
    guard.mount(&root_part, "/mnt", &[])?;

    run_cmd_captured("btrfs", &["subvolume", "create", "/mnt/@"], tx)?;
    run_cmd_captured("btrfs", &["subvolume", "create", "/mnt/@home"], tx)?;
    run_cmd_captured("btrfs", &["subvolume", "create", "/mnt/@log"], tx)?;
    run_cmd_captured("btrfs", &["subvolume", "create", "/mnt/@pkg"], tx)?;
    run_cmd_captured("btrfs", &["subvolume", "create", "/mnt/@snapshots"], tx)?;

    guard.unmount("/mnt")?;

    let mount_opts = "rw,noatime,compress=zstd,discard=async,space_cache=v2";
    guard.mount(
        &root_part,
        "/mnt",
        &["-o", &format!("subvol=@,{}", mount_opts)],
    )?;

    fs::create_dir_all("/mnt/home")?;
    fs::create_dir_all("/mnt/var/log")?;
    fs::create_dir_all("/mnt/var/cache/pacman/pkg")?;
    fs::create_dir_all("/mnt/.snapshots")?;
    fs::create_dir_all("/mnt/boot")?;

    guard.mount(
        &root_part,
        "/mnt/home",
        &["-o", &format!("subvol=@home,{}", mount_opts)],
    )?;
    guard.mount(
        &root_part,
        "/mnt/var/log",
        &["-o", &format!("subvol=@log,{}", mount_opts)],
    )?;
    guard.mount(
        &root_part,
        "/mnt/var/cache/pacman/pkg",
        &["-o", &format!("subvol=@pkg,{}", mount_opts)],
    )?;
    guard.mount(
        &root_part,
        "/mnt/.snapshots",
        &["-o", &format!("subvol=@snapshots,{}", mount_opts)],
    )?;

    let efi_part = resolve_partition(device, 1);
    guard.mount(&efi_part, "/mnt/boot", &[])?;

    Ok(())
}

fn injection(tx: &Sender<InstallMsg>) -> Result<()> {
    // Keep pacstrap minimal and reliable. Non-essential desktop/tooling packages
    // are installed later via Ax as the regular user in chroot.
    let packages = [
        "base",
        "base-devel",
        "linux",
        "linux-firmware",
        "intel-ucode",
        "amd-ucode",
        "btrfs-progs",
        "libgit2",
        "sudo",
        "networkmanager",
        "bluez",
        "bluez-utils",
        "git",
        "zsh",
        "curl",
    ];

    tx.send(InstallMsg::Log(
        "Updating Arch Linux Keyring...".to_string(),
    ))?;
    let _ = run_cmd_captured("pacman", &["-Sy", "archlinux-keyring", "--noconfirm"], tx);

    tx.send(InstallMsg::Log(
        "[Phase 1/3] pacstrap essentials".to_string(),
    ))?;
    tx.send(InstallMsg::Log(
        "Starting Pacstrap (Essentials)...".to_string(),
    ))?;
    let mut args = vec!["-K", "/mnt"];
    args.extend(packages.iter());
    run_cmd_captured("pacstrap", &args, tx)?;

    tx.send(InstallMsg::Log("Generating fstab...".to_string()))?;
    let output = Command::new("genfstab").args(["-U", "/mnt"]).output()?;
    fs::write("/mnt/etc/fstab", output.stdout)?;

    tx.send(InstallMsg::Log(
        "Injecting binaries (Slate & Ax)...".to_string(),
    ))?;
    let current_exe = std::env::current_exe()?;
    fs::copy(&current_exe, "/mnt/usr/local/bin/slate")?;

    run_cmd_captured(
        "curl",
        &[
            "-L",
            "https://github.com/manpreet113/ax/releases/latest/download/ax",
            "-o",
            "/mnt/usr/local/bin/ax",
        ],
        tx,
    )?;
    run_cmd_captured(
        "chmod",
        &["+x", "/mnt/usr/local/bin/ax", "/mnt/usr/local/bin/slate"],
        tx,
    )?;

    Ok(())
}

fn run_cmd_captured(cmd: &str, args: &[&str], tx: &Sender<InstallMsg>) -> Result<()> {
    let _ = tx.send(InstallMsg::Log(format!("$ {} {}", cmd, args.join(" "))));

    let mut child = Command::new(cmd)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let stdout = child.stdout.take().context("Failed to open stdout")?;
    let stderr = child.stderr.take().context("Failed to open stderr")?;

    let tx_out = tx.clone();
    let tx_err = tx.clone();
    let stderr_lines: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let stderr_lines_for_thread = Arc::clone(&stderr_lines);

    let out_handle = std::thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines() {
            if let Ok(l) = line {
                let clean = sanitize_for_tui(&l);
                if !clean.is_empty() {
                    let _ = tx_out.send(InstallMsg::Log(clean));
                }
            }
        }
    });

    let err_handle = std::thread::spawn(move || {
        let reader = BufReader::new(stderr);
        for line in reader.lines() {
            if let Ok(l) = line {
                let clean = sanitize_for_tui(&l);
                if clean.is_empty() {
                    continue;
                }
                if let Ok(mut captured) = stderr_lines_for_thread.lock() {
                    captured.push(clean.clone());
                    if captured.len() > 200 {
                        let drop_n = captured.len() - 200;
                        captured.drain(0..drop_n);
                    }
                }
                let _ = tx_err.send(InstallMsg::Log(format!("[Err] {}", clean)));
            }
        }
    });

    let status = child.wait()?;
    let _ = out_handle.join();
    let _ = err_handle.join();

    if !status.success() {
        let code = status
            .code()
            .map_or("signal".to_string(), |c| c.to_string());
        let stderr_tail = stderr_lines
            .lock()
            .ok()
            .map(|v| v.iter().rev().take(5).cloned().collect::<Vec<_>>())
            .unwrap_or_default()
            .into_iter()
            .rev()
            .collect::<Vec<_>>();

        if stderr_tail.is_empty() {
            bail!("Command failed: {} (exit {})", cmd, code);
        }

        bail!(
            "Command failed: {} (exit {})\nRecent stderr:\n{}",
            cmd,
            code,
            stderr_tail.join("\n")
        );
    }
    Ok(())
}

fn sanitize_for_tui(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '\u{1b}' {
            if let Some('[') = chars.peek().copied() {
                let _ = chars.next();
                while let Some(next) = chars.next() {
                    if ('@'..='~').contains(&next) {
                        break;
                    }
                }
                continue;
            }
            if let Some(']') = chars.peek().copied() {
                let _ = chars.next();
                while let Some(next) = chars.next() {
                    if next == '\u{7}' {
                        break;
                    }
                }
                continue;
            }
            continue;
        }

        if ch == '\r' || (ch.is_control() && ch != '\n' && ch != '\t') {
            continue;
        }

        out.push(ch);
    }

    out
}

struct MountGuard<'a> {
    mounts: Vec<PathBuf>,
    tx: &'a Sender<InstallMsg>,
}

impl<'a> MountGuard<'a> {
    fn new(tx: &'a Sender<InstallMsg>) -> Self {
        Self {
            mounts: Vec::new(),
            tx,
        }
    }

    fn mount(&mut self, source: &str, target: &str, options: &[&str]) -> Result<()> {
        let _ = self.tx.send(InstallMsg::Log(format!(
            "Mounting {} -> {}",
            source, target
        )));
        let status = Command::new("mount")
            .args(options)
            .arg(source)
            .arg(target)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()?;
        if !status.success() {
            bail!("Mount failed");
        }
        self.mounts.push(PathBuf::from(target));
        Ok(())
    }

    fn unmount(&mut self, target: &str) -> Result<()> {
        let _ = self
            .tx
            .send(InstallMsg::Log(format!("Unmounting {}", target)));
        let status = Command::new("umount")
            .arg(target)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()?;
        if !status.success() {
            bail!("Unmount failed");
        }
        if let Some(pos) = self.mounts.iter().position(|p| p == Path::new(target)) {
            self.mounts.remove(pos);
        }
        Ok(())
    }
}

impl<'a> Drop for MountGuard<'a> {
    fn drop(&mut self) {
        for m in self.mounts.iter().rev() {
            let _ = Command::new("umount").arg("-l").arg(m).status();
        }
    }
}

fn resolve_partition(device: &str, part_num: i32) -> String {
    if device.contains("nvme") || device.contains("mmcblk") {
        format!("{}p{}", device, part_num)
    } else {
        format!("{}{}", device, part_num)
    }
}

fn optimize_mirrors(tx: &Sender<InstallMsg>) -> Result<()> {
    tx.send(InstallMsg::Log(
        "Optimizing Mirrors & Pacman...".to_string(),
    ))?;

    // 1. Enable ParallelDownloads & DisableDownloadTimeout in host
    let pacman_conf = "/etc/pacman.conf";
    if Path::new(pacman_conf).exists() {
        let content = fs::read_to_string(pacman_conf)?;
        let mut updated = content.clone();
        
        // Parallel Downloads
        if content.contains("#ParallelDownloads") {
            updated = updated.replace("#ParallelDownloads", "ParallelDownloads");
        } else if !content.contains("ParallelDownloads") {
            updated.push_str("\nParallelDownloads = 5\n");
        }

        // Disable Download Timeout
        if !content.contains("DisableDownloadTimeout") {
            if let Some(pos) = updated.find("[options]") {
                if let Some(line_end) = updated[pos..].find('\n') {
                    updated.insert_str(pos + line_end + 1, "DisableDownloadTimeout\n");
                }
            } else {
                updated.push_str("\nDisableDownloadTimeout\n");
            }
        }
        
        if updated != content {
            let _ = fs::write(pacman_conf, updated);
        }
    }

    // 2. Prepend known reliable global mirrors as a safety net
    let mirrorlist_path = "/etc/pacman.d/mirrorlist";
    let reliable_mirrors = "## Bulletproof Fallbacks\nServer = https://mirrors.edge.kernel.org/archlinux/$repo/os/$arch\nServer = https://mirror.rackspace.com/archlinux/$repo/os/$arch\nServer = https://mirrors.rit.edu/archlinux/$repo/os/$arch\n\n";
    
    let current_mirrors = fs::read_to_string(mirrorlist_path).unwrap_or_default();
    let _ = fs::write(mirrorlist_path, format!("{}{}", reliable_mirrors, current_mirrors));

    // 3. Turbo-rank top 10 mirrors (excluding the problematic pkgbuild.com)
    run_cmd_captured(
        "reflector",
        &[
            "--latest",
            "10",
            "--protocol",
            "https",
            "--sort",
            "rate",
            "--threads",
            "10",
            "--download-timeout",
            "10",
            "--country",
            "India,Singapore,United States",
            "--exclude",
            "pkgbuild.com",
            "--save",
            "/etc/pacman.d/mirrorlist",
        ],
        tx,
    )?;

    // 4. Prepend reliable mirrors AGAIN if reflector overwrote the list
    let final_mirrors = fs::read_to_string(mirrorlist_path).unwrap_or_default();
    if !final_mirrors.contains("Bulletproof Fallbacks") {
        let _ = fs::write(mirrorlist_path, format!("{}{}", reliable_mirrors, final_mirrors));
    }

    Ok(())
}
