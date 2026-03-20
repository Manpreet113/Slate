use crate::preflight;
use crate::system;
use crate::tui::{self, InstallMsg, UserInfo};
use anyhow::{bail, Context, Result};
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::Sender;

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
        tx.send(InstallMsg::Log("Initializing Preflight Checks...".to_string()))?;
        tx.send(InstallMsg::Progress(2))?;

        // 1. Preflight
        let preflight_logs = preflight::run_checks(dev_path)?;
        for log in preflight_logs {
            tx.send(InstallMsg::Log(log))?;
        }
        tx.send(InstallMsg::Progress(10))?;

        // 2. Partitioning
        tx.send(InstallMsg::Log("Partitioning disk...".to_string()))?;
        cleansing(dev_path, &tx)?;
        tx.send(InstallMsg::Progress(25))?;

        // 3. Btrfs Setup
        let mut guard = MountGuard::new(&tx);
        subvolume_dance(dev_path, &mut guard, &tx)?;
        tx.send(InstallMsg::Progress(40))?;

        // 4. Injection (Pacstrap)
        injection(&tx)?;
        tx.send(InstallMsg::Progress(70))?;

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
        let _ = tx.send(InstallMsg::Error(format!("{:?}", e)));
    }
}

fn cleansing(device: &str, tx: &Sender<InstallMsg>) -> Result<()> {
    run_cmd_captured("sgdisk", &["--zap-all", device], tx)?;
    run_cmd_captured("sgdisk", &["-n", "1:0:+1G", "-t", "1:ef00", device], tx)?;
    run_cmd_captured("sgdisk", &["-n", "2:0:0", "-t", "2:8300", device], tx)?;

    let efi_part = resolve_partition(device, 1);
    run_cmd_captured("mkfs.vfat", &["-F32", "-n", "EFI", &efi_part], tx)?;

    let root_part = resolve_partition(device, 2);
    run_cmd_captured("mkfs.btrfs", &["-f", "-L", "Arch", &root_part], tx)?;

    Ok(())
}

fn subvolume_dance(device: &str, guard: &mut MountGuard, tx: &Sender<InstallMsg>) -> Result<()> {
    let root_part = resolve_partition(device, 2);
    guard.mount(&root_part, "/mnt", &[])?;

    run_cmd_captured("btrfs", &["subvolume", "create", "/mnt/@"], tx)?;
    run_cmd_captured("btrfs", &["subvolume", "create", "/mnt/@home"], tx)?;
    run_cmd_captured("btrfs", &["subvolume", "create", "/mnt/@pkg"], tx)?;

    guard.unmount("/mnt")?;

    let mount_opts = "rw,noatime,compress=zstd,discard=async,space_cache=v2";
    guard.mount(&root_part, "/mnt", &["-o", &format!("subvol=@,{}", mount_opts)])?;
    
    fs::create_dir_all("/mnt/home")?;
    fs::create_dir_all("/mnt/var/cache/pacman/pkg")?;
    fs::create_dir_all("/mnt/boot")?;

    guard.mount(&root_part, "/mnt/home", &["-o", &format!("subvol=@home,{}", mount_opts)])?;
    guard.mount(&root_part, "/mnt/var/cache/pacman/pkg", &["-o", &format!("subvol=@pkg,{}", mount_opts)])?;

    let efi_part = resolve_partition(device, 1);
    guard.mount(&efi_part, "/mnt/boot", &[])?;

    Ok(())
}

fn injection(tx: &Sender<InstallMsg>) -> Result<()> {
    run_cmd_captured("pacstrap", &["-K", "/mnt", "base", "base-devel", "linux", "linux-firmware", "intel-ucode", "amd-ucode", "btrfs-progs", "networkmanager"], tx)?;

    tx.send(InstallMsg::Log("Generating fstab...".to_string()))?;
    let output = Command::new("genfstab").args(["-U", "/mnt"]).output()?;
    fs::write("/mnt/etc/fstab", output.stdout)?;

    tx.send(InstallMsg::Log("Injecting binaries...".to_string()))?;
    let current_exe = std::env::current_exe()?;
    fs::copy(&current_exe, "/mnt/usr/local/bin/slate")?;
    
    run_cmd_captured("curl", &["-L", "https://github.com/manpreet113/ax/releases/latest/download/ax", "-o", "/mnt/usr/local/bin/ax"], tx)?;
    run_cmd_captured("chmod", &["+x", "/mnt/usr/local/bin/ax", "/mnt/usr/local/bin/slate"], tx)?;

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

    std::thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines() {
            if let Ok(l) = line {
                let _ = tx_out.send(InstallMsg::Log(l));
            }
        }
    });

    std::thread::spawn(move || {
        let reader = BufReader::new(stderr);
        for line in reader.lines() {
            if let Ok(l) = line {
                let _ = tx_err.send(InstallMsg::Log(format!("[Err] {}", l)));
            }
        }
    });

    let status = child.wait()?;
    if !status.success() {
        bail!("Command failed: {}", cmd);
    }
    Ok(())
}

struct MountGuard<'a> {
    mounts: Vec<PathBuf>,
    tx: &'a Sender<InstallMsg>,
}

impl<'a> MountGuard<'a> {
    fn new(tx: &'a Sender<InstallMsg>) -> Self {
        Self { mounts: Vec::new(), tx }
    }

    fn mount(&mut self, source: &str, target: &str, options: &[&str]) -> Result<()> {
        let _ = self.tx.send(InstallMsg::Log(format!("Mounting {} -> {}", source, target)));
        let status = Command::new("mount")
            .args(options)
            .arg(source)
            .arg(target)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()?;
        if !status.success() { bail!("Mount failed"); }
        self.mounts.push(PathBuf::from(target));
        Ok(())
    }

    fn unmount(&mut self, target: &str) -> Result<()> {
        let _ = self.tx.send(InstallMsg::Log(format!("Unmounting {}", target)));
        let status = Command::new("umount")
            .arg(target)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()?;
        if !status.success() { bail!("Unmount failed"); }
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
