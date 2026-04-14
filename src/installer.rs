use crate::system;
use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::Sender;
use std::thread;
use std::time::{Duration, Instant};

pub const TARGET_ROOT: &str = "/mnt";
pub const TARGET_PLAN_PATH: &str = "/mnt/etc/slate/install-plan.json";
const TARGET_CHECKPOINT_PATH: &str = "/mnt/etc/slate/checkpoint.json";
const HOST_PLAN_PATH: &str = "/tmp/slate-install-plan.json";
const SHELL_REPO_URL: &str = "https://github.com/manpreet113/shell.git";
const SHELL_REPO_DIR: &str = "/tmp/slate-shell";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallPlan {
    pub disk: String,
    pub hostname: String,
    pub username: String,
    pub password: String,
    pub keymap: String,
    pub timezone: String,
    pub git_name: String,
    pub git_email: String,
    pub desktop_profile: String,
}

impl InstallPlan {
    pub fn validate(&self) -> Result<()> {
        if self.disk.trim().is_empty() {
            bail!("No target disk selected");
        }
        for (name, value) in [
            ("hostname", &self.hostname),
            ("username", &self.username),
            ("password", &self.password),
            ("keymap", &self.keymap),
            ("timezone", &self.timezone),
        ] {
            if value.trim().is_empty() {
                bail!("{} cannot be empty", name);
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum StageId {
    Collect,
    PrepareDisk,
    Bootstrap,
    StageApply,
    Verify,
    Finalize,
}

impl StageId {
    pub const ALL: [StageId; 6] = [
        StageId::Collect,
        StageId::PrepareDisk,
        StageId::Bootstrap,
        StageId::StageApply,
        StageId::Verify,
        StageId::Finalize,
    ];

    pub fn label(self) -> &'static str {
        match self {
            StageId::Collect => "Collect",
            StageId::PrepareDisk => "Prepare Disk",
            StageId::Bootstrap => "Bootstrap",
            StageId::StageApply => "Stage Apply",
            StageId::Verify => "Verify",
            StageId::Finalize => "Finalize",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Checkpoint {
    pub active_stage: Option<StageId>,
    pub completed_stages: Vec<StageId>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone)]
pub enum InstallEvent {
    StageStarted(StageId),
    StageFinished(StageId),
    Log(String),
    Finished,
    Failed {
        stage: Option<StageId>,
        message: String,
    },
}

#[derive(Clone)]
pub struct EventSink {
    tx: Sender<InstallEvent>,
}

impl EventSink {
    pub fn new(tx: Sender<InstallEvent>) -> Self {
        Self { tx }
    }

    pub fn log<S: Into<String>>(&self, message: S) {
        let _ = self.tx.send(InstallEvent::Log(message.into()));
    }

    pub fn stage_started(&self, stage: StageId) {
        let _ = self.tx.send(InstallEvent::StageStarted(stage));
    }

    pub fn stage_finished(&self, stage: StageId) {
        let _ = self.tx.send(InstallEvent::StageFinished(stage));
    }

    pub fn failed(&self, stage: Option<StageId>, message: String) {
        let _ = self.tx.send(InstallEvent::Failed { stage, message });
    }

    pub fn finished(&self) {
        let _ = self.tx.send(InstallEvent::Finished);
    }
}

pub fn run_install(plan: InstallPlan, sink: EventSink) {
    let result = (|| -> Result<()> {
        plan.validate()?;
        persist_host_plan(&plan)?;
        let mut ctx = InstallContext::new(plan, sink.clone());
        ctx.execute_host()
    })();

    match result {
        Ok(()) => sink.finished(),
        Err(err) => sink.failed(None, format!("{:#}", err)),
    }
}

pub fn run_stage_apply() -> Result<()> {
    let plan = read_plan_from(Path::new("/etc/slate/install-plan.json"))?;
    let mut ctx = ChrootContext::new(plan);
    ctx.execute()
}

pub fn read_plan_from(path: &Path) -> Result<InstallPlan> {
    let raw =
        fs::read_to_string(path).with_context(|| format!("Failed to read {}", path.display()))?;
    let plan: InstallPlan = serde_json::from_str(&raw)
        .with_context(|| format!("Failed to parse {}", path.display()))?;
    plan.validate()?;
    Ok(plan)
}

fn persist_host_plan(plan: &InstallPlan) -> Result<()> {
    fs::write(HOST_PLAN_PATH, serde_json::to_vec_pretty(plan)?)
        .context("Failed to persist host install plan")?;
    Ok(())
}

struct InstallContext {
    plan: InstallPlan,
    sink: EventSink,
    checkpoint: Checkpoint,
    current_stage: Option<StageId>,
}

impl InstallContext {
    fn new(plan: InstallPlan, sink: EventSink) -> Self {
        Self {
            plan,
            sink,
            checkpoint: Checkpoint::default(),
            current_stage: None,
        }
    }

    fn execute_host(&mut self) -> Result<()> {
        self.run_stage(StageId::Collect, |ctx| ctx.collect())?;
        self.run_stage(StageId::PrepareDisk, |ctx| ctx.prepare_disk())?;
        self.run_stage(StageId::Bootstrap, |ctx| ctx.bootstrap())?;
        self.run_stage(StageId::StageApply, |ctx| ctx.stage_apply())?;
        self.run_stage(StageId::Verify, |ctx| ctx.verify())?;
        self.run_stage(StageId::Finalize, |ctx| ctx.finalize())?;
        Ok(())
    }

    fn run_stage<F>(&mut self, stage: StageId, f: F) -> Result<()>
    where
        F: FnOnce(&mut Self) -> Result<()>,
    {
        self.current_stage = Some(stage);
        self.checkpoint.active_stage = Some(stage);
        self.checkpoint.last_error = None;
        self.persist_checkpoint()?;
        self.sink.stage_started(stage);
        self.sink.log(format!("== {} ==", stage.label()));

        match f(self) {
            Ok(()) => {
                self.checkpoint.active_stage = None;
                self.checkpoint.completed_stages.push(stage);
                self.persist_checkpoint()?;
                self.sink.stage_finished(stage);
                Ok(())
            }
            Err(err) => {
                let rendered = format!("{} failed: {:#}", stage.label(), err);
                self.checkpoint.last_error = Some(rendered.clone());
                self.persist_checkpoint()?;
                self.sink.failed(Some(stage), rendered.clone());
                Err(anyhow!(rendered))
            }
        }
    }

    fn collect(&mut self) -> Result<()> {
        if !nix::unistd::Uid::effective().is_root() {
            bail!("Root privileges are required");
        }
        if !Path::new("/sys/firmware/efi").exists() {
            bail!("UEFI mode is required");
        }
        if !Path::new(&self.plan.disk).exists() {
            bail!("Target disk not found: {}", self.plan.disk);
        }

        let tools = [
            "sgdisk",
            "mkfs.vfat",
            "mkfs.btrfs",
            "mount",
            "umount",
            "pacstrap",
            "genfstab",
            "arch-chroot",
            "bootctl",
            "systemctl",
            "git",
            "curl",
        ];
        for tool in tools {
            require_command(tool)?;
        }

        let status = Command::new("curl")
            .args(["-I", "--connect-timeout", "10", "https://archlinux.org"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .context("Failed to run curl network check")?;
        if !status.success() {
            bail!("Network check failed");
        }

        self.sink.log(format!("Target disk: {}", self.plan.disk));
        self.sink.log(format!(
            "Installing desktop profile: {}",
            self.plan.desktop_profile
        ));
        Ok(())
    }

    fn prepare_disk(&mut self) -> Result<()> {
        let disk = self.plan.disk.as_str();
        let runner = CommandRunner::new(&self.sink, Some(StageId::PrepareDisk));

        runner.run(
            "umount",
            &["-R", TARGET_ROOT],
            Some(Duration::from_secs(20)),
            true,
        )?;
        runner.run(
            "sgdisk",
            &["--zap-all", disk],
            Some(Duration::from_secs(20)),
            false,
        )?;
        runner.run(
            "sgdisk",
            &["-o", disk],
            Some(Duration::from_secs(20)),
            false,
        )?;
        runner.run(
            "sgdisk",
            &["-n", "1:0:+1G", "-t", "1:ef00", "-c", "1:EFI", disk],
            Some(Duration::from_secs(20)),
            false,
        )?;
        runner.run(
            "sgdisk",
            &["-n", "2:0:0", "-t", "2:8300", "-c", "2:ROOT", disk],
            Some(Duration::from_secs(20)),
            false,
        )?;

        let efi = system::partition_path(disk, 1);
        let root = system::partition_path(disk, 2);
        wait_for_path(&efi, Duration::from_secs(15))?;
        wait_for_path(&root, Duration::from_secs(15))?;

        runner.run(
            "mkfs.vfat",
            &["-F", "32", "-n", "SLATE_EFI", &efi],
            Some(Duration::from_secs(30)),
            false,
        )?;
        runner.run(
            "mkfs.btrfs",
            &["-f", "-L", "SLATE_ROOT", &root],
            Some(Duration::from_secs(60)),
            false,
        )?;

        fs::create_dir_all(TARGET_ROOT)?;
        let mut mounts = MountTable::default();
        mounts.mount(
            &runner,
            &root,
            TARGET_ROOT,
            &["-o", "rw,noatime,compress=zstd,space_cache=v2"],
        )?;

        for subvol in ["@", "@home", "@log", "@pkg", "@snapshots"] {
            runner.run(
                "btrfs",
                &["subvolume", "create", &format!("{TARGET_ROOT}/{subvol}")],
                Some(Duration::from_secs(30)),
                false,
            )?;
        }

        mounts.unmount(&runner, TARGET_ROOT)?;
        mounts.mount(
            &runner,
            &root,
            TARGET_ROOT,
            &["-o", "rw,noatime,compress=zstd,space_cache=v2,subvol=@"],
        )?;

        for dir in [
            "/mnt/home",
            "/mnt/var/log",
            "/mnt/var/cache/pacman/pkg",
            "/mnt/.snapshots",
            "/mnt/boot",
            "/mnt/etc/slate",
        ] {
            fs::create_dir_all(dir)?;
        }

        mounts.mount(
            &runner,
            &root,
            "/mnt/home",
            &["-o", "rw,noatime,compress=zstd,space_cache=v2,subvol=@home"],
        )?;
        mounts.mount(
            &runner,
            &root,
            "/mnt/var/log",
            &["-o", "rw,noatime,compress=zstd,space_cache=v2,subvol=@log"],
        )?;
        mounts.mount(
            &runner,
            &root,
            "/mnt/var/cache/pacman/pkg",
            &["-o", "rw,noatime,compress=zstd,space_cache=v2,subvol=@pkg"],
        )?;
        mounts.mount(
            &runner,
            &root,
            "/mnt/.snapshots",
            &[
                "-o",
                "rw,noatime,compress=zstd,space_cache=v2,subvol=@snapshots",
            ],
        )?;
        mounts.mount(&runner, &efi, "/mnt/boot", &[])?;
        Ok(())
    }

    fn bootstrap(&mut self) -> Result<()> {
        let runner = CommandRunner::new(&self.sink, Some(StageId::Bootstrap));
        let packages = [
            "base",
            "linux",
            "linux-firmware",
            "base-devel",
            "btrfs-progs",
            "sudo",
            "networkmanager",
            "systemd",
            "git",
            "curl",
            "zsh",
            "bootctl",
            "intel-ucode",
            "amd-ucode",
        ];

        self.sink.log("Bootstrapping base system...");
        let mut args = vec!["-K", TARGET_ROOT];
        args.extend(packages);
        runner.run("pacstrap", &args, Some(Duration::from_secs(1800)), false)?;

        let output = Command::new("genfstab")
            .args(["-U", TARGET_ROOT])
            .output()
            .context("Failed to run genfstab")?;
        if !output.status.success() {
            bail!("genfstab failed");
        }
        fs::write("/mnt/etc/fstab", output.stdout).context("Failed to write fstab")?;

        fs::create_dir_all("/mnt/etc/slate")?;
        fs::write(TARGET_PLAN_PATH, serde_json::to_vec_pretty(&self.plan)?)
            .context("Failed to write target install plan")?;
        self.persist_checkpoint()?;

        let current_exe =
            std::env::current_exe().context("Failed to resolve current executable")?;
        fs::create_dir_all("/mnt/usr/local/bin")?;
        fs::copy(current_exe, "/mnt/usr/local/bin/slate")
            .context("Failed to copy slate binary into target")?;
        runner.run(
            "chmod",
            &["+x", "/mnt/usr/local/bin/slate"],
            Some(Duration::from_secs(10)),
            false,
        )?;
        Ok(())
    }

    fn stage_apply(&mut self) -> Result<()> {
        let runner = CommandRunner::new(&self.sink, Some(StageId::StageApply));
        runner.run(
            "arch-chroot",
            &[TARGET_ROOT, "slate", "chroot-stage"],
            Some(Duration::from_secs(3600)),
            false,
        )?;
        Ok(())
    }

    fn verify(&mut self) -> Result<()> {
        let checks = [
            "/mnt/boot/loader/loader.conf",
            "/mnt/boot/loader/entries/slate.conf",
            "/mnt/home",
            "/mnt/etc/hostname",
            "/mnt/etc/slate/install-plan.json",
        ];
        for path in checks {
            if !Path::new(path).exists() {
                bail!("Verification failed: missing {}", path);
            }
        }

        let username = &self.plan.username;
        if !Path::new(&format!("/mnt/home/{username}")).exists() {
            bail!("Verification failed: missing user home for {}", username);
        }
        Ok(())
    }

    fn finalize(&mut self) -> Result<()> {
        self.sink.log("Install finished successfully.");
        Ok(())
    }

    fn persist_checkpoint(&self) -> Result<()> {
        let serialized = serde_json::to_vec_pretty(&self.checkpoint)?;
        let target_path = Path::new(TARGET_CHECKPOINT_PATH);
        if let Some(parent) = target_path.parent() {
            fs::create_dir_all(parent)?;
        }
        if Path::new(TARGET_ROOT).exists() {
            let _ = fs::write(target_path, &serialized);
        }
        fs::write("/tmp/slate-checkpoint.json", serialized)
            .context("Failed to persist checkpoint")?;
        Ok(())
    }
}

#[derive(Default)]
struct MountTable {
    targets: Vec<String>,
}

impl MountTable {
    fn mount(
        &mut self,
        runner: &CommandRunner<'_>,
        source: &str,
        target: &str,
        options: &[&str],
    ) -> Result<()> {
        fs::create_dir_all(target)?;
        let mut args = Vec::new();
        args.extend_from_slice(options);
        args.push(source);
        args.push(target);
        runner.run("mount", &args, Some(Duration::from_secs(30)), false)?;
        self.targets.push(target.to_string());
        Ok(())
    }

    fn unmount(&mut self, runner: &CommandRunner<'_>, target: &str) -> Result<()> {
        runner.run("umount", &[target], Some(Duration::from_secs(20)), false)?;
        self.targets.retain(|item| item != target);
        Ok(())
    }
}

impl Drop for MountTable {
    fn drop(&mut self) {
        for target in self.targets.iter().rev() {
            let _ = Command::new("umount").arg("-l").arg(target).status();
        }
    }
}

struct CommandRunner<'a> {
    sink: &'a EventSink,
    stage: Option<StageId>,
}

impl<'a> CommandRunner<'a> {
    fn new(sink: &'a EventSink, stage: Option<StageId>) -> Self {
        Self { sink, stage }
    }

    fn run(
        &self,
        cmd: &str,
        args: &[&str],
        timeout: Option<Duration>,
        allow_failure: bool,
    ) -> Result<()> {
        self.sink.log(format!("$ {} {}", cmd, args.join(" ")));
        let mut child = Command::new(cmd)
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .with_context(|| format!("Failed to spawn {}", cmd))?;

        let stdout = child.stdout.take().context("Failed to capture stdout")?;
        let stderr = child.stderr.take().context("Failed to capture stderr")?;
        let sink_out = self.sink.clone();
        let sink_err = self.sink.clone();
        let out_thread = thread::spawn(move || stream_lines(stdout, sink_out, false));
        let err_thread = thread::spawn(move || stream_lines(stderr, sink_err, true));

        let deadline = timeout.map(|value| Instant::now() + value);
        loop {
            if let Some(status) = child.try_wait().context("Failed to poll command")? {
                let stdout_tail = out_thread.join().unwrap_or_default();
                let stderr_tail = err_thread.join().unwrap_or_default();
                if status.success() || allow_failure {
                    return Ok(());
                }
                let code = status.code().unwrap_or(-1);
                let tail = format_command_tail(&stdout_tail, &stderr_tail);
                let stage = self
                    .stage
                    .map(StageId::label)
                    .unwrap_or("Unknown stage")
                    .to_string();
                if tail.is_empty() {
                    bail!("{}: command failed: {} (exit {})", stage, cmd, code);
                }
                bail!(
                    "{}: command failed: {} (exit {})\n{}",
                    stage,
                    cmd,
                    code,
                    tail
                );
            }

            if let Some(deadline) = deadline {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    let _ = out_thread.join();
                    let _ = err_thread.join();
                    bail!("Timed out while running {}", cmd);
                }
            }
            thread::sleep(Duration::from_millis(100));
        }
    }
}

fn stream_lines<R: std::io::Read>(reader: R, sink: EventSink, is_stderr: bool) -> Vec<String> {
    let mut tail = Vec::new();
    let reader = BufReader::new(reader);
    for line in reader.lines().map_while(Result::ok) {
        let clean = sanitize_for_log(&line);
        if clean.is_empty() {
            continue;
        }
        if is_stderr {
            sink.log(format!("[stderr] {}", clean));
        } else {
            sink.log(clean.clone());
        }
        tail.push(clean);
        if tail.len() > 20 {
            tail.remove(0);
        }
    }
    tail
}

fn format_command_tail(stdout_tail: &[String], stderr_tail: &[String]) -> String {
    let mut lines = Vec::new();
    if !stderr_tail.is_empty() {
        lines.push("Recent stderr:".to_string());
        lines.extend(stderr_tail.iter().cloned());
    } else if !stdout_tail.is_empty() {
        lines.push("Recent stdout:".to_string());
        lines.extend(stdout_tail.iter().cloned());
    }
    lines.join("\n")
}

fn sanitize_for_log(input: &str) -> String {
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
            continue;
        }
        if ch == '\r' || (ch.is_control() && ch != '\n' && ch != '\t') {
            continue;
        }
        out.push(ch);
    }
    out.trim().to_string()
}

fn wait_for_path(path: &str, timeout: Duration) -> Result<()> {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if Path::new(path).exists() {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(200));
    }
    bail!("Timed out waiting for device {}", path)
}

fn require_command(cmd: &str) -> Result<()> {
    let status = Command::new("sh")
        .arg("-c")
        .arg(format!("command -v {}", cmd))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .with_context(|| format!("Failed to check command {}", cmd))?;
    if !status.success() {
        bail!("Required command is missing: {}", cmd);
    }
    Ok(())
}

struct ChrootContext {
    plan: InstallPlan,
}

impl ChrootContext {
    fn new(plan: InstallPlan) -> Self {
        Self { plan }
    }

    fn execute(&mut self) -> Result<()> {
        self.base_config()?;
        self.user_config()?;
        self.boot_config()?;
        self.desktop_packages()?;
        self.desktop_assets()?;
        self.desktop_finalize()?;
        Ok(())
    }

    fn base_config(&self) -> Result<()> {
        fs::create_dir_all("/etc/slate")?;
        self.write_hostname()?;
        self.write_locale()?;
        self.write_timezone()?;
        fs::write(
            "/etc/vconsole.conf",
            format!("KEYMAP={}\n", self.plan.keymap),
        )?;
        run_simple("systemctl", &["enable", "NetworkManager"])?;
        run_simple("systemctl", &["enable", "systemd-timesyncd"])?;
        Ok(())
    }

    fn user_config(&self) -> Result<()> {
        if !user_exists(&self.plan.username)? {
            run_simple(
                "useradd",
                &[
                    "-m",
                    "-G",
                    "wheel",
                    "-s",
                    "/usr/bin/zsh",
                    &self.plan.username,
                ],
            )?;
        }
        run_with_input(
            "chpasswd",
            &[],
            &format!(
                "root:{}\n{}:{}\n",
                self.plan.password, self.plan.username, self.plan.password
            ),
        )?;

        let sudoers = "/etc/sudoers";
        let content = fs::read_to_string(sudoers).context("Failed to read sudoers")?;
        let updated = if content.contains("%wheel ALL=(ALL:ALL) ALL") {
            content
        } else {
            content.replace("# %wheel ALL=(ALL:ALL) ALL", "%wheel ALL=(ALL:ALL) ALL")
        };
        fs::write(sudoers, updated)?;
        Ok(())
    }

    fn boot_config(&self) -> Result<()> {
        run_simple("bootctl", &["install"])?;
        let root_device = system::find_mount_source(TARGET_ROOT)?
            .unwrap_or_else(|| system::partition_path(&self.plan.disk, 2));
        let root_uuid = system::get_uuid(&root_device)?;
        fs::create_dir_all("/boot/loader/entries")?;
        fs::write(
            "/boot/loader/loader.conf",
            "default slate.conf\ntimeout 3\nconsole-mode max\n",
        )?;
        fs::write(
            "/boot/loader/entries/slate.conf",
            format!(
                "title Slate\nlinux /vmlinuz-linux\ninitrd /intel-ucode.img\ninitrd /amd-ucode.img\ninitrd /initramfs-linux.img\noptions root=UUID={} rw rootflags=subvol=@\n",
                root_uuid
            ),
        )?;
        Ok(())
    }

    fn desktop_packages(&self) -> Result<()> {
        let packages = [
            "hyprland",
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
            "network-manager-applet",
            "blueman",
            "grim",
            "slurp",
            "imagemagick",
            "upower",
            "wl-clipboard",
            "wlsunset",
            "wtype",
            "noto-fonts",
            "noto-fonts-emoji",
            "ttf-dejavu",
        ];
        let mut args = vec!["-S", "--needed", "--noconfirm"];
        args.extend(packages);
        run_simple("pacman", &args)?;
        Ok(())
    }

    fn desktop_assets(&self) -> Result<()> {
        if Path::new(SHELL_REPO_DIR).exists() {
            fs::remove_dir_all(SHELL_REPO_DIR).context("Failed to clean shell repo cache")?;
        }
        run_simple(
            "git",
            &["clone", "--depth=1", SHELL_REPO_URL, SHELL_REPO_DIR],
        )?;

        let user_home = PathBuf::from(format!("/home/{}", self.plan.username));
        let config_dst = user_home.join(".config");
        let local_dst = user_home.join(".local");
        fs::create_dir_all(&config_dst)?;

        copy_dir_contents(
            Path::new(SHELL_REPO_DIR).join(".config").as_path(),
            &config_dst,
        )?;
        let local_src = Path::new(SHELL_REPO_DIR).join(".local");
        if local_src.exists() {
            fs::create_dir_all(&local_dst)?;
            copy_dir_contents(&local_src, &local_dst)?;
        }

        if !self.plan.git_name.trim().is_empty() {
            fs::write(
                user_home.join(".gitconfig"),
                format!(
                    "[user]\n\tname = {}\n\temail = {}\n",
                    self.plan.git_name, self.plan.git_email
                ),
            )?;
        }

        run_simple(
            "chown",
            &[
                "-R",
                &format!("{}:{}", self.plan.username, self.plan.username),
                user_home.to_string_lossy().as_ref(),
            ],
        )?;
        let _ = fs::remove_dir_all(SHELL_REPO_DIR);
        Ok(())
    }

    fn desktop_finalize(&self) -> Result<()> {
        let user_home = PathBuf::from(format!("/home/{}", self.plan.username));
        fs::write(
            user_home.join(".zprofile"),
            "if [[ -z $DISPLAY ]] && [[ $(tty) = /dev/tty1 ]] && command -v Hyprland >/dev/null; then\n  exec Hyprland\nfi\n",
        )?;
        fs::write(
            user_home.join(".zshrc"),
            "alias ls='eza --icons'\nalias ll='eza -lha --icons'\nalias cat='bat'\nalias grep='rg'\neval \"$(starship init zsh)\"\neval \"$(zoxide init zsh)\"\nexport PATH=$PATH:$HOME/.local/bin\n",
        )?;
        run_simple(
            "chown",
            &[
                "-R",
                &format!("{}:{}", self.plan.username, self.plan.username),
                user_home.to_string_lossy().as_ref(),
            ],
        )?;
        Ok(())
    }

    fn write_hostname(&self) -> Result<()> {
        fs::write("/etc/hostname", format!("{}\n", self.plan.hostname))?;
        Ok(())
    }

    fn write_locale(&self) -> Result<()> {
        let locale_gen = "/etc/locale.gen";
        let content = fs::read_to_string(locale_gen).context("Failed to read locale.gen")?;
        let updated = content.replace("#en_US.UTF-8 UTF-8", "en_US.UTF-8 UTF-8");
        fs::write(locale_gen, updated)?;
        fs::write("/etc/locale.conf", "LANG=en_US.UTF-8\n")?;
        run_simple("locale-gen", &[])?;
        Ok(())
    }

    fn write_timezone(&self) -> Result<()> {
        let target = format!("/usr/share/zoneinfo/{}", self.plan.timezone);
        if !Path::new(&target).exists() {
            bail!("Timezone not found: {}", self.plan.timezone);
        }
        if Path::new("/etc/localtime").exists() {
            fs::remove_file("/etc/localtime")
                .context("Failed to remove existing /etc/localtime")?;
        }
        std::os::unix::fs::symlink(target, "/etc/localtime").context("Failed to link timezone")?;
        run_simple("hwclock", &["--systohc"])?;
        Ok(())
    }
}

fn run_simple(cmd: &str, args: &[&str]) -> Result<()> {
    let output = Command::new(cmd)
        .args(args)
        .output()
        .with_context(|| format!("Failed to run {}", cmd))?;
    if output.status.success() {
        return Ok(());
    }

    let stderr = sanitize_for_log(&String::from_utf8_lossy(&output.stderr));
    if stderr.is_empty() {
        bail!("Command failed: {}", cmd);
    }
    bail!("Command failed: {}: {}", cmd, stderr)
}

fn run_with_input(cmd: &str, args: &[&str], input: &str) -> Result<()> {
    let mut child = Command::new(cmd)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("Failed to run {}", cmd))?;
    if let Some(mut stdin) = child.stdin.take() {
        use std::io::Write;
        stdin.write_all(input.as_bytes())?;
    }
    let output = child.wait_with_output()?;
    if output.status.success() {
        return Ok(());
    }

    let stderr = sanitize_for_log(&String::from_utf8_lossy(&output.stderr));
    if stderr.is_empty() {
        bail!("Command failed: {}", cmd);
    }
    bail!("Command failed: {}: {}", cmd, stderr)
}

fn user_exists(username: &str) -> Result<bool> {
    let status = Command::new("id")
        .arg("-u")
        .arg(username)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .context("Failed to query existing user")?;
    Ok(status.success())
}

fn copy_dir_contents(src: &Path, dst: &Path) -> Result<()> {
    if !src.exists() {
        bail!("Required path missing: {}", src.display());
    }
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        copy_path(&entry.path(), &dst.join(entry.file_name()))?;
    }
    Ok(())
}

fn copy_path(src: &Path, dst: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(src)?;
    if metadata.is_dir() {
        fs::create_dir_all(dst)?;
        for entry in fs::read_dir(src)? {
            let entry = entry?;
            copy_path(&entry.path(), &dst.join(entry.file_name()))?;
        }
        return Ok(());
    }
    if metadata.file_type().is_symlink() {
        if dst.exists() {
            let _ = fs::remove_file(dst);
        }
        let target = fs::read_link(src)?;
        std::os::unix::fs::symlink(target, dst)?;
        return Ok(());
    }
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(src, dst)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{sanitize_for_log, Checkpoint, InstallPlan, StageId};

    #[test]
    fn install_plan_validation_rejects_missing_fields() {
        let plan = InstallPlan {
            disk: String::new(),
            hostname: "host".into(),
            username: "user".into(),
            password: "pass".into(),
            keymap: "us".into(),
            timezone: "UTC".into(),
            git_name: String::new(),
            git_email: String::new(),
            desktop_profile: "slate".into(),
        };

        assert!(plan.validate().is_err());
    }

    #[test]
    fn sanitize_for_log_strips_escape_sequences() {
        assert_eq!(sanitize_for_log("\u{1b}[31merror\u{1b}[0m"), "error");
    }

    #[test]
    fn checkpoint_round_trip_stage_ids() {
        let checkpoint = Checkpoint {
            active_stage: Some(StageId::Bootstrap),
            completed_stages: vec![StageId::Collect, StageId::PrepareDisk],
            last_error: None,
        };

        let serialized = serde_json::to_string(&checkpoint).unwrap();
        let decoded: Checkpoint = serde_json::from_str(&serialized).unwrap();
        assert_eq!(decoded.active_stage, Some(StageId::Bootstrap));
        assert_eq!(decoded.completed_stages.len(), 2);
    }
}
