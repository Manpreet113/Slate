use crate::system;
use crate::tui::UserInfo;
use anyhow::{bail, Context, Result};
use std::collections::HashSet;
use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const SHELL_REPO_URL: &str = "https://github.com/manpreet113/shell.git";
const SHELL_REPO_DIR: &str = "/tmp/slate-shell";
const SHELL_PLUGIN_REPO: &str = "https://github.com/hyprwm/hyprland-plugins.git";
const TEMP_SUDOERS_FILE: &str = "/etc/sudoers.d/10-slate-ax";

pub fn chroot_stage() -> Result<()> {
    let user_info_path = Path::new("/root/user_info.json");
    let user_info_content =
        fs::read_to_string(user_info_path).context("Failed to read user_info.json in chroot")?;
    let user_info: UserInfo = serde_json::from_str(&user_info_content)?;

    setup_fast_downloads()?;
    configure_base(&user_info)?;
    configure_user(&user_info)?;
    configure_shell(&user_info)?;
    configure_tools(&user_info)?;
    configure_post_services()?;
    configure_boot()?;
    setup_autostart(&user_info)?;

    Ok(())
}

fn configure_base(config: &UserInfo) -> Result<()> {
    println!("  > Configuring Base System...");

    enable_multilib_repo()?;

    fs::write("/etc/hostname", format!("{}\n", config.hostname))?;

    let _ = fs::remove_file("/etc/localtime");
    let zone_path = format!("/usr/share/zoneinfo/{}", config.timezone);
    if Path::new(&zone_path).exists() {
        let _ = std::os::unix::fs::symlink(&zone_path, "/etc/localtime");
    } else {
        let _ = std::os::unix::fs::symlink("/usr/share/zoneinfo/UTC", "/etc/localtime");
    }

    let locale_gen = "/etc/locale.gen";
    if Path::new(locale_gen).exists() {
        let content = fs::read_to_string(locale_gen)?;
        let new_content = content.replace("#en_US.UTF-8 UTF-8", "en_US.UTF-8 UTF-8");
        fs::write(locale_gen, new_content)?;
        run_command("locale-gen", &[])?;
    }
    fs::write("/etc/locale.conf", "LANG=en_US.UTF-8\n")?;
    fs::write("/etc/vconsole.conf", format!("KEYMAP={}\n", config.keymap))?;
    run_command("hwclock", &["--systohc"])?;

    Ok(())
}

fn setup_fast_downloads() -> Result<()> {
    let pacman_conf = "/etc/pacman.conf";
    if !Path::new(pacman_conf).exists() {
        return Ok(());
    }

    let content = fs::read_to_string(pacman_conf)?;
    let mut updated = content.clone();

    if content.contains("ParallelDownloads") {
        updated = updated.replace("#ParallelDownloads", "ParallelDownloads");
        updated = updated.replace("ParallelDownloads = 5", "ParallelDownloads = 10");
        updated = updated.replace("ParallelDownloads = 1", "ParallelDownloads = 10");
    } else {
        // Try to insert after [options]
        if let Some(pos) = content.find("[options]") {
            if let Some(line_end) = content[pos..].find('\n') {
                updated.insert_str(pos + line_end + 1, "ParallelDownloads = 10\n");
            }
        } else {
            updated.push_str("\nParallelDownloads = 10\n");
        }
    }

    if !content.contains("DisableDownloadTimeout") {
        if let Some(pos) = updated.find("[options]") {
            if let Some(line_end) = updated[pos..].find('\n') {
                updated.insert_str(pos + line_end + 1, "DisableDownloadTimeout\n");
            }
        } else {
            updated.push_str("\nDisableDownloadTimeout\n");
        }
    }

    if updated != content {
        fs::write(pacman_conf, updated)?;
    }

    Ok(())
}

fn enable_multilib_repo() -> Result<()> {
    let pacman_conf = "/etc/pacman.conf";
    if !Path::new(pacman_conf).exists() {
        return Ok(());
    }

    let content = fs::read_to_string(pacman_conf)?;
    if content.contains("\n[multilib]\n")
        && content.contains("\nInclude = /etc/pacman.d/mirrorlist\n")
    {
        return Ok(());
    }

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

    run_command(
        "useradd",
        &["-m", "-G", "wheel", "-s", "/usr/bin/zsh", &config.username],
    )?;

    let root_auth = format!("root:{}", config.password);
    let user_auth = format!("{}:{}", config.username, config.password);
    run_command_stdin("chpasswd", &[], &format!("{}\n{}", root_auth, user_auth))?;

    let sudoers_file = "/etc/sudoers";
    if Path::new(sudoers_file).exists() {
        let sudoers = fs::read_to_string(sudoers_file)?;
        let mut new_sudoers = sudoers.replace("# %wheel ALL=(ALL:ALL) ALL", "%wheel ALL=(ALL:ALL) ALL");
        
        // Ensure sudoers.d is included
        if !new_sudoers.contains("@includedir /etc/sudoers.d") && !new_sudoers.contains("#includedir /etc/sudoers.d") {
            new_sudoers.push_str("\n@includedir /etc/sudoers.d\n");
        } else {
            new_sudoers = new_sudoers.replace("#includedir /etc/sudoers.d", "@includedir /etc/sudoers.d");
        }
        
        fs::write(sudoers_file, new_sudoers)?;
    }

    let zshrc = r#"
# slate-desktop: modern zshrc
alias ls='eza --icons'
alias l='eza -lh --icons'
alias ll='eza -lha --icons'
alias cat='bat'
alias grep='rg'
alias cd='zoxide'

eval "$(starship init zsh)"
eval "$(zoxide init zsh)"

export PATH=$PATH:$HOME/.local/bin
"#;
    let user_home = format!("/home/{}", config.username);
    fs::write(format!("{}/.zshrc", user_home), zshrc)?;

    fs::create_dir_all("/etc/systemd/system/getty@tty1.service.d")?;
    let autologin_override = format!(
        r#"[Service]
ExecStart=
ExecStart=-/usr/bin/agetty --autologin {} --noclear %I $TERM
"#,
        config.username
    );
    fs::write(
        "/etc/systemd/system/getty@tty1.service.d/override.conf",
        autologin_override,
    )?;

    fs::create_dir_all(format!("{}/.config", user_home))?;
    fs::write(
        format!("{}/.config/starship.toml", user_home),
        "[add_newline]\ninsert_newline = false\n",
    )?;

    run_command(
        "chown",
        &[
            "-R",
            &format!("{}:{}", config.username, config.username),
            &user_home,
        ],
    )?;

    Ok(())
}

fn configure_shell(config: &UserInfo) -> Result<()> {
    println!("[Phase 2/3] shell provisioning");

    let user_home = format!("/home/{}", config.username);
    let shell_repo = provision_shell_repo()?;
    let requirements = parse_requirements_file(&shell_repo.join("requirements.txt"))
        .context("Failed to parse shell requirements")?;
    let packages = merged_package_plan(&requirements);

    install_shell_packages(config, &user_home, &packages)?;
    deploy_shell_files(config, &user_home, &shell_repo)?;
    apply_shell_overrides(config, &user_home)?;
    configure_shell_plugins(config, &user_home)?;

    let _ = fs::remove_dir_all(&shell_repo);

    Ok(())
}

fn provision_shell_repo() -> Result<PathBuf> {
    println!("  > Cloning Slate shell repo...");

    let shell_repo = PathBuf::from(SHELL_REPO_DIR);
    if shell_repo.exists() {
        fs::remove_dir_all(&shell_repo).context("Failed to remove existing shell checkout")?;
    }

    run_command(
        "git",
        &["clone", "--depth=1", SHELL_REPO_URL, SHELL_REPO_DIR],
    )?;

    Ok(shell_repo)
}

fn install_shell_packages(config: &UserInfo, user_home: &str, packages: &[String]) -> Result<()> {
    println!("  > Installing Slate shell packages with Ax...");

    if packages.is_empty() {
        return Ok(());
    }

    let sudoers_rule = format!("{} ALL=(ALL) NOPASSWD: ALL\n", config.username);
    fs::write(TEMP_SUDOERS_FILE, sudoers_rule)?;
    fs::set_permissions(TEMP_SUDOERS_FILE, fs::Permissions::from_mode(0o440))?;

    let mut args: Vec<&str> = vec!["-S", "--needed", "--noconfirm"];
    args.extend(packages.iter().map(String::as_str));

    let result = run_command_as_user(config, user_home, "ax", &args);
    let cleanup = fs::remove_file(TEMP_SUDOERS_FILE);

    if let Err(err) = cleanup {
        cleanup_failed(err)?;
    }

    result
}

fn deploy_shell_files(config: &UserInfo, user_home: &str, shell_repo: &Path) -> Result<()> {
    println!("  > Deploying shell files...");

    let config_src = shell_repo.join(".config");
    let local_src = shell_repo.join(".local");
    let config_dst = Path::new(user_home).join(".config");
    let local_dst = Path::new(user_home).join(".local");

    fs::create_dir_all(&config_dst)?;
    copy_dir_contents(&config_src, &config_dst)?;

    if local_src.exists() {
        fs::create_dir_all(&local_dst)?;
        copy_dir_contents(&local_src, &local_dst)?;
    }

    let wallpaper_script = config_dst.join("quickshell/scripts/wallpaper.sh");
    if wallpaper_script.exists() {
        fs::set_permissions(&wallpaper_script, fs::Permissions::from_mode(0o755))?;
    }

    run_command(
        "chown",
        &[
            "-R",
            &format!("{}:{}", config.username, config.username),
            user_home,
        ],
    )?;

    Ok(())
}

fn apply_shell_overrides(config: &UserInfo, user_home: &str) -> Result<()> {
    println!("  > Applying Slate-specific shell overrides...");

    let input_conf = Path::new(user_home).join(".config/hypr/conf.d/input.conf");
    if input_conf.exists() {
        let content = fs::read_to_string(&input_conf)?;
        let updated = set_hypr_keymap(&content, &config.keymap);
        fs::write(&input_conf, updated)?;
    }

    Ok(())
}

fn configure_shell_plugins(config: &UserInfo, user_home: &str) -> Result<()> {
    println!("  > Enabling Hyprland shell plugin...");

    run_command_as_user(config, user_home, "hyprpm", &["update"])?;
    run_command_as_user(config, user_home, "hyprpm", &["add", SHELL_PLUGIN_REPO])?;
    run_command_as_user(config, user_home, "hyprpm", &["enable", "hyprscrolling"])?;

    Ok(())
}

fn configure_tools(config: &UserInfo) -> Result<()> {
    println!("[Phase 3/3] post-install configuration");

    if !config.git_name.is_empty() {
        let user_home = format!("/home/{}", config.username);
        let gitconfig = format!(
            r#"[user]
	name = {}
	email = {}
"#,
            config.git_name, config.git_email
        );
        fs::write(format!("{}/.gitconfig", user_home), gitconfig)?;
        run_command(
            "chown",
            &[
                &format!("{}:{}", config.username, config.username),
                &format!("{}/.gitconfig", user_home),
            ],
        )?;
    }

    Ok(())
}

fn configure_post_services() -> Result<()> {
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

fn slate_shell_packages() -> Vec<&'static str> {
    vec![
        "hyprlock",
        "hypridle",
        "xdg-desktop-portal-hyprland",
        "qt6-wayland",
        "pipewire",
        "wireplumber",
        "pipewire-pulse",
        "pipewire-alsa",
        "firefox",
        "starship",
        "eza",
        "bat",
        "zoxide",
        "fzf",
        "ripgrep",
        "networkmanager",
        "network-manager-applet",
        "blueman",
        "easyeffects",
        "grim",
        "slurp",
        "imagemagick",
        "sqlite",
        "upower",
        "wl-clipboard",
        "wlsunset",
        "wtype",
        "zbar",
        "glib2",
        "power-profiles-daemon",
        "ttf-roboto",
        "ttf-dejavu",
        "ttf-liberation",
        "noto-fonts",
        "noto-fonts-cjk",
        "noto-fonts-emoji",
        "ttf-nerd-fonts-symbols",
        "gpu-screen-recorder",
        "adw-gtk-theme",
    ]
}

fn merged_package_plan(shell_requirements: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut packages = Vec::new();

    for pkg in slate_shell_packages()
        .into_iter()
        .map(|pkg| pkg.to_string())
        .chain(shell_requirements.iter().cloned())
    {
        if seen.insert(pkg.clone()) {
            packages.push(pkg);
        }
    }

    packages
}

fn parse_requirements_file(path: &Path) -> Result<Vec<String>> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("Failed to read requirements file at {}", path.display()))?;
    Ok(parse_requirements(&raw))
}

fn parse_requirements(raw: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut packages = Vec::new();

    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let normalized = normalize_package_name(trimmed);
        if seen.insert(normalized.clone()) {
            packages.push(normalized);
        }
    }

    packages
}

fn normalize_package_name(name: &str) -> String {
    match name.trim() {
        "python3" => "python",
        "pactl" => "libpulse",
        "fonts-inter" => "inter-font",
        "fonts-roboto-mono" => "ttf-roboto-mono",
        "nm-connection-editor" => "network-manager-applet",
        other => other,
    }
    .to_string()
}

fn set_hypr_keymap(content: &str, keymap: &str) -> String {
    let mut updated = Vec::new();
    let mut replaced = false;

    for line in content.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("kb_layout") {
            let indent_len = line.len() - trimmed.len();
            let indent = " ".repeat(indent_len);
            updated.push(format!("{indent}kb_layout    = {keymap}"));
            replaced = true;
        } else {
            updated.push(line.to_string());
        }
    }

    if !replaced {
        updated.push(format!("    kb_layout    = {keymap}"));
    }

    let mut rendered = updated.join("\n");
    if content.ends_with('\n') {
        rendered.push('\n');
    }
    rendered
}

fn copy_dir_contents(src: &Path, dst: &Path) -> Result<()> {
    if !src.exists() {
        bail!("Required path missing: {}", src.display());
    }

    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        copy_path_recursive(&src_path, &dst_path)?;
    }

    Ok(())
}

fn copy_path_recursive(src: &Path, dst: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(src)?;

    if metadata.is_dir() {
        fs::create_dir_all(dst)?;
        fs::set_permissions(
            dst,
            fs::Permissions::from_mode(metadata.permissions().mode()),
        )?;
        for entry in fs::read_dir(src)? {
            let entry = entry?;
            copy_path_recursive(&entry.path(), &dst.join(entry.file_name()))?;
        }
    } else if metadata.file_type().is_symlink() {
        let target = fs::read_link(src)?;
        if dst.exists() {
            let _ = fs::remove_file(dst);
        }
        std::os::unix::fs::symlink(target, dst)?;
    } else {
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(src, dst)?;
        fs::set_permissions(
            dst,
            fs::Permissions::from_mode(metadata.permissions().mode()),
        )?;
    }

    Ok(())
}

fn run_command(cmd: &str, args: &[&str]) -> Result<()> {
    println!("    $ {} {}", cmd, args.join(" "));
    let output = Command::new(cmd)
        .args(args)
        .output()
        .with_context(|| format!("Failed to run {}", cmd))?;

    if !output.status.success() {
        let stderr = sanitize_for_tui(&String::from_utf8_lossy(&output.stderr));
        let stderr = stderr.trim().to_string();
        if stderr.is_empty() {
            bail!("Command failed: {}", cmd);
        }
        bail!("Command failed: {}: {}", cmd, stderr);
    }
    Ok(())
}

fn run_command_as_user(config: &UserInfo, _user_home: &str, cmd: &str, args: &[&str]) -> Result<()> {
    println!("    $ runuser -u {} -- {} {}", config.username, cmd, args.join(" "));

    let mut child = Command::new("runuser");
    child
        .arg("-u")
        .arg(&config.username)
        .arg("--")
        .arg(cmd)
        .args(args)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());

    let status = child.status().context("Failed to run command as user")?;
    if !status.success() {
        bail!("Command failed: {} (exit {})", cmd, status.code().unwrap_or(-1));
    }

    Ok(())
}

fn setup_autostart(config: &UserInfo) -> Result<()> {
    println!("  > Setting up Hyprland auto-start...");

    let user_home = format!("/home/{}", config.username);
    let zprofile = r#"
# slate-desktop: auto-start hyprland on tty1
if [[ -z $DISPLAY ]] && [[ $(tty) = /dev/tty1 ]] && command -v Hyprland >/dev/null; then
  exec Hyprland
fi
"#;
    fs::write(format!("{}/.zprofile", user_home), zprofile)?;
    run_command(
        "chown",
        &[
            &format!("{}:{}", config.username, config.username),
            &format!("{}/.zprofile", user_home),
        ],
    )?;

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
        let stderr = sanitize_for_tui(&String::from_utf8_lossy(&output.stderr));
        let stderr = stderr.trim().to_string();
        if stderr.is_empty() {
            bail!("Command {} failed", cmd);
        }
        bail!("Command {} failed: {}", cmd, stderr);
    }
    Ok(())
}

fn sanitize_for_tui(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '\u{1b}' {
            if let Some('[') = chars.peek().copied() {
                let _ = chars.next();
                for next in chars.by_ref() {
                    if ('@'..='~').contains(&next) {
                        break;
                    }
                }
                continue;
            }
            if let Some(']') = chars.peek().copied() {
                let _ = chars.next();
                for next in chars.by_ref() {
                    if next == '\u{7}' {
                        break;
                    }
                }
                continue;
            }
            continue;
        }

        if ch == '\r' || (ch.is_control() && ch != '\n' && ch != '\t') {
            continue;
        }

        out.push(ch);
    }

    out
}

fn cleanup_failed(err: std::io::Error) -> Result<()> {
    bail!("Failed to remove temporary sudoers file: {}", err);
}

#[cfg(test)]
mod tests {
    use super::{parse_requirements, set_hypr_keymap};

    #[test]
    fn parses_requirements_and_normalizes_known_aliases() {
        let parsed = parse_requirements(
            r#"
            quickshell
            python3
            # comment

            nm-connection-editor
            fonts-inter
            fonts-roboto-mono
            python3
            "#,
        );

        assert_eq!(
            parsed,
            vec![
                "quickshell",
                "python",
                "network-manager-applet",
                "inter-font",
                "ttf-roboto-mono",
            ]
        );
    }

    #[test]
    fn rewrites_hypr_keymap_in_place() {
        let updated = set_hypr_keymap(
            "input {\n    kb_layout    = us\n    follow_mouse = 1\n}\n",
            "de",
        );

        assert!(updated.contains("kb_layout    = de"));
        assert!(!updated.contains("kb_layout    = us"));
    }
}
