use crate::system;
use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::io::{self, BufRead, BufReader, Write};
use std::os::unix::fs::MetadataExt;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::Sender;
use std::thread;
use std::time::{Duration, Instant};

pub const TARGET_ROOT: &str = "/mnt";
pub const TARGET_PLAN_PATH: &str = "/mnt/etc/slate/install-plan.json";
const TARGET_CHECKPOINT_PATH: &str = "/mnt/etc/slate/checkpoint.json";
const HOST_PLAN_PATH: &str = "/tmp/slate-install-plan.json";
const SHELL_ARCHIVE_URL: &str =
    "https://github.com/manpreet113/shell/archive/refs/heads/main.tar.gz";
const SHELL_REPO_DIR: &str = "/tmp/slate-shell";
const AX_BINARY_URL: &str = "https://github.com/manpreet113/ax/releases/latest/download/ax";
const TEMP_AX_SUDOERS_FILE: &str = "/etc/sudoers.d/10-slate-ax";

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

pub fn repair() -> Result<()> {
    if !nix::unistd::Uid::effective().is_root() {
        bail!("`slate repair` must be run as root, preferably via sudo");
    }

    let target = RepairTarget::resolve()?;
    let mut ctx = RepairContext::new(target)?;
    ctx.run()
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
    mounts: MountTable,
}

impl InstallContext {
    fn new(plan: InstallPlan, sink: EventSink) -> Self {
        Self {
            plan,
            sink,
            checkpoint: Checkpoint::default(),
            current_stage: None,
            mounts: MountTable::default(),
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
            "curl",
            "tar",
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
        self.mounts.mount(
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

        self.mounts.unmount(&runner, TARGET_ROOT)?;
        self.mounts.mount(
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

        self.mounts.mount(
            &runner,
            &root,
            "/mnt/home",
            &["-o", "rw,noatime,compress=zstd,space_cache=v2,subvol=@home"],
        )?;
        self.mounts.mount(
            &runner,
            &root,
            "/mnt/var/log",
            &["-o", "rw,noatime,compress=zstd,space_cache=v2,subvol=@log"],
        )?;
        self.mounts.mount(
            &runner,
            &root,
            "/mnt/var/cache/pacman/pkg",
            &["-o", "rw,noatime,compress=zstd,space_cache=v2,subvol=@pkg"],
        )?;
        self.mounts.mount(
            &runner,
            &root,
            "/mnt/.snapshots",
            &[
                "-o",
                "rw,noatime,compress=zstd,space_cache=v2,subvol=@snapshots",
            ],
        )?;
        self.mounts.mount(&runner, &efi, "/mnt/boot", &[])?;
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
            "curl",
            "zsh",
            "intel-ucode",
            "amd-ucode",
            "libgit2",
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
        runner.run(
            "curl",
            &["-L", "--fail", AX_BINARY_URL, "-o", "/mnt/usr/local/bin/ax"],
            Some(Duration::from_secs(120)),
            false,
        )?;
        runner.run(
            "chmod",
            &["+x", "/mnt/usr/local/bin/ax"],
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
        self.mounts.targets.clear();
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
        self.ensure_pacman_keyring()?;
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

    fn ensure_pacman_keyring(&self) -> Result<()> {
        run_simple("pacman-key", &["--init"])?;
        run_simple("pacman-key", &["--populate", "archlinux"])?;
        run_simple("pacman", &["-Sy", "--noconfirm", "archlinux-keyring"])?;
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
        let mut updated = if content.contains("%wheel ALL=(ALL:ALL) ALL") {
            content
        } else {
            content.replace("# %wheel ALL=(ALL:ALL) ALL", "%wheel ALL=(ALL:ALL) ALL")
        };
        if !updated.contains("@includedir /etc/sudoers.d")
            && !updated.contains("#includedir /etc/sudoers.d")
        {
            updated.push_str("\n@includedir /etc/sudoers.d\n");
        } else {
            updated = updated.replace("#includedir /etc/sudoers.d", "@includedir /etc/sudoers.d");
        }
        fs::write(sudoers, updated)?;
        fs::create_dir_all("/etc/sudoers.d")?;
        fs::write(
            format!("/etc/sudoers.d/10-{}", self.plan.username),
            format!("{} ALL=(ALL:ALL) ALL\n", self.plan.username),
        )?;
        fs::set_permissions(
            format!("/etc/sudoers.d/10-{}", self.plan.username),
            fs::Permissions::from_mode(0o440),
        )?;
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
        self.ensure_shell_source()?;
        let requirements =
            parse_requirements_file(&Path::new(SHELL_REPO_DIR).join("requirements.txt"))
                .context("Failed to parse shell requirements")?;
        let packages = merged_package_plan(&requirements);
        install_packages_with_ax(&self.plan.username, &self.target_home(), &packages)?;
        Ok(())
    }

    fn desktop_assets(&self) -> Result<()> {
        self.ensure_shell_source()?;

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

        apply_shell_overrides(&self.plan, &user_home)?;

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

    fn ensure_shell_source(&self) -> Result<()> {
        if Path::new(SHELL_REPO_DIR).exists() {
            return Ok(());
        }
        fetch_repo_archive(SHELL_ARCHIVE_URL, Path::new(SHELL_REPO_DIR))
    }

    fn desktop_finalize(&self) -> Result<()> {
        let user_home = self.target_home();
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

    fn target_home(&self) -> PathBuf {
        PathBuf::from(format!("/home/{}", self.plan.username))
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

struct RepairTarget {
    username: String,
    home: PathBuf,
    hostname: String,
    keymap: String,
    timezone: String,
    git_name: String,
    git_email: String,
}

impl RepairTarget {
    fn resolve() -> Result<Self> {
        let username = std::env::var("SUDO_USER")
            .ok()
            .filter(|value| !value.trim().is_empty() && value != "root")
            .or_else(|| {
                std::env::var("USER")
                    .ok()
                    .filter(|value| !value.trim().is_empty() && value != "root")
            })
            .ok_or_else(|| anyhow!("Unable to resolve current non-root user"))?;

        let user = nix::unistd::User::from_name(&username)
            .context("Failed to query target user")?
            .ok_or_else(|| anyhow!("Target user does not exist: {}", username))?;

        let home = user.dir;
        let hostname = fs::read_to_string("/etc/hostname")
            .unwrap_or_else(|_| "slate".to_string())
            .trim()
            .to_string();
        let keymap = detect_keymap().unwrap_or_else(|| "us".to_string());
        let timezone = detect_timezone().unwrap_or_else(|| "UTC".to_string());
        let (git_name, git_email) = detect_git_identity(&home).unwrap_or_default();

        Ok(Self {
            username,
            home,
            hostname,
            keymap,
            timezone,
            git_name,
            git_email,
        })
    }

    fn install_plan(&self) -> InstallPlan {
        InstallPlan {
            disk: String::new(),
            hostname: self.hostname.clone(),
            username: self.username.clone(),
            password: String::new(),
            keymap: self.keymap.clone(),
            timezone: self.timezone.clone(),
            git_name: self.git_name.clone(),
            git_email: self.git_email.clone(),
            desktop_profile: "Slate".to_string(),
        }
    }
}

struct RepairContext {
    target: RepairTarget,
    applied: Vec<&'static str>,
    skipped: Vec<&'static str>,
    failed: Vec<String>,
}

impl RepairContext {
    fn new(target: RepairTarget) -> Result<Self> {
        Ok(Self {
            target,
            applied: Vec::new(),
            skipped: Vec::new(),
            failed: Vec::new(),
        })
    }

    fn run(&mut self) -> Result<()> {
        println!("Slate repair");
        println!("Target user: {}", self.target.username);
        println!("Home: {}", self.target.home.display());
        println!();

        let packages = self.inspect_packages()?;
        self.run_group("packages", packages, Self::apply_packages);
        let shell = self.inspect_shell()?;
        self.run_group("shell", shell, Self::apply_shell);
        let user = self.inspect_user()?;
        self.run_group("user", user, Self::apply_user);
        let system = self.inspect_system()?;
        self.run_group("system", system, Self::apply_system);
        let boot = self.inspect_boot()?;
        self.run_group("boot", boot, Self::apply_boot);

        println!();
        println!("Repair summary");
        println!(
            "Applied: {}",
            if self.applied.is_empty() {
                "none".to_string()
            } else {
                self.applied.join(", ")
            }
        );
        println!(
            "Skipped: {}",
            if self.skipped.is_empty() {
                "none".to_string()
            } else {
                self.skipped.join(", ")
            }
        );
        println!(
            "Failed: {}",
            if self.failed.is_empty() {
                "none".to_string()
            } else {
                self.failed.join(" | ")
            }
        );

        if self.failed.is_empty() {
            Ok(())
        } else {
            bail!("One or more repair groups failed")
        }
    }

    fn run_group(
        &mut self,
        name: &'static str,
        issues: Vec<String>,
        apply: fn(&mut Self) -> Result<()>,
    ) {
        if issues.is_empty() {
            println!("[ok] {}: no repair needed", name);
            return;
        }

        println!("[plan] {}", name);
        for issue in &issues {
            println!("  - {}", issue);
        }
        if prompt_yes_no("Apply this group? [y/N] ").unwrap_or(false) {
            match apply(self) {
                Ok(()) => {
                    println!("[done] {}", name);
                    self.applied.push(name);
                }
                Err(err) => {
                    println!("[fail] {}: {}", name, err);
                    self.failed.push(format!("{}: {}", name, err));
                }
            }
        } else {
            println!("[skip] {}", name);
            self.skipped.push(name);
        }
        println!();
    }

    fn inspect_packages(&self) -> Result<Vec<String>> {
        let mut issues = Vec::new();
        self.ensure_shell_source()?;
        let requirements =
            parse_requirements_file(&Path::new(SHELL_REPO_DIR).join("requirements.txt"))
                .context("Failed to inspect shell requirements")?;
        let packages = merged_package_plan(&requirements);
        let missing = packages
            .iter()
            .filter(|pkg| !package_installed(pkg))
            .cloned()
            .collect::<Vec<_>>();

        if !Path::new("/usr/local/bin/ax").exists() {
            issues.push("Missing /usr/local/bin/ax".to_string());
        }
        if !missing.is_empty() {
            issues.push(format!(
                "{} desktop packages missing: {}",
                missing.len(),
                preview_list(&missing, 8)
            ));
        }
        Ok(issues)
    }

    fn inspect_shell(&self) -> Result<Vec<String>> {
        self.ensure_shell_source()?;
        let mut issues = Vec::new();
        let config_src = Path::new(SHELL_REPO_DIR).join(".config");
        let local_src = Path::new(SHELL_REPO_DIR).join(".local");
        let config_dst = self.target.home.join(".config");
        let local_dst = self.target.home.join(".local");

        let config_diff = count_tree_differences(&config_src, &config_dst)?;
        if config_diff > 0 {
            issues.push(format!(
                "Shell config differs from source in {} path(s) and may be overwritten",
                config_diff
            ));
        }
        if local_src.exists() {
            let local_diff = count_tree_differences(&local_src, &local_dst)?;
            if local_diff > 0 {
                issues.push(format!(
                    "Shell local assets differ from source in {} path(s) and may be overwritten",
                    local_diff
                ));
            }
        }

        let input_conf = self.target.home.join(".config/hypr/conf.d/input.conf");
        if !input_conf.exists() {
            issues.push("Missing Hyprland input config".to_string());
        } else {
            let content = fs::read_to_string(&input_conf).unwrap_or_default();
            let expected = format!("kb_layout    = {}", self.target.keymap);
            if !content.contains(&expected) {
                issues.push(format!(
                    "Hyprland keymap override is missing or not set to {}",
                    self.target.keymap
                ));
            }
        }
        Ok(issues)
    }

    fn inspect_user(&self) -> Result<Vec<String>> {
        let mut issues = Vec::new();
        if !user_in_group(&self.target.username, "wheel")? {
            issues.push("User is not in wheel group".to_string());
        }
        let sudoers = fs::read_to_string("/etc/sudoers").unwrap_or_default();
        if !sudoers.contains("@includedir /etc/sudoers.d") {
            issues.push("/etc/sudoers does not include /etc/sudoers.d".to_string());
        }
        let sudoers_file = PathBuf::from(format!("/etc/sudoers.d/10-{}", self.target.username));
        if !sudoers_file.exists() {
            issues.push(format!("Missing {}", sudoers_file.display()));
        }
        for file in [".zprofile", ".zshrc"] {
            let path = self.target.home.join(file);
            if !path.exists() {
                issues.push(format!("Missing {}", path.display()));
            } else if !owned_by_user(&path, &self.target.username)? {
                issues.push(format!("Wrong ownership on {}", path.display()));
            }
        }
        Ok(issues)
    }

    fn inspect_system(&self) -> Result<Vec<String>> {
        let mut issues = Vec::new();
        for service in ["NetworkManager", "systemd-timesyncd", "bluetooth"] {
            if !service_enabled(service) {
                issues.push(format!("Service {} is not enabled", service));
            }
        }
        let locale = fs::read_to_string("/etc/locale.conf").unwrap_or_default();
        if !locale.contains("LANG=en_US.UTF-8") {
            issues.push("Locale is not set to en_US.UTF-8".to_string());
        }
        let vconsole = fs::read_to_string("/etc/vconsole.conf").unwrap_or_default();
        if !vconsole.contains("KEYMAP=") {
            issues.push("KEYMAP is missing from /etc/vconsole.conf".to_string());
        }
        Ok(issues)
    }

    fn inspect_boot(&self) -> Result<Vec<String>> {
        let mut issues = Vec::new();
        if !Path::new("/boot/loader/loader.conf").exists() {
            issues.push("Missing /boot/loader/loader.conf".to_string());
        }
        if !Path::new("/boot/loader/entries/slate.conf").exists() {
            issues.push("Missing /boot/loader/entries/slate.conf".to_string());
        }
        Ok(issues)
    }

    fn apply_packages(&mut self) -> Result<()> {
        self.ensure_pacman_keyring()?;
        self.ensure_shell_source()?;
        let requirements =
            parse_requirements_file(&Path::new(SHELL_REPO_DIR).join("requirements.txt"))?;
        let packages = merged_package_plan(&requirements);
        fetch_ax_binary()?;
        install_packages_with_ax(&self.target.username, &self.target.home, &packages)?;
        Ok(())
    }

    fn apply_shell(&mut self) -> Result<()> {
        self.ensure_shell_source()?;
        let config_src = Path::new(SHELL_REPO_DIR).join(".config");
        let local_src = Path::new(SHELL_REPO_DIR).join(".local");
        let config_dst = self.target.home.join(".config");
        let local_dst = self.target.home.join(".local");

        fs::create_dir_all(&config_dst)?;
        copy_dir_contents(&config_src, &config_dst)?;
        if local_src.exists() {
            fs::create_dir_all(&local_dst)?;
            copy_dir_contents(&local_src, &local_dst)?;
        }
        apply_shell_overrides(&self.target.install_plan(), &self.target.home)?;
        run_simple(
            "chown",
            &[
                "-R",
                &format!("{}:{}", self.target.username, self.target.username),
                self.target.home.to_string_lossy().as_ref(),
            ],
        )?;
        Ok(())
    }

    fn apply_user(&mut self) -> Result<()> {
        if !user_in_group(&self.target.username, "wheel")? {
            run_simple("usermod", &["-aG", "wheel", &self.target.username])?;
        }

        let sudoers = "/etc/sudoers";
        let content = fs::read_to_string(sudoers).context("Failed to read sudoers")?;
        let mut updated = if content.contains("%wheel ALL=(ALL:ALL) ALL") {
            content
        } else {
            content.replace("# %wheel ALL=(ALL:ALL) ALL", "%wheel ALL=(ALL:ALL) ALL")
        };
        if !updated.contains("@includedir /etc/sudoers.d")
            && !updated.contains("#includedir /etc/sudoers.d")
        {
            updated.push_str("\n@includedir /etc/sudoers.d\n");
        } else {
            updated = updated.replace("#includedir /etc/sudoers.d", "@includedir /etc/sudoers.d");
        }
        fs::write(sudoers, updated)?;

        fs::create_dir_all("/etc/sudoers.d")?;
        let sudoers_file = format!("/etc/sudoers.d/10-{}", self.target.username);
        fs::write(
            &sudoers_file,
            format!("{} ALL=(ALL:ALL) ALL\n", self.target.username),
        )?;
        fs::set_permissions(&sudoers_file, fs::Permissions::from_mode(0o440))?;

        write_user_shell_files(&self.target.home)?;
        run_simple(
            "chown",
            &[
                "-R",
                &format!("{}:{}", self.target.username, self.target.username),
                self.target.home.to_string_lossy().as_ref(),
            ],
        )?;
        Ok(())
    }

    fn apply_system(&mut self) -> Result<()> {
        self.ensure_pacman_keyring()?;
        write_locale_static()?;
        write_timezone_static(&self.target.timezone)?;
        fs::write(
            "/etc/vconsole.conf",
            format!("KEYMAP={}\n", self.target.keymap),
        )?;
        run_simple("systemctl", &["enable", "NetworkManager"])?;
        run_simple("systemctl", &["enable", "systemd-timesyncd"])?;
        run_simple("systemctl", &["enable", "bluetooth"])?;
        Ok(())
    }

    fn apply_boot(&mut self) -> Result<()> {
        write_bootloader_files()?;
        Ok(())
    }

    fn ensure_shell_source(&self) -> Result<()> {
        if Path::new(SHELL_REPO_DIR).exists() {
            return Ok(());
        }
        fetch_repo_archive(SHELL_ARCHIVE_URL, Path::new(SHELL_REPO_DIR))
    }

    fn ensure_pacman_keyring(&self) -> Result<()> {
        run_simple("pacman-key", &["--init"])?;
        run_simple("pacman-key", &["--populate", "archlinux"])?;
        run_simple("pacman", &["-Sy", "--noconfirm", "archlinux-keyring"])?;
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

fn fetch_repo_archive(url: &str, target_dir: &Path) -> Result<()> {
    let archive_path = Path::new("/tmp/slate-shell.tar.gz");
    if archive_path.exists() {
        fs::remove_file(archive_path).context("Failed to remove stale shell archive")?;
    }
    if target_dir.exists() {
        fs::remove_dir_all(target_dir).with_context(|| {
            format!(
                "Failed to remove existing shell directory at {}",
                target_dir.display()
            )
        })?;
    }

    run_simple(
        "curl",
        &[
            "-L",
            "--fail",
            url,
            "-o",
            archive_path.to_string_lossy().as_ref(),
        ],
    )?;
    fs::create_dir_all(target_dir)
        .with_context(|| format!("Failed to create {}", target_dir.display()))?;
    run_simple(
        "tar",
        &[
            "-xzf",
            archive_path.to_string_lossy().as_ref(),
            "--strip-components=1",
            "-C",
            target_dir.to_string_lossy().as_ref(),
        ],
    )?;
    Ok(())
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

fn slate_shell_packages() -> Vec<&'static str> {
    vec![
        "base-devel",
        "git",
        "hyprland",
        "quickshell",
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
        "cpio",
        "cmake"
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

fn apply_shell_overrides(plan: &InstallPlan, user_home: &Path) -> Result<()> {
    let input_conf = user_home.join(".config/hypr/conf.d/input.conf");
    if input_conf.exists() {
        let content = fs::read_to_string(&input_conf)?;
        let updated = set_hypr_keymap(&content, &plan.keymap);
        fs::write(&input_conf, updated)?;
    }
    Ok(())
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

fn detect_keymap() -> Option<String> {
    let raw = fs::read_to_string("/etc/vconsole.conf").ok()?;
    raw.lines().find_map(|line| {
        line.strip_prefix("KEYMAP=")
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
    })
}

fn detect_timezone() -> Option<String> {
    let link = fs::read_link("/etc/localtime").ok()?;
    let full = if link.is_absolute() {
        link
    } else {
        Path::new("/etc").join(link)
    };
    full.strip_prefix("/usr/share/zoneinfo")
        .ok()
        .map(|value| value.to_string_lossy().trim_start_matches('/').to_string())
        .filter(|value| !value.is_empty())
}

fn detect_git_identity(home: &Path) -> Option<(String, String)> {
    let raw = fs::read_to_string(home.join(".gitconfig")).ok()?;
    let mut name = String::new();
    let mut email = String::new();
    for line in raw.lines() {
        let trimmed = line.trim();
        if let Some(value) = trimmed.strip_prefix("name =") {
            name = value.trim().to_string();
        } else if let Some(value) = trimmed.strip_prefix("email =") {
            email = value.trim().to_string();
        }
    }
    Some((name, email))
}

fn prompt_yes_no(prompt: &str) -> Result<bool> {
    print!("{}", prompt);
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    Ok(matches!(
        input.trim().to_ascii_lowercase().as_str(),
        "y" | "yes"
    ))
}

fn package_installed(name: &str) -> bool {
    Command::new("pacman")
        .args(["-Q", name])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn preview_list(items: &[String], max: usize) -> String {
    let mut preview = items.iter().take(max).cloned().collect::<Vec<_>>();
    if items.len() > max {
        preview.push("...".to_string());
    }
    preview.join(", ")
}

fn count_tree_differences(src: &Path, dst: &Path) -> Result<usize> {
    if !src.exists() {
        return Ok(0);
    }
    if !dst.exists() {
        return Ok(1);
    }

    let mut diff_count = 0usize;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        diff_count += compare_path(&src_path, &dst_path)?;
    }
    Ok(diff_count)
}

fn compare_path(src: &Path, dst: &Path) -> Result<usize> {
    let src_meta = fs::symlink_metadata(src)?;
    if !dst.exists() {
        return Ok(1);
    }
    let dst_meta = fs::symlink_metadata(dst)?;

    if src_meta.file_type().is_symlink() {
        return Ok((fs::read_link(src)? != fs::read_link(dst)?).into());
    }
    if src_meta.is_dir() {
        if !dst_meta.is_dir() {
            return Ok(1);
        }
        let mut total = 0usize;
        for entry in fs::read_dir(src)? {
            let entry = entry?;
            total += compare_path(&entry.path(), &dst.join(entry.file_name()))?;
        }
        return Ok(total);
    }
    if !dst_meta.is_file() {
        return Ok(1);
    }
    Ok((fs::read(src)? != fs::read(dst)?).into())
}

fn user_in_group(username: &str, group: &str) -> Result<bool> {
    let output = Command::new("id")
        .args(["-nG", username])
        .output()
        .with_context(|| format!("Failed to inspect groups for {}", username))?;
    if !output.status.success() {
        bail!("Failed to inspect groups for {}", username);
    }
    let groups = String::from_utf8_lossy(&output.stdout);
    Ok(groups.split_whitespace().any(|item| item == group))
}

fn owned_by_user(path: &Path, username: &str) -> Result<bool> {
    let user = nix::unistd::User::from_name(username)?
        .ok_or_else(|| anyhow!("User not found: {}", username))?;
    let metadata = fs::metadata(path)?;
    Ok(metadata.uid() == user.uid.as_raw())
}

fn service_enabled(service: &str) -> bool {
    Command::new("systemctl")
        .args(["is-enabled", service])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn fetch_ax_binary() -> Result<()> {
    run_simple(
        "curl",
        &["-L", "--fail", AX_BINARY_URL, "-o", "/usr/local/bin/ax"],
    )?;
    run_simple("chmod", &["+x", "/usr/local/bin/ax"])?;
    Ok(())
}

fn install_packages_with_ax(username: &str, user_home: &Path, packages: &[String]) -> Result<()> {
    if packages.is_empty() {
        return Ok(());
    }

    fetch_ax_binary()?;
    let sudoers_rule = format!("{} ALL=(ALL) NOPASSWD: ALL\n", username);
    fs::write(TEMP_AX_SUDOERS_FILE, sudoers_rule)?;
    fs::set_permissions(TEMP_AX_SUDOERS_FILE, fs::Permissions::from_mode(0o440))?;

    let mut args = vec!["-S", "--needed", "--noconfirm"];
    args.extend(packages.iter().map(String::as_str));

    let result = run_command_as_user(username, user_home, "ax", &args);
    let cleanup = fs::remove_file(TEMP_AX_SUDOERS_FILE);
    if let Err(err) = cleanup {
        bail!("Failed to remove temporary ax sudoers file: {}", err);
    }
    result
}

fn run_command_as_user(username: &str, user_home: &Path, cmd: &str, args: &[&str]) -> Result<()> {
    let status = Command::new("runuser")
        .arg("-u")
        .arg(username)
        .arg("--")
        .arg("env")
        .arg(format!("HOME={}", user_home.display()))
        .arg(cmd)
        .args(args)
        .status()
        .with_context(|| format!("Failed to run {} as {}", cmd, username))?;

    if status.success() {
        return Ok(());
    }

    bail!(
        "Command failed: {} as {} (exit {})",
        cmd,
        username,
        status.code().unwrap_or(-1)
    )
}

fn write_user_shell_files(home: &Path) -> Result<()> {
    fs::write(
        home.join(".zprofile"),
        "if [[ -z $DISPLAY ]] && [[ $(tty) = /dev/tty1 ]] && command -v Hyprland >/dev/null; then\n  exec Hyprland\nfi\n",
    )?;
    fs::write(
        home.join(".zshrc"),
        "alias ls='eza --icons'\nalias ll='eza -lha --icons'\nalias cat='bat'\nalias grep='rg'\neval \"$(starship init zsh)\"\neval \"$(zoxide init zsh)\"\nexport PATH=$PATH:$HOME/.local/bin\n",
    )?;
    Ok(())
}

fn write_locale_static() -> Result<()> {
    let locale_gen = "/etc/locale.gen";
    let content = fs::read_to_string(locale_gen).context("Failed to read locale.gen")?;
    let updated = content.replace("#en_US.UTF-8 UTF-8", "en_US.UTF-8 UTF-8");
    fs::write(locale_gen, updated)?;
    fs::write("/etc/locale.conf", "LANG=en_US.UTF-8\n")?;
    run_simple("locale-gen", &[])?;
    Ok(())
}

fn write_timezone_static(timezone: &str) -> Result<()> {
    let target = format!("/usr/share/zoneinfo/{}", timezone);
    if !Path::new(&target).exists() {
        bail!("Timezone not found: {}", timezone);
    }
    if Path::new("/etc/localtime").exists() {
        let _ = fs::remove_file("/etc/localtime");
    }
    std::os::unix::fs::symlink(&target, "/etc/localtime")?;
    run_simple("hwclock", &["--systohc"])?;
    Ok(())
}

fn write_bootloader_files() -> Result<()> {
    run_simple("bootctl", &["install"])?;
    let root_device = system::find_mount_source("/")?
        .ok_or_else(|| anyhow!("Failed to determine root mount source"))?;
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

#[cfg(test)]
mod tests {
    use super::{
        detect_timezone, normalize_package_name, parse_requirements, sanitize_for_log,
        set_hypr_keymap, Checkpoint, InstallPlan, StageId,
    };

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

    #[test]
    fn parses_requirements_and_normalizes_known_aliases() {
        let parsed = parse_requirements(
            r#"
            quickshell
            python3
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
        assert_eq!(normalize_package_name("pactl"), "libpulse");
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

    #[test]
    fn timezone_detection_handles_missing_link() {
        let _ = detect_timezone();
    }
}
