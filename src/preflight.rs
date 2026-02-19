use anyhow::{bail, Result};
use nix::unistd::Uid;
use std::fs;
use std::io::{self, Write};
use std::path::Path;
use std::process::Command;

pub fn run(device: &str) -> Result<()> {
    println!("\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("  INITIALIZING PREFLIGHT PROTOCOLS");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    // 1. Check Root
    if !Uid::effective().is_root() {
        bail!("This operation requires ROOT privileges. Resubmit with sudo.");
    }
    println!("  ✓ Root access verified");

    // 2. Check UEFI
    if !Path::new("/sys/firmware/efi").exists() {
        bail!("Legacy BIOS detected. Slate requires UEFI mode.");
    }
    println!("  ✓ UEFI mode verified");

    // 3. Check Device Existence
    if !Path::new(device).exists() {
        bail!("Target device {} does not exist.", device);
    }
    // Verify it is a block device?
    // metadata().file_type().is_block_device() requires nightly or unix extension
    // Simple existence is fine for now, failure will happen at sgdisk if not block.
    println!("  ✓ Target device exists: {}", device);

    // 4. Check Mounts
    check_mounts(device)?;
    println!("  ✓ Mount check passed");

    // 5. Check Tools
    check_tools()?;
    println!("  ✓ Required tools verified");

    // 6. Check Network
    check_network()?;
    println!("  ✓ Network connectivity verified");

    // 7. Confirmation
    confirm_destruction(device)?;

    Ok(())
}

fn check_mounts(device: &str) -> Result<()> {
    let mounts = fs::read_to_string("/proc/mounts")?;
    for line in mounts.lines() {
        if line.contains(device) {
            bail!("Device {} is currently mounted! Unmount it first.", device);
        }
    }
    Ok(())
}

fn check_tools() -> Result<()> {
    let tools = [
        "sgdisk",
        "cryptsetup",
        "mkfs.btrfs",
        "arch-chroot",
        "pacstrap",
        "mkinitcpio",
        "bootctl",
        "curl",
    ];

    let mut missing = Vec::new();
    for tool in tools {
        let status = Command::new("sh")
            .arg("-c")
            .arg(format!("command -v {}", tool))
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();

        match status {
            Ok(s) if s.success() => continue,
            _ => missing.push(tool),
        }
    }

    if !missing.is_empty() {
        bail!("Missing required tools: {}", missing.join(", "));
    }
    Ok(())
}

fn check_network() -> Result<()> {
    let status = Command::new("curl")
        .args(["-I", "https://archlinux.org"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();

    match status {
        Ok(s) if s.success() => Ok(()),
        _ => bail!("Network unreachable. Cannot reach archlinux.org."),
    }
}

fn confirm_destruction(device: &str) -> Result<()> {
    println!("\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("  WARNING: IRREVOCABLE DATA DESTRUCTION");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("  Target: {}", device);
    println!("  Action: WIPE + FORMAT (LUKS2 + Btrfs)");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    print!("  To proceed, type the device name '{}': ", device);
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;

    if input.trim() != device {
        bail!("Aborted. Device name did not match.");
    }

    println!("\n  > CONFIRMED. ENGAGING DRIVES...");
    Ok(())
}
