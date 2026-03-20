use crate::system;
use anyhow::{bail, Context, Result};
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::Command;
use crate::tui::UserInfo;

pub fn chroot_stage() -> Result<()> {
    println!("\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("  ARCH LINUX: CHROOT STAGE");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    // 1. Load User Info
    let user_info_path = Path::new("/root/user_info.json");
    let user_info_content = fs::read_to_string(user_info_path)
        .context("Failed to read user_info.json in chroot")?;
    let user_info: UserInfo = serde_json::from_str(&user_info_content)?;

    // 2. Base System Config
    configure_base(&user_info)?;

    // 3. User & Auth
    configure_user(&user_info)?;

    // 4. Bootloader
    configure_boot()?;

    println!("\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("  CHROOT STAGE COMPLETE");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    Ok(())
}

fn configure_base(config: &UserInfo) -> Result<()> {
    println!("  > Configuring Base System...");

    // Hostname
    fs::write("/etc/hostname", &config.hostname)?;

    // Timezone (Default to UTC)
    let _ = fs::remove_file("/etc/localtime");
    std::os::unix::fs::symlink("/usr/share/zoneinfo/UTC", "/etc/localtime")?;

    // Locale
    let locale_gen = "/etc/locale.gen";
    if Path::new(locale_gen).exists() {
        let content = fs::read_to_string(locale_gen)?;
        let new_content = content.replace("#en_US.UTF-8 UTF-8", "en_US.UTF-8 UTF-8");
        fs::write(locale_gen, new_content)?;
        run_command("locale-gen", &[])?;
    }
    fs::write("/etc/locale.conf", "LANG=en_US.UTF-8\n")?;

    Ok(())
}

fn configure_user(config: &UserInfo) -> Result<()> {
    println!("  > Configuring User & Auth...");

    // Create user
    run_command("useradd", &["-m", "-G", "wheel", "-s", "/bin/bash", &config.username])?;

    // Set passwords
    let root_auth = format!("root:{}", config.password);
    let user_auth = format!("{}:{}", config.username, config.password);
    run_command_stdin("chpasswd", &[], &format!("{}\n{}", root_auth, user_auth))?;

    // Sudoers
    let sudoers_file = "/etc/sudoers";
    if Path::new(sudoers_file).exists() {
        let sudoers = fs::read_to_string(sudoers_file)?;
        let new_sudoers = sudoers.replace("# %wheel ALL=(ALL:ALL) ALL", "%wheel ALL=(ALL:ALL) ALL");
        fs::write(sudoers_file, new_sudoers)?;
    }

    Ok(())
}

fn configure_boot() -> Result<()> {
    println!("  > Configuring Bootloader...");

    // 1. Detect UUID of root partition (the Btrfs partition)
    let root_dev = system::get_root_device()?;
    let root_uuid = system::get_uuid(&root_dev)?;
    println!("    Root UUID: {}", root_uuid);

    // 2. Install systemd-boot
    run_command("bootctl", &["install"])?;

    // 3. Create loader entry
    let entry_content = format!(
        "title   Arch Linux\nlinux   /vmlinuz-linux\ninitrd  /intel-ucode.img\ninitrd  /amd-ucode.img\ninitrd  /initramfs-linux.img\noptions root=UUID={} rw rootflags=subvol=@\n",
        root_uuid
    );
    fs::create_dir_all("/boot/loader/entries")?;
    fs::write("/boot/loader/entries/arch.conf", entry_content)?;

    // 4. Update loader.conf
    let loader_conf = "default arch.conf\ntimeout 3\nconsole-mode max\n";
    fs::write("/boot/loader/loader.conf", loader_conf)?;

    Ok(())
}

fn run_command(cmd: &str, args: &[&str]) -> Result<()> {
    println!("    $ {} {}", cmd, args.join(" "));
    let status = Command::new(cmd)
        .args(args)
        .status()
        .context(format!("Failed to run {}", cmd))?;

    if !status.success() {
        bail!("Command failed: {}", cmd);
    }
    Ok(())
}

fn run_command_stdin(cmd: &str, args: &[&str], input: &str) -> Result<()> {
    let mut child = Command::new(cmd)
        .args(args)
        .stdin(std::process::Stdio::piped())
        .spawn()?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(input.as_bytes())?;
    }

    let status = child.wait()?;
    if !status.success() {
        bail!("Command {} failed", cmd);
    }
    Ok(())
}
