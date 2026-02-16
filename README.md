# Slate

**Tool, not jailer.** A complete Arch Linux + Hyprland configuration manager with dynamic template rendering.

## Installation (Replaces install.sh!)

```bash
git clone https://github.com/manpreet113/slate.git ~/Slate
cd ~/Slate
cargo build --release
sudo ./target/release/slate install
```

`slate install` does everything:
- Updates system & installs all packages (pacman + AUR)
- Bootstraps yay if needed
- Patches your bootloader (Limine or systemd-boot) with detected PARTUUID
- Installs Plymouth theme & mkinitcpio config
- Sets zsh as default shell
- Auto-detects hardware and generates configs
- Sets up template-based config management

After reboot, you're done!

## Daily Usage

### Change Colors
```bash
slate set palette.accent "#5f87af"
slate set palette.bg_void "#0a0a0a"
```

Waybar, Ghostty, and other apps reload automatically.

### Change Hardware Settings
```bash
slate set hardware.monitor_scale 1.5
slate set hardware.font_family "JetBrains Mono"
```

### Regenerate All Configs
```bash
slate reload
```

### Edit Templates
```bash
nano ~/.config/slate/templates/waybar/style.css
slate reload
```

## Commands

- **`slate install`** - Full system setup (packages, bootloader, configs)
- **`slate init`** - Initialize config management only (if already have packages)
- **`slate reload`** - Regenerate all configs from templates
- **`slate set <key> <value>`** - Update config value and auto-reload
- **`slate check`** - Verify LUKS encryption and system requirements

## Architecture

Slate uses template-based rendering with Tera:

**State**: `~/.config/slate/slate.toml` (colors, hardware, app list)  
**Templates**: `~/.config/slate/templates/` (your editable configs with `{{ variables }}`)  
**Output**: `~/.config/` (generated configs - do not edit manually)

Change a value → Slate renders templates → Writes atomically → Signals apps to reload

## Philosophy

1. **Explicit over implicit** - Template mappings in TOML, not hardcoded
2. **Atomic all-or-nothing** - Render → write .tmp → rename all → signal all
3. **One-command setup** - `slate install` does everything
4. **Live updates** - `slate set palette.accent "#fff"` and watch it change

## What Happened to install.sh?

**Deprecated.** Use `slate install` instead.

The Rust manager does everything install.sh did, plus:
- Dynamic config generation from templates
- Live color/font updates without editing files
- Proper error handling and atomic operations
- No fragile symlinks

See [SLATE_MANAGER.md](./SLATE_MANAGER.md) for full documentation.