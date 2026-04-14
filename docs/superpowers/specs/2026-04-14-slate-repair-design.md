# Slate Repair Design

## Goal

Add a CLI-only `slate repair` command that repairs an already installed Slate system in place. The command should target the current non-root user by default, inspect the system, build a grouped repair plan, prompt before applying each group, and avoid silent overwrites of Slate-managed files.

## Scope

In scope:
- CLI-only repair flow
- Default target is the current non-root user
- Grouped prompts before applying repairs
- Repair of packages, shell files, user config, services, and bootloader state
- Idempotent package and service repair
- Explicit confirmation before overwriting managed config groups

Out of scope:
- TUI for repair
- Disk partitioning or reinstall behavior
- Full rollback/transaction support
- Arbitrary user targeting in the initial design

## Chosen Approach

Implement a single `slate repair` command with internal grouped inspections and prompts.

Why:
- Matches the requested UX
- Keeps CLI surface area small
- Allows safe, bounded confirmation points
- Avoids forcing the user to know which subsystem is broken before repairing

Alternatives considered:
- Many repair subcommands per area: more explicit, but more command complexity
- Report-only doctor mode: safer, but does not solve the user’s immediate repair need

## User Model

`slate repair` should determine the target user as follows:
- if `SUDO_USER` is present and not `root`, use that
- otherwise use `USER`
- if the resolved user is `root` or empty, fail and ask the operator to run as a normal user via `sudo`

The command may require root for some actions, but the repair target is the current real user, not root.

## Flow

1. Resolve target user
2. Inspect current system state
3. Build grouped repair actions
4. Print each group and planned changes
5. Prompt `Apply this group? [y/N]`
6. Apply approved groups in order
7. Print final summary:
   - applied groups
   - skipped groups
   - failed groups

## Repair Groups

### Packages

Responsibilities:
- ensure `ax` exists and is executable
- ensure Slate desktop packages are installed
- ensure requirement-derived shell packages are installed

Implementation notes:
- use idempotent package installs
- fetch `ax` if missing
- show package count and notable missing items before prompting

### Shell

Responsibilities:
- fetch shell source
- compare and repair Slate-managed shell assets under `.config` and `.local`
- apply Hyprland keymap override

Safety:
- this group must explicitly mention that managed shell files may be overwritten
- prompt before applying

### User

Responsibilities:
- ensure user exists and has wheel membership
- ensure `/etc/sudoers` includes `sudoers.d`
- ensure a per-user sudoers file exists with correct permissions
- ensure `.zprofile` and `.zshrc` are present
- ensure ownership of repaired user paths is correct
- optionally repair `.gitconfig` shape if Slate-managed Git identity is expected

### System

Responsibilities:
- ensure key services are enabled
- ensure locale/timezone/keymap-related config is sane where possible
- ensure pacman keyring is initialized/populated if needed for repair actions

### Boot

Responsibilities:
- ensure `systemd-boot` is installed
- ensure loader config exists
- ensure Slate boot entry exists and references the installed root UUID

Safety:
- print that bootloader files may be rewritten before prompting

## Detection Model

Each group should inspect first and only offer repair if there is something actionable.

Examples:
- `packages`: missing commands or packages
- `shell`: missing shell source deployment, missing key config directories, mismatched managed files
- `user`: missing wheel group membership, missing sudoers include, wrong file permissions
- `system`: disabled required services
- `boot`: missing loader entry or missing loader config

If a group is already healthy, report that and do not prompt.

## Confirmation Model

Prompt once per group:
- `packages`
- `shell`
- `user`
- `system`
- `boot`

Prompt style:
- short summary of detected issues
- statement if managed files may be overwritten
- `Apply this group? [y/N]`

Default should be `No`.

## Implementation Notes

Reuse as much of the staged installer backend as practical:
- package planning
- shell source fetching
- shell overrides
- boot entry generation helpers
- command execution helpers

Avoid sharing the TUI layer. Repair should remain plain CLI.

## Acceptance Criteria

The feature is successful when:
- `slate repair` runs without the TUI
- it targets the current non-root user by default
- it prompts once per repair group
- it can repair missing `ax`, sudo setup, shell deployment, service enables, and bootloader files
- it avoids silent overwrite of managed file groups

## Risks

- Determining whether a user-modified shell file is safe to overwrite may remain imperfect without a stronger managed-file marker system
- Bootloader repair must be careful to avoid rewriting unrelated custom entries beyond Slate-managed files
- Repairing packages on a broken system still depends on network and keyring health

