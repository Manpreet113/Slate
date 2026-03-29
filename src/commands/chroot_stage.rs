use crate::system;
use anyhow::{bail, Context, Result};
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
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

    // 4. Desktop Environment (Direct Boot & Hyprland)
    configure_desktop(&user_info)?;

    // 5. Tooling (Ax, VSCode, Clipse, Git)
    configure_tools(&user_info)?;

    // 6. Post-config Services
    configure_post_services()?;

    // 7. Bootloader
    configure_boot()?;

    Ok(())
}

fn configure_base(config: &UserInfo) -> Result<()> {
    println!("  > Configuring Base System...");

    enable_multilib_repo()?;

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

    Ok(())
}

fn enable_multilib_repo() -> Result<()> {
    let pacman_conf = "/etc/pacman.conf";
    if !Path::new(pacman_conf).exists() {
        return Ok(());
    }

    let content = fs::read_to_string(pacman_conf)?;
    if content.contains("\n[multilib]\n") && content.contains("\nInclude = /etc/pacman.d/mirrorlist\n") {
        return Ok(());
    }

    // Enable only the [multilib] block include, not any Include line in [options].
    let mut updated_lines: Vec<String> = Vec::new();
    let mut in_multilib = false;
    let mut saw_multilib = false;

    for line in content.lines() {
        let trimmed = line.trim();

        if trimmed == "#[multilib]" {
            updated_lines.push("[multilib]".to_string());
            in_multilib = true;
            saw_multilib = true;
            continue;
        }
        if trimmed == "[multilib]" {
            updated_lines.push(line.to_string());
            in_multilib = true;
            saw_multilib = true;
            continue;
        }

        if trimmed.starts_with('[') && trimmed != "[multilib]" {
            in_multilib = false;
        }

        if in_multilib && trimmed == "#Include = /etc/pacman.d/mirrorlist" {
            updated_lines.push("Include = /etc/pacman.d/mirrorlist".to_string());
            continue;
        }

        updated_lines.push(line.to_string());
    }

    let mut updated = updated_lines.join("\n");
    if content.ends_with('\n') {
        updated.push('\n');
    }

    if !saw_multilib {
        if !updated.ends_with('\n') {
            updated.push('\n');
        }
        updated.push_str("\n[multilib]\nInclude = /etc/pacman.d/mirrorlist\n");
    }

    if updated != content {
        fs::write(pacman_conf, updated)?;
        run_command("pacman", &["-Sy", "--noconfirm"])?;
    }

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
    
    // Auto-login to TTY1
    fs::create_dir_all("/etc/systemd/system/getty@tty1.service.d")?;
    let autologin_override = format!(r#"[Service]
ExecStart=
ExecStart=-/usr/bin/agetty --autologin {} --noclear %I $TERM
"#, config.username);
    fs::write("/etc/systemd/system/getty@tty1.service.d/override.conf", autologin_override)?;

    // Auto-start Hyprland via .zprofile
    let zprofile = r#"
# slate-desktop: auto-start hyprland on tty1
if [[ -z $DISPLAY ]] && [[ $(tty) = /dev/tty1 ]]; then
  exec start-hyprland
fi
"#;
    fs::write(format!("{}/.zprofile", user_home), zprofile)?;
    
    // Starship config (minimal)
    fs::create_dir_all(format!("{}/.config", user_home))?;
    fs::write(format!("{}/.config/starship.toml", user_home), "[add_newline]\ninsert_newline = false\n")?;

    run_command("chown", &["-R", &format!("{}:{}", config.username, config.username), &user_home])?;

    Ok(())
}

fn configure_desktop(config: &UserInfo) -> Result<()> {
    println!("  > Configuring Hyprland...");

    let user_home = format!("/home/{}", config.username);
    let hypr_dir = format!("{}/.config/hypr", user_home);
    fs::create_dir_all(&hypr_dir)?;

    let hypr_conf = format!(r#"
# slate-desktop: direct-boot hyprland config
monitor=,preferred,auto,auto

# Security: start hyprlock immediately
exec-once = hyprlock

# Core Services
exec-once = clipse -listen

$terminal = kitty
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

bind = SUPER, Return, exec, $terminal
bind = SUPER, B, exec, $browser
bind = SUPER, Q, killactive,
bind = SUPER, M, exit,
bind = SUPER, F, togglefloating,
bind = SUPER, Space, exec, wofi --show drun
bind = SUPER, V, exec, kitty -e clipse

# Mouse bindings
bindm = SUPER, mouse:272, movewindow
bindm = SUPER, mouse:273, resizewindow
"#, config.keymap);

    fs::write(format!("{}/hyprland.conf", hypr_dir), hypr_conf)?;
    run_command("chown", &["-R", &format!("{}:{}", config.username, config.username), &user_home])?;

    Ok(())
}

fn configure_tools(config: &UserInfo) -> Result<()> {
    println!("[Phase 2/3] Ax user packages");
    println!("  > Finalizing Tools (Ax, Git, VSCode, Clipse)...");

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
    println!("  > Installing desktop/tooling packages via Ax as user...");
    let packages = [
        // Desktop/session
        "hyprland",
        "hyprlock",
        "hypridle",
        "xdg-desktop-portal-hyprland",
        "qt6-wayland",
        // Audio/media
        "pipewire",
        "wireplumber",
        "pipewire-pulse",
        "pipewire-alsa",
        // Apps and CLI tools
        "firefox",
        "kitty",
        "wofi",
        "starship",
        "eza",
        "bat",
        "zoxide",
        "fzf",
        "ripgrep",
        // Utilities and extras
        "network-manager-applet",
        "blueman",
        "pavucontrol",
        "easyeffects",
        "grim",
        "slurp",
        "imagemagick",
        "jq",
        "sqlite",
        "upower",
        "wl-clipboard",
        "wlsunset",
        "wtype",
        "zbar",
        "glib2",
        "zenity",
        "power-profiles-daemon",
        // Fonts and AUR tooling packages
        "ttf-roboto",
        "ttf-roboto-mono",
        "ttf-dejavu",
        "ttf-liberation",
        "noto-fonts",
        "noto-fonts-cjk",
        "noto-fonts-emoji",
        "ttf-nerd-fonts-symbols",
        "gpu-screen-recorder",
        "adw-gtk-theme",
        "visual-studio-code-bin",
        "clipse",
    ];

    let sudoers_dropin = format!("/etc/sudoers.d/90-slate-ax-{}", config.username);
    let sudoers_content = format!("{} ALL=(ALL:ALL) NOPASSWD: ALL\n", config.username);
    fs::write(&sudoers_dropin, sudoers_content)?;
    run_command("chmod", &["0440", &sudoers_dropin])?;

    let ax_cmd = format!("ax -S {} --noconfirm", packages.join(" "));
    let install_res = run_command("su", &["-", &config.username, "-c", &ax_cmd]);

    let _ = fs::remove_file(&sudoers_dropin);
    install_res?;

    Ok(())
}

fn configure_post_services() -> Result<()> {
    println!("[Phase 3/3] post-config services");
    println!("  > Enabling core services...");
    run_command("systemctl", &["enable", "systemd-timesyncd"])?;
    run_command("systemctl", &["enable", "NetworkManager"])?;
    run_command("systemctl", &["enable", "bluetooth"])?;
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
    let output = Command::new(cmd)
        .args(args)
        .output()
        .context(format!("Failed to run {}", cmd))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if stderr.is_empty() {
            bail!("Command failed: {}", cmd);
        }
        bail!("Command failed: {}: {}", cmd, stderr);
    }
    Ok(())
}

fn run_command_stdin(cmd: &str, args: &[&str], input: &str) -> Result<()> {
    let mut child = Command::new(cmd)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(input.as_bytes())?;
    }

    let output = child.wait_with_output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if stderr.is_empty() {
            bail!("Command {} failed", cmd);
        }
        bail!("Command {} failed: {}", cmd, stderr);
    }
    Ok(())
}
