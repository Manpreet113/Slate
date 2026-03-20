use anyhow::{bail, Context, Result};
use std::fs;

pub fn check(verbose: bool) -> Result<()> {
    println!("[Slate] Checking system requirements...");

    // 1. Confirm Arch Linux (Live ISO or existing Arch)
    let os_release = fs::read_to_string("/etc/os-release").context("Failed to read /etc/os-release")?;
    if !os_release.contains("ID=arch") && !os_release.contains("ID=archarm") {
        bail!("Slate requires Arch Linux. This system is not Arch.");
    }
    if verbose { println!("✓ Running on Arch Linux"); }

    // 2. Check Root
    if !nix::unistd::Uid::effective().is_root() {
        bail!("This operation requires ROOT privileges.");
    }
    if verbose { println!("✓ Root access verified"); }

    // 3. Check UEFI
    if !std::path::Path::new("/sys/firmware/efi").exists() {
        bail!("Legacy BIOS detected. Slate requires UEFI mode.");
    }
    if verbose { println!("✓ UEFI mode verified"); }

    println!("\n[Slate] System check complete. Ready for installation.");
    Ok(())
}
