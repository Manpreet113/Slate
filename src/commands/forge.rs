use crate::preflight;
use crate::system;
use crate::tui;
use anyhow::{bail, Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// The entry point for `slate install`
pub fn forge() -> Result<()> {
    println!("\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("  SLATE: ARCH LINUX INSTALLER");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

    // 1. Disk Selection
    let devices = system::list_block_devices().context("Failed to list block devices")?;
    if devices.is_empty() {
        bail!("No block devices found for installation!");
    }
    let selected_device = tui::select_disk(&devices)?;
    let device = &selected_device.path;

    // 2. User Info Collection
    let user_info = tui::get_user_info()?;

    // 3. Safety Check (Preflight)
    preflight::run(device)?;

    // 4. Partitioning
    cleansing(device)?;

    // Instantiate MountGuard to manage cleanup
    let mut guard = MountGuard::new();

    // 5. Btrfs Subvolumes & Mounting
    subvolume_dance(device, &mut guard)?;

    // 6. System bootstrap
    injection()?;

    // 7. Save user info for chroot stage
    let user_info_path = Path::new("/mnt/root/user_info.json");
    let user_info_json = serde_json::to_string(&user_info)?;
    fs::write(user_info_path, user_info_json)?;

    println!("\n[Forge] Phase 7: Entering Chroot...");
    let status = Command::new("arch-chroot")
        .args(["/mnt", "slate", "chroot-stage"])
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()?;

    if !status.success() {
        bail!("Chroot stage failed");
    }

    // Cleanup user info after successful chroot
    let _ = fs::remove_file(user_info_path);

    println!("\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("  FORGE COMPLETE.");
    println!("  System is ready. Reboot when ready.");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    Ok(())
}

/// 4. The Cleansing: Wipe and partition
fn cleansing(device: &str) -> Result<()> {
    println!("\n[Forge] Phase 4: Partitioning...");

    // Wipe partition table
    run_command("sgdisk", &["--zap-all", device])?;

    // Create EFI partition (1GB, type ef00)
    run_command("sgdisk", &["-n", "1:0:+1G", "-t", "1:ef00", device])?;

    // Create Root partition (Remaining space, type 8300 - Linux Filesystem)
    run_command("sgdisk", &["-n", "2:0:0", "-t", "2:8300", device])?;

    // Format EFI
    let efi_part = resolve_partition(device, 1);
    println!("  > Formatting EFI: {}", efi_part);
    run_command("mkfs.vfat", &["-F32", "-n", "EFI", &efi_part])?;

    // Format Btrfs Root
    let root_part = resolve_partition(device, 2);
    println!("  > Formatting Btrfs: {}", root_part);
    run_command("mkfs.btrfs", &["-f", "-L", "Arch", &root_part])?;

    Ok(())
}

/// 5. The Subvolume Dance: Btrfs Layout
fn subvolume_dance(device: &str, guard: &mut MountGuard) -> Result<()> {
    println!("\n[Forge] Phase 5: The Subvolume Dance...");
    let root_part = resolve_partition(device, 2);

    // Mount root temporarily to create subvolumes
    guard.mount(&root_part, "/mnt", &[])?;

    println!("  > Creating Subvolumes...");
    run_command("btrfs", &["subvolume", "create", "/mnt/@"])?;
    run_command("btrfs", &["subvolume", "create", "/mnt/@home"])?;
    run_command("btrfs", &["subvolume", "create", "/mnt/@pkg"])?;

    // Unmount root
    guard.unmount("/mnt")?;

    println!("  > Mounting Subvolumes...");
    let mount_opts = "rw,noatime,compress=zstd,discard=async,space_cache=v2";

    // Mount Root (@)
    guard.mount(
        &root_part,
        "/mnt",
        &["-o", &format!("subvol=@,{}", mount_opts)],
    )?;

    // Create directories
    fs::create_dir_all("/mnt/home")?;
    fs::create_dir_all("/mnt/var/cache/pacman/pkg")?;
    fs::create_dir_all("/mnt/var/log")?;
    fs::create_dir_all("/mnt/boot/EFI")?;

    // Mount @home
    guard.mount(
        &root_part,
        "/mnt/home",
        &["-o", &format!("subvol=@home,{}", mount_opts)],
    )?;

    // Mount @pkg
    guard.mount(
        &root_part,
        "/mnt/var/cache/pacman/pkg",
        &["-o", &format!("subvol=@pkg,{}", mount_opts)],
    )?;

    // Mount EFI
    let efi_part = resolve_partition(device, 1);
    guard.mount(&efi_part, "/mnt/boot/EFI", &[])?;

    Ok(())
}

struct MountGuard {
    mounts: Vec<PathBuf>,
}

impl MountGuard {
    fn new() -> Self {
        Self { mounts: Vec::new() }
    }

    fn mount(&mut self, source: &str, target: &str, options: &[&str]) -> Result<()> {
        let status = Command::new("mount")
            .args(options)
            .arg(source)
            .arg(target)
            .status()?;

        if !status.success() {
            bail!("Failed to mount {} to {}", source, target);
        }

        self.mounts.push(PathBuf::from(target));
        Ok(())
    }

    fn unmount(&mut self, target: &str) -> Result<()> {
        let status = Command::new("umount").arg(target).status()?;

        if !status.success() {
            bail!("Failed to unmount {}", target);
        }

        // Remove from list so we don't double unmount on drop
        if let Some(pos) = self.mounts.iter().rposition(|p| p == Path::new(target)) {
            self.mounts.remove(pos);
        }
        Ok(())
    }
}

impl Drop for MountGuard {
    fn drop(&mut self) {
        // Unmount in reverse order
        for mount in self.mounts.iter().rev() {
            println!("  [Cleanup] Unmounting {}", mount.display());
            let _ = Command::new("umount").arg("-l").arg(mount).status();
        }
    }
}

/// 6. The Injection: Bootstrap system
fn injection() -> Result<()> {
    println!("\n[Forge] Phase 6: The Injection...");

    println!("  > Installing Base System...");
    let status = Command::new("pacstrap")
        .args([
            "-K",
            "/mnt",
            "base",
            "base-devel",
            "linux",
            "linux-firmware",
            "btrfs-progs",
            "git",
            "vim",
            "intel-ucode",
            "amd-ucode",
            "networkmanager",
        ])
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()?;

    if !status.success() {
        bail!("Pacstrap failed");
    }

    println!("  > Generating Fstab...");
    let output = Command::new("genfstab").arg("-U").arg("/mnt").output()?;

    if !output.status.success() {
        bail!("genfstab failed");
    }

    let fstab_path = Path::new("/mnt/etc/fstab");
    fs::write(fstab_path, output.stdout)?;

    println!("  > Injecting Slate Binary...");
    let current_exe = std::env::current_exe()?;
    let target_dir = Path::new("/mnt/usr/local/bin");
    fs::create_dir_all(target_dir)?;
    fs::copy(&current_exe, target_dir.join("slate"))?;
    run_command("chmod", &["+x", "/mnt/usr/local/bin/slate"])?;

    println!("  > Injecting ax Binary...");
    let ax_url = "https://github.com/manpreet113/ax/releases/latest/download/ax";
    let ax_p = target_dir.join("ax");
    run_command(
        "curl",
        &["-L", ax_url, "-o", ax_p.to_string_lossy().as_ref()],
    )?;
    run_command("chmod", &["+x", "/mnt/usr/local/bin/ax"])?;

    Ok(())
}

fn run_command(cmd: &str, args: &[&str]) -> Result<()> {
    println!("  $ {} {}", cmd, args.join(" "));
    let output = Command::new(cmd)
        .args(args)
        .output()
        .context(format!("Failed to execute {}", cmd))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "Command failed: {} {}\nError: {}",
            cmd,
            args.join(" "),
            stderr
        );
    }
    Ok(())
}

fn resolve_partition(device: &str, part_num: i32) -> String {
    if device.contains("nvme") || device.contains("mmcblk") {
        format!("{}p{}", device, part_num)
    } else {
        format!("{}{}", device, part_num)
    }
}
