use anyhow::{Context, Result, bail};
use std::process::{Command, Stdio};
use std::io::{self, Write};
use std::path::Path;
use std::fs;

/// The entry point for `slate forge <device>`
pub fn forge(device: &str) -> Result<()> {
    // 1. Safety Check
    interrogation(device)?;

    // 2. Partitioning
    cleansing(device)?;

    // 3. Encryption & Formatting
    vault(device)?;

    // 4. Btrfs Subvolumes & Mounting
    subvolume_dance(device)?;

    // 5. System bootstrap
    injection()?;

    println!("\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("  FORGE COMPLETE.");
    println!("  The system is ready for `slate install`.");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("  Now run:");
    println!("    arch-chroot /mnt");
    println!("    slate install");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    Ok(())
}

/// 1. The Interrogation: Ensure user really wants to destroy data
fn interrogation(device: &str) -> Result<()> {
    println!("\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("  WAR FORGE: TARGETING DEVICE -> {}", device);
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("  WARNING: THIS WILL SECURE ERASE {}.", device);
    println!("  ALL DATA WILL BE IRRETRIEVABLY DESTROYED.");
    println!("  THE VOID AWAITS.");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    print!("  Type 'VOID' to proceed: ");
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;

    if input.trim() != "VOID" {
        bail!("Aborted. The void recedes.");
    }

    println!("  > CONFIRMED. INITIATING PROTOCOL...");
    Ok(())
}

/// 2. The Cleansing: Wipe and partition
fn cleansing(device: &str) -> Result<()> {
    println!("\n[Forge] Phase 2: The Cleansing...");

    // Wipe partition table
    run_command("sgdisk", &["--zap-all", device])?;

    // Create EFI partition (1GB, type ef00)
    // -n 1:0:+1G -> New partition 1, default start, +1G size
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
    // LuksFormat
    // echo -n "password" | cryptsetup ... is insecure for automation without keyfile, 
    // but here we expect interactive password entry from user.
    // However, run_command might capture stdin/stdout. 
    // For interactive `cryptsetup`, we should let it inherit stdin.
    
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
fn subvolume_dance(device: &str) -> Result<()> {
    println!("\n[Forge] Phase 4: The Subvolume Dance...");

    // Mount root to create subvolumes
    run_command("mount", &["/dev/mapper/root", "/mnt"])?;

    println!("  > Creating Subvolumes...");
    run_command("btrfs", &["subvolume", "create", "/mnt/@"])?;
    run_command("btrfs", &["subvolume", "create", "/mnt/@home"])?;
    run_command("btrfs", &["subvolume", "create", "/mnt/@pkg"])?;
    run_command("btrfs", &["subvolume", "create", "/mnt/@log"])?;

    run_command("umount", &["/mnt"])?;

    println!("  > Mounting Subvolumes...");
    let mount_opts = "rw,noatime,compress=zstd,discard=async,space_cache=v2";

    // Mount Root (@)
    run_command("mount", &["-o", &format!("subvol=@,{}", mount_opts), "/dev/mapper/root", "/mnt"])?;

    // Create directories
    fs::create_dir_all("/mnt/home")?;
    fs::create_dir_all("/mnt/var/cache/pacman/pkg")?;
    fs::create_dir_all("/mnt/var/log")?;
    fs::create_dir_all("/mnt/boot/EFI")?;

    // Mount @home
    run_command("mount", &["-o", &format!("subvol=@home,{}", mount_opts), "/dev/mapper/root", "/mnt/home"])?;

    // Mount @pkg
    run_command("mount", &["-o", &format!("subvol=@pkg,{}", mount_opts), "/dev/mapper/root", "/mnt/var/cache/pacman/pkg"])?;

    // Mount @log
    run_command("mount", &["-o", &format!("subvol=@log,{}", mount_opts), "/dev/mapper/root", "/mnt/var/log"])?;

    // Mount EFI
    let efi_part = resolve_partition(device, 1);
    run_command("mount", &[&efi_part, "/mnt/boot/EFI"])?;

    Ok(())
}

/// 5. The Injection: Bootstrap system
fn injection() -> Result<()> {
    println!("\n[Forge] Phase 5: The Injection...");

    println!("  > Installing Base System...");
    // Pacstrap requires interactive sometimes? Usually not if --noconfirm?
    // But pacstrap -K /mnt base base-devel git usually runs fine.
    // Let's inherit stdio to see progress.
    let status = Command::new("pacstrap")
        .args(["-K", "/mnt", "base", "base-devel", "git", "vim", "intel-ucode"]) // Added vim/ucode as sane defaults
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()?;
    
    if !status.success() {
        bail!("Pacstrap failed");
    }

    println!("  > Generating Fstab...");
    // genfstab -U /mnt >> /mnt/etc/fstab
    // Rust Command doesn't do shell redirection easily.
    let output = Command::new("genfstab")
        .arg("-U")
        .arg("/mnt")
        .output()?;
    
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
    
    // Make executable just in case
    run_command("chmod", &["+x", "/mnt/usr/local/bin/slate"])?;

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
        bail!("Command failed: {} {}\nError: {}", cmd, args.join(" "), stderr);
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
