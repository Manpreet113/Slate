use anyhow::{bail, Context, Result};
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

pub fn install() -> Result<()> {
    crate::system::ensure_not_root()?;
    crate::system::ensure_base_devel()?;

    println!("[Slate] Full system installation starting...");
    println!("[Slate] This will install packages, configure bootloader, and set up system files.\n");
    
    // Get repo directory
    let repo_dir = env::current_dir()?.canonicalize()?;
    
    // 1. System update
    println!("[Slate] Synchronizing repositories and updating system...");
    run_command("sudo", &["pacman", "-Syu", "--noconfirm"])?;
    
    // 2. Install official packages
    println!("\n[Slate] Installing official packages...");
    let pacman_list = repo_dir.join("packages/pacman.txt");
    
    if !pacman_list.exists() {
        bail!("packages/pacman.txt not found in {}", repo_dir.display());
    }
    
    let packages = fs::read_to_string(&pacman_list)?
        .lines()
        .filter(|line| !line.trim().is_empty() && !line.trim().starts_with('#'))
        .collect::<Vec<_>>()
        .join(" ");
    
    if !packages.is_empty() {
        let status = Command::new("sudo")
            .arg("pacman")
            .arg("-S")
            .arg("--needed")
            .arg("--noconfirm")
            .args(packages.split_whitespace())
            .status()?;
        
        if !status.success() {
            bail!("Failed to install official packages");
        }
    }
    
    // 3. Bootstrap yay if needed
    println!("\n[Slate] Checking for AUR helper...");
    if Command::new("which").arg("yay").output()?.status.success() {
        println!("  ✓ yay already installed");
    } else {
        println!("  → Bootstrapping yay-bin...");
        let temp_dir = std::env::temp_dir().join("yay-bin-install");
        fs::create_dir_all(&temp_dir)?;
        
        run_command("git", &[
            "clone",
            "https://aur.archlinux.org/yay-bin.git",
            temp_dir.to_str().unwrap()
        ])?;
        
        let build_status = Command::new("makepkg")
            .args(["-si", "--noconfirm"])
            .current_dir(&temp_dir)
            .status()?;
        
        fs::remove_dir_all(&temp_dir).ok();
        
        if !build_status.success() {
            bail!("Failed to bootstrap yay");
        }
        println!("  ✓ yay installed");
    }
    
    // 4. Install AUR packages
    println!("\n[Slate] Installing AUR packages...");
    let aur_list = repo_dir.join("packages/aur.txt");
    
    if aur_list.exists() {
        let aur_packages = fs::read_to_string(&aur_list)?
            .lines()
            .filter(|line| !line.trim().is_empty() && !line.trim().starts_with('#'))
            .collect::<Vec<_>>()
            .join(" ");
        
        if !aur_packages.is_empty() {
            let status = Command::new("yay")
                .arg("-S")
                .arg("--needed")
                .arg("--noconfirm")
                .args(aur_packages.split_whitespace())
                .status()?;
            
            if !status.success() {
                println!("  ⚠ Some AUR packages failed to install");
            }
        }
    }
    
    // 5. Install system configs
    println!("\n[Slate] Installing system configs...");
    let system_dir = repo_dir.join("system");
    
    if system_dir.exists() {
        // mkinitcpio.conf
        if system_dir.join("mkinitcpio.conf").exists() {
            run_command("sudo", &[
                "cp",
                system_dir.join("mkinitcpio.conf").to_str().unwrap(),
                "/etc/mkinitcpio.conf"
            ])?;
            println!("  ✓ Installed mkinitcpio.conf");
        }
        
        // Plymouth theme
        let plymouth_theme = system_dir.join("mono-steel");
        if plymouth_theme.exists() && plymouth_theme.is_dir() {
            let dest_dir = PathBuf::from("/usr/share/plymouth/themes/mono-steel");
            run_command("sudo", &["mkdir", "-p", dest_dir.to_str().unwrap()])?;
            
            // Recursively copy directory contents
            copy_dir_recursive_sudo(&plymouth_theme, &dest_dir)?;
            println!("  ✓ Installed Plymouth theme");
        }
    }
    
    // 6. Change default shell to zsh
    println!("\n[Slate] Verifying default shell...");
    let current_shell = env::var("SHELL").unwrap_or_default();
    
    if current_shell != "/usr/bin/zsh" {
        println!("  → Changing default shell to zsh");
        run_command("chsh", &["-s", "/usr/bin/zsh"])?;
        println!("  ✓ Default shell set to zsh");
    } else {
        println!("  ✓ zsh is already the default shell");
    }
    
    // 7. Detect hardware and patch bootloader
    println!("\n[Slate] Discovering hardware configuration...");
    let partuuid = detect_partuuid()?;
    println!("  ✓ Root PARTUUID: {}", partuuid);
    
    println!("\n[Slate] Patching bootloader...");
    patch_bootloader(&system_dir, &partuuid)?;
    
    // 8. Set Plymouth theme and rebuild initcpio
    println!("\n[Slate] Setting Plymouth theme and rebuilding initcpio...");
    run_command("sudo", &["plymouth-set-default-theme", "-R", "mono-steel"])?;
    println!("  ✓ Plymouth theme set and initcpio rebuilt");
    
    // 9. Run slate init to set up config management
    println!("\n[Slate] Initializing configuration management...");
    crate::commands::init()?;
    
    println!("\n[Slate] Installation complete!");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("Reboot to enter the void.");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    
    Ok(())
}

fn detect_partuuid() -> Result<String> {
    use crate::system;

    let root_device = system::get_root_device()
        .context("Failed to detect root device")?;
    
    if !root_device.starts_with("/dev/mapper/") && !root_device.starts_with("/dev/dm-") {
        bail!(
            "Slate requires LUKS encryption. Root device is {}, not a mapped device.",
            root_device
        );
    }
    
    let physical_device = system::trace_to_physical_partition(&root_device)
        .context("Failed to trace root device to physical partition")?;
        
    let partuuid = system::get_partuuid(&physical_device)
        .context("Failed to get PARTUUID")?;
        
    Ok(partuuid)
}

fn patch_bootloader(system_dir: &PathBuf, partuuid: &str) -> Result<()> {
    // Check for Limine
    if PathBuf::from("/boot/limine").exists() || PathBuf::from("/boot/limine.conf").exists() {
        println!("  → Detected Limine bootloader");
        let limine_conf = system_dir.join("limine.conf");
        
        if !limine_conf.exists() {
            println!("  ⚠ Warning: system/limine.conf not found, skipping limine patch");
            return Ok(());
        }
        
        run_command("sudo", &["mkdir", "-p", "/boot/limine"])?;
        
        // Read template, replace PARTUUID, write to /boot
        let template = fs::read_to_string(&limine_conf)?;
        let patched = template.replace("{{ROOT_PARTUUID}}", partuuid);
        
        let temp_file = std::env::temp_dir().join("limine.conf");
        fs::write(&temp_file, patched)?;
        
        run_command("sudo", &[
            "cp",
            temp_file.to_str().unwrap(),
            "/boot/limine/limine.conf"
        ])?;
        
        println!("  ✓ Patched /boot/limine/limine.conf");
        
    } else if PathBuf::from("/boot/loader/entries").exists() {
        println!("  → Detected systemd-boot");
        
        let entries_path = PathBuf::from("/boot/loader/entries");
        let arch_entry = fs::read_dir(&entries_path)?
            .filter_map(|e| e.ok())
            .find(|e| {
                e.file_name()
                    .to_string_lossy()
                    .to_lowercase()
                    .contains("arch")
            });
        
        if let Some(entry) = arch_entry {
            let entry_path = entry.path();
            println!("  → Patching {}", entry_path.display());
            
            let content = fs::read_to_string(&entry_path)?;
            let patched = regex::Regex::new(r"root=PARTUUID=[a-zA-Z0-9-]+")?
                .replace(&content, &format!("root=PARTUUID={}", partuuid));
            
            let temp_file = std::env::temp_dir().join("systemd-boot-entry.conf");
            fs::write(&temp_file, patched.as_ref())?;
            
            run_command("sudo", &[
                "cp",
                temp_file.to_str().unwrap(),
                entry_path.to_str().unwrap()
            ])?;
            
            println!("  ✓ Patched systemd-boot entry");
        } else {
            println!("  ⚠ Warning: No Arch entry found in /boot/loader/entries/");
        }
        
    } else {
        println!("  ⚠ Unknown bootloader. You'll need to configure boot parameters manually.");
    }
    
    Ok(())
}

fn copy_dir_recursive_sudo(src: &PathBuf, dst: &PathBuf) -> Result<()> {
    // Ensure destination directory exists
    run_command("sudo", &["mkdir", "-p", dst.to_str().unwrap()])?;
    
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let file_name = entry.file_name();
        let dst_path = dst.join(&file_name);
        
        if src_path.is_dir() {
            // Recursively copy subdirectory
            copy_dir_recursive_sudo(&src_path, &dst_path)?;
        } else {
            // Copy file with sudo
            run_command("sudo", &[
                "cp",
                src_path.to_str().unwrap(),
                dst_path.to_str().unwrap()
            ])?;
        }
    }
    
    Ok(())
}

fn run_command(cmd: &str, args: &[&str]) -> Result<()> {
    let status = Command::new(cmd)
        .args(args)
        .status()
        .with_context(|| format!("Failed to run: {} {}", cmd, args.join(" ")))?;
    
    if !status.success() {
        bail!("Command failed: {} {}", cmd, args.join(" "));
    }
    
    Ok(())
}
