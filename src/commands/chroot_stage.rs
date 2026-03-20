use crate::system;
use anyhow::{bail, Context, Result};
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::Command;
use crate::tui::UserInfo;

pub fn chroot_stage() -> Result<()> {
    // 1. Load User Info
    let user_info_path = Path::new("/root/user_info.json");
    let user_info_content = fs::read_to_string(user_info_path)
        .context("Failed to read user_info.json in chroot")?;
    let user_info: UserInfo = serde_json::from_str(&user_info_content)?;

    // 2. Base System Config
    configure_base(&user_info)?;

    // 3. User & Auth & Shell
    configure_user(&user_info)?;

    // 4. Desktop Environment (Hyprland & Greetd)
    configure_desktop(&user_info)?;

    // 5. Tooling (Ax, VSCode, Git)
    configure_tools(&user_info)?;

    // 6. Bootloader
    configure_boot()?;

    Ok(())
}

fn configure_base(config: &UserInfo) -> Result<()> {
    println!("  > Configuring Base System...");

    // Hostname
    fs::write("/etc/hostname", format!("{}\n", config.hostname))?;

    // Timezone
    let _ = fs::remove_file("/etc/localtime");
    let zone_path = format!("/usr/share/zoneinfo/{}", config.timezone);
    if Path::new(&zone_path).exists() {
        let _ = std::os::unix::fs::symlink(&zone_path, "/etc/localtime");
    } else {
        let _ = std::os::unix::fs::symlink("/usr/share/zoneinfo/UTC", "/etc/localtime");
    }

    // Locale
    let locale_gen = "/etc/locale.gen";
    if Path::new(locale_gen).exists() {
        let content = fs::read_to_string(locale_gen)?;
        let new_content = content.replace("#en_US.UTF-8 UTF-8", "en_US.UTF-8 UTF-8");
        fs::write(locale_gen, new_content)?;
        run_command("locale-gen", &[])?;
    }
    fs::write("/etc/locale.conf", "LANG=en_US.UTF-8\n")?;

    // Keymap
    fs::write("/etc/vconsole.conf", format!("KEYMAP={}\n", config.keymap))?;

    // Time & NTP
    run_command("hwclock", &["--systohc"])?;
    run_command("systemctl", &["enable", "systemd-timesyncd"])?;
    run_command("systemctl", &["enable", "NetworkManager"])?;
    run_command("systemctl", &["enable", "bluetooth"])?;

    Ok(())
}

fn configure_user(config: &UserInfo) -> Result<()> {
    println!("  > Configuring User & Zsh...");

    // Create user with Zsh
    run_command("useradd", &["-m", "-G", "wheel", "-s", "/usr/bin/zsh", &config.username])?;

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

    // Modern Zshrc
    let zshrc = r#"
# slate-desktop: modern zshrc
alias ls='eza --icons'
alias l='eza -lh --icons'
alias ll='eza -lha --icons'
alias cat='bat'
alias grep='rg'
alias cd='zoxide'

# Starship Prompt
eval "$(starship init zsh)"
eval "$(zoxide init zsh)"

# Path
export PATH=$PATH:$HOME/.local/bin
"#;
    let user_home = format!("/home/{}", config.username);
    fs::write(format!("{}/.zshrc", user_home), zshrc)?;
    
    // Starship config (minimal)
    fs::create_dir_all(format!("{}/.config", user_home))?;
    fs::write(format!("{}/.config/starship.toml", user_home), "[add_newline]\ninsert_newline = false\n")?;

    run_command("chown", &["-R", &format!("{}:{}", config.username, config.username), &user_home])?;

    Ok(())
}

fn configure_desktop(config: &UserInfo) -> Result<()> {
    println!("  > Configuring Hyprland & Greetd...");

    let user_home = format!("/home/{}", config.username);
    let hypr_dir = format!("{}/.config/hypr", user_home);
    fs::create_dir_all(&hypr_dir)?;

    let hypr_conf = format!(r#"
# slate-desktop: minimal hyprland config
monitor=,preferred,auto,auto

exec-once = waybar & dunst
$terminal = ghostty
$browser = firefox

input {{
    kb_layout = {}
    follow_mouse = 1
}}

general {{
    gaps_in = 5
    gaps_out = 10
    border_size = 2
    col.active_border = rgba(33ccffee) rgba(00ff99ee) 45deg
    col.inactive_border = rgba(595959aa)
    layout = dwindle
}}

decoration {{
    rounding = 10
    blur {{
        enabled = true
        size = 3
        passes = 1
    }}
}}

animations {{
    enabled = true
    bezier = myBezier, 0.05, 0.9, 0.1, 1.05
    animation = windows, 1, 7, myBezier
    animation = windowsOut, 1, 7, default, popin 80%
    animation = border, 1, 10, default
    animation = fade, 1, 7, default
    animation = workspaces, 1, 6, default
}}

bind = SUPER, Enter, exec, $terminal
bind = SUPER, B, exec, $browser
bind = SUPER, Q, killactive,
bind = SUPER, M, exit,
bind = SUPER, V, togglefloating,
bind = SUPER, Space, exec, wofi --show drun

# Mouse bindings
bindm = SUPER, mouse:272, movewindow
bindm = SUPER, mouse:273, resizewindow
"#, config.keymap);

    fs::write(format!("{}/hyprland.conf", hypr_dir), hypr_conf)?;
    run_command("chown", &["-R", &format!("{}:{}", config.username, config.username), &user_home])?;

    // Greetd / Tuigreet
    fs::create_dir_all("/etc/greetd")?;
    let greetd_conf = format!(r#"[terminal]
vt = 1

[default_session]
command = "tuigreet --time --cmd hyprland"
user = "{}"
"#, config.username);
    fs::write("/etc/greetd/config.toml", greetd_conf)?;
    run_command("systemctl", &["enable", "greetd"])?;

    Ok(())
}

fn configure_tools(config: &UserInfo) -> Result<()> {
    println!("  > Finalizing Tools (Ax & Git)...");

    // Git Config
    if !config.git_name.is_empty() {
        let user_home = format!("/home/{}", config.username);
        let gitconfig = format!(r#"[user]
	name = {}
	email = {}
"#, config.git_name, config.git_email);
        fs::write(format!("{}/.gitconfig", user_home), gitconfig)?;
        run_command("chown", &[&format!("{}:{}", config.username, config.username), &format!("{}/.gitconfig", user_home)])?;
    }

    // AUR Packages via Ax
    println!("  > Installing VS Code via Ax (AUR)...");
    // We need to run ax as the user, not root, for AUR stuff usually, 
    // but since we're in chroot and ax is at /usr/local/bin/ax, let's try direct.
    // If ax requires a non-root user, we'd use 'sudo -u username ax ...'
    let _ = Command::new("sudo")
        .args(["-u", &config.username, "ax", "-S", "visual-studio-code-bin", "--noconfirm"])
        .status();

    Ok(())
}

fn configure_boot() -> Result<()> {
    println!("  > Configuring Bootloader...");

    let root_dev = system::get_root_device()?;
    let root_uuid = system::get_uuid(&root_dev)?;

    run_command("bootctl", &["install"])?;

    let entry_content = format!(
        "title   Slate OS (Arch)\nlinux   /vmlinuz-linux\ninitrd  /intel-ucode.img\ninitrd  /amd-ucode.img\ninitrd  /initramfs-linux.img\noptions root=UUID={} rw rootflags=subvol=@\n",
        root_uuid
    );
    fs::create_dir_all("/boot/loader/entries")?;
    fs::write("/boot/loader/entries/arch.conf", entry_content)?;

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
