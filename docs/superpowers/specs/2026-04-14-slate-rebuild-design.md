# Slate Rebuild Design

## Goal

Rebuild Slate into a one-pass Arch Linux installer that wipes the selected disk, installs a bootable base system, provisions the full Slate desktop in the same run, and exposes a simpler, more readable TUI. The new design should favor deterministic execution and debuggable failures over maximum theoretical speed, while still keeping obvious performance wins such as parallel package downloads and minimal duplicate work.

## Scope

In scope:
- Full-disk wipe installs only
- UEFI systems only
- Internet-connected installs only
- Single guided installer flow
- Base Arch install plus Slate desktop provisioning in the same run
- Deterministic logging, stage checkpoints, and explicit failure reporting
- TUI redesign focused on legibility and operational clarity

Out of scope for this rebuild:
- Dual boot
- Manual partitioning
- Offline installs
- BIOS/legacy boot
- Highly customizable package profiles
- Fancy multi-screen wizard flows

## Chosen Approach

Slate will be rebuilt as a host-side orchestrator plus an explicit in-chroot stage runner.

Why this approach:
- It creates a hard boundary between disk/bootstrap work and post-bootstrap system configuration.
- It replaces ad hoc state passing with a serialized install plan.
- It makes failures attributable to a named stage with concrete context.
- It keeps the distribution model simple: one binary still owns the full install.

Alternatives considered:
- Single-process monolith: simpler at first, but harder to reason about when partial failures happen.
- Script-first rebuild: faster to write, but likely to regress back into brittle shell glue.

## Architecture

### Core Model

Before any destructive action, Slate builds an `InstallPlan` containing:
- selected disk
- hostname
- username
- password
- keymap
- timezone
- optional Git identity
- desktop profile metadata
- stage configuration and package bundle identifiers

The plan is validated once, then written to disk and reused across later stages. No later stage should reconstruct intent from UI state or temporary scratch files.

Expected persistence points:
- host-side temporary copy before bootstrap
- target-side copy inside the mounted system for chroot execution
- checkpoint/result files for debugging and recovery messaging

### Stage Pipeline

Execution is a strict sequence:

1. `collect`
   Validate environment, input, and required dependencies.

2. `prepare_disk`
   Wipe the selected disk, create GPT partitions, wait for stable device nodes, format EFI and Btrfs partitions, create subvolumes, and mount the final filesystem layout.

3. `bootstrap`
   Run `pacstrap` with a curated minimal base set, generate `fstab`, copy the Slate binary into the target, and persist the install plan/checkpoint state.

4. `stage_apply`
   Execute inside chroot. Configure locale, timezone, keymap, hostname, users, sudo, services, bootloader, desktop packages, and Slate shell assets.

5. `verify`
   Confirm expected boot entries, required binaries, enabled services, user files, and desktop provisioning outputs.

6. `finalize`
   Write final status, surface a concise summary in the TUI, and leave the system in a cleanly mounted or cleanly unmounted state depending on the exit path.

### Shared Backend

The host and chroot flows should share a small backend layer for:
- command execution
- structured logging
- file writes and managed config updates
- checkpoint serialization
- command context and timeout handling

This avoids duplicating fragile helper code on both sides of the chroot boundary.

## Reliability Model

### General Rules

- Fail early before disk changes.
- Fail closed on destructive steps.
- Attach stage and command context to every surfaced error.
- Keep external command execution behind one wrapper.
- Prefer idempotent writes and `--needed` installs where possible.
- Treat mirror optimization as best-effort, not required for correctness.

### Command Runner

All system commands should run through one command wrapper that supports:
- streamed stdout/stderr into installer logs
- captured stderr tail for failures
- timeout support
- explicit command rendering
- stage-aware error context

This wrapper is the single source of truth for process execution. Direct `Command::new(...)` calls should be avoided outside the backend layer unless there is a strong reason.

### Mount and Device Handling

Mount logic should be centralized:
- one mount tracker owns created mounts
- mounts are registered in order and cleaned up in reverse
- mount failures include source, target, and options

Device path handling should be normalized once:
- standard disks: `/dev/sda` -> partitions like `/dev/sda1`
- NVMe/MMC: `/dev/nvme0n1` -> partitions like `/dev/nvme0n1p1`

Partition readiness must be explicit. The installer should wait for device nodes after partitioning and fail with a clear timeout if the kernel does not expose them in time.

### Recovery Semantics

Recovery is stage-based, not fully transactional.

- `collect`: always rerunnable
- `prepare_disk`: not resumable; rerun from scratch
- `bootstrap`: safe to rerun after remounting target
- `stage_apply`: written to be mostly idempotent
- `verify`: always rerunnable

The user-facing failure summary should always report:
- failing stage
- failing command when applicable
- short error text
- last completed stage

## Chroot Stage Design

The current chroot stage is the main instability source and should be replaced with explicit sub-stages under `stage_apply`.

Planned sub-stages:
- `base_config`
- `user_config`
- `boot_config`
- `desktop_packages`
- `desktop_assets`
- `desktop_finalize`

### Base Config

Responsibilities:
- hostname
- locale
- timezone
- keymap
- pacman configuration updates needed inside target
- essential service enablement

### User Config

Responsibilities:
- create primary user
- set root and user passwords
- enable wheel sudo access
- write managed shell/user config owned by Slate

### Boot Config

Responsibilities:
- install and configure `systemd-boot`
- generate boot entry using the installed root filesystem UUID
- avoid guessing from host runtime state outside the target mount layout

### Desktop Packages

Responsibilities:
- install required desktop packages deterministically
- use `--needed`
- rely on a curated package plan with clear normalization rules

The installer may still use `ax` if it is reliable enough in practice, but the design should not require it as the only viable package path. If a direct `pacman` path is more deterministic for core packages, it should be preferred.

### Desktop Assets

Responsibilities:
- acquire or bundle Slate shell assets
- deploy them into the user home
- fix permissions
- apply managed overrides such as keymap-related updates

This stage should prefer deterministic replacement of managed paths over incremental mutation of unknown files.

### Desktop Finalize

Responsibilities:
- plugin setup
- autostart/session wiring
- final ownership checks
- post-install checks required for first boot

## TUI Redesign

### Goals

- readable at normal terminal sizes
- fewer states
- less decorative noise
- no fragmented one-field-per-screen wizard
- installation status visible at a glance

### Screens

#### Plan Screen

One compact form with:
- disk
- hostname
- username
- password
- keymap
- timezone
- optional Git name/email

Long option lists such as timezone and keymap can open an inline searchable selector, but the overall flow remains a single form, not a deep wizard tree.

#### Review Screen

A destructive confirmation view showing:
- selected disk to be wiped
- partition/layout summary
- created user
- installed desktop profile

#### Install Screen

Three stable regions:
- left: stage list with status markers
- right: live logs
- bottom: progress and current action

#### Result Screen

Success or failure summary with:
- final status
- last completed stage
- failing stage
- failing command if any

### Visual Direction

- dark neutral base
- one restrained accent color
- strong contrast for current selection and active stage
- simple borders and consistent spacing
- logs prioritized over ornament

## Implementation Notes

### Package Strategy

The package plan should be explicit and split into:
- base system packages
- desktop runtime packages
- Slate-specific supporting packages

Normalization rules for package aliases should live in one place and be covered by unit tests.

### Config Management

Slate-managed config writes should use deterministic helpers:
- replace known managed files entirely when appropriate
- perform targeted updates when preserving external content matters
- avoid blind string replacement without structural assumptions

### Logging

Two forms of output are needed:
- structured internal events for checkpoints/stage state
- user-facing text logs for the TUI

The TUI should render readable logs, but the backend should preserve enough structure for debugging and future resume support.

## Testing Strategy

Minimum testing expectations for the rebuild:
- unit tests for install plan validation
- unit tests for package normalization
- unit tests for config rendering helpers
- unit tests for partition path resolution
- unit tests for checkpoint serialization

Operational verification should include:
- `cargo test`
- `cargo clippy --all-targets --all-features -- -D warnings`
- at least one dry-run style validation path where feasible without touching disks

## Risks

- Desktop provisioning depends on upstream package and asset availability.
- Network quality can still affect install time and success, even with a reliable structure.
- A fully deterministic desktop setup may require reducing dependence on loosely controlled upstream scripts or repo state.

## Acceptance Criteria

The rebuild is successful when:
- the installer wipes a selected disk and completes a full Arch + Slate desktop install in one run
- chroot-stage work is decomposed into explicit sub-stages with context-rich failures
- the TUI is simplified to a readable form/review/install/result flow
- command execution and mount handling are centralized
- tests and clippy pass locally

