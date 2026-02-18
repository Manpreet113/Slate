# Slate - Quick Start

## First Time Setup (Just One Command!)

```bash
cd ~/Slate
cargo build --release
./target/release/slate init
```

That's it! `slate init` will:
- Auto-detect your PARTUUID (with sudo prompt)
- Create `~/.config/slate/slate.toml` with detected values
- Copy all templates to `~/.config/slate/templates/`
- Generate your initial configs in `~/.config/`

## Using Slate

### Change Colors
```bash
slate set palette.accent "#5f87af"
```

### Change Font
```bash
slate set hardware.font_family "JetBrains Mono"
```

### Regenerate Configs
```bash
slate reload
```

### Check System Status
```bash
slate check --verbose
```

## Customizing

Edit your templates in `~/.config/slate/templates/`, then run `slate reload`.

Example: Change Waybar padding
```bash
nano ~/.config/slate/templates/waybar/style.css
slate reload
```

## What Happened to install.sh?

`install.sh` is still needed for:
- Installing packages (pacman + AUR)
- Setting up Plymouth theme
- Changing default shell

`slate` handles:
- Hardware detection
- Config generation from templates
- Live updates without manual editing
