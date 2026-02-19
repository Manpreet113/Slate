use crate::preflight;
use anyhow::{bail, Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// The entry point for `slate forge <device>`
pub fn forge(device: &str) -> Result<()> {
    // 1. Safety Check (Preflight)
    preflight::run(device)?;

    // 2. Partitioning
    cleansing(device)?;

    // 3. Encryption & Formatting
    vault(device)?;

    // Instantiate MountGuard to manage cleanup
    let mut guard = MountGuard::new();

    // 4. Btrfs Subvolumes & Mounting
    subvolume_dance(device, &mut guard)?;

    // 5. System bootstrap
    injection()?;

    println!("\n[Forge] Phase 6: Entering Chroot...");
    let status = Command::new("arch-chroot")
        .args(["/mnt", "slate", "chroot-stage"])
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()?;

    if !status.success() {
        bail!("Chroot stage failed");
    }

    println!("\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("  FORGE COMPLETE.");
    println!("  System is ready. Reboot when ready.");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    // Guard will be dropped here, unmounting everything
    Ok(())
}

/// 2. The Cleansing: Wipe and partition
fn cleansing(device: &str) -> Result<()> {
    println!("\n[Forge] Phase 2: The Cleansing...");

    // Wipe partition table
    run_command("sgdisk", &["--zap-all", device])?;

    // Create EFI partition (512MB, type ef00)
    // -n 1:0:+512M -> New partition 1, default start, +512M size
    run_command("sgdisk", &["-n", "1:0:+1G", "-t", "1:ef00", device])?;

    // Create Root partition (Remaining space, type 8309 - Linux LUKS)
    run_command("sgdisk", &["-n", "2:0:0", "-t", "2:8309", device])?;

    // Format EFI
    let efi_part = resolve_partition(device, 1);
    println!("  > Formatting EFI: {}", efi_part);
    run_command("mkfs.vfat", &["-F32", "-n", "EFI", &efi_part])?;

    Ok(())
}

/// 3. The Vault: LUKS2 Encryption and Root Format
fn vault(device: &str) -> Result<()> {
    println!("\n[Forge] Phase 3: The Vault...");
    let root_part = resolve_partition(device, 2);

    println!("  > Encrypting Root: {}", root_part);

    let status = Command::new("cryptsetup")
        .args(["luksFormat", "--type", "luks2", &root_part])
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()?;

    if !status.success() {
        bail!("Failed to encrypt root partition");
    }

    println!("  > Opening Vault...");
    let status = Command::new("cryptsetup")
        .args(["open", &root_part, "root"])
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()?;

    if !status.success() {
        bail!("Failed to open root partition");
    }

    println!("  > Formatting Btrfs...");
    run_command("mkfs.btrfs", &["-f", "-L", "Arch", "/dev/mapper/root"])?;

    Ok(())
}

/// 4. The Subvolume Dance: Btrfs Layout
fn subvolume_dance(device: &str, guard: &mut MountGuard) -> Result<()> {
    println!("\n[Forge] Phase 4: The Subvolume Dance...");

    // Mount root temporarily to create subvolumes
    guard.mount("/dev/mapper/root", "/mnt", &[])?;

    println!("  > Creating Subvolumes...");
    run_command("btrfs", &["subvolume", "create", "/mnt/@"])?;
    run_command("btrfs", &["subvolume", "create", "/mnt/@home"])?;
    run_command("btrfs", &["subvolume", "create", "/mnt/@pkg"])?;
    // No @log or @var as per plan

    // Unmount root
    guard.unmount("/mnt")?;

    println!("  > Mounting Subvolumes...");
    let mount_opts = "rw,noatime,compress=zstd,discard=async,space_cache=v2";

    // Mount Root (@)
    guard.mount(
        "/dev/mapper/root",
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
        "/dev/mapper/root",
        "/mnt/home",
        &["-o", &format!("subvol=@home,{}", mount_opts)],
    )?;

    // Mount @pkg
    guard.mount(
        "/dev/mapper/root",
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
        // Also close LUKS if open? The plan didn't explicitly say LuksGuard but simple MountGuard.
        // Usually /dev/mapper/root auto-closes if unmounted? No.
        // We should probably close it too if we want full cleanup.
        // But for now sticking to the plan: "Implement a MountGuard struct"

        // After unmounting /mnt, we should probably try to close root.
        let _ = Command::new("cryptsetup").arg("close").arg("root").status();
    }
}

/// 5. The Injection: Bootstrap system
fn injection() -> Result<()> {
    println!("\n[Forge] Phase 5: The Injection...");

    println!("  > Installing Base System...");
    let status = Command::new("pacstrap")
        .args([
            "-K",
            "/mnt",
            "base",
            "base-devel",
            "git",
            "vim",
            "intel-ucode",
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
    // Curl ax from github releases
    let ax_url = "https://github.com/manpreet113/ax/releases/latest/download/ax";
    let ax_p = target_dir.join("ax");
    run_command(
        "curl",
        &["-L", ax_url, "-o", ax_p.to_string_lossy().as_ref()],
    )?;
    run_command("chmod", &["+x", "/mnt/usr/local/bin/ax"])?;

    Ok(())
}

// --- Helpers ---

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
