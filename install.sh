#!/usr/bin/env bash
set -e

# === Helpers ===
GREEN='\033[0;32m' BLUE='\033[0;34m' YELLOW='\033[1;33m' RED='\033[0;31m' NC='\033[0m'
log_info() { echo -e "${BLUE}ℹ  $1${NC}" >&2; }
log_success() { echo -e "${GREEN}✔  $1${NC}" >&2; }
log_warn() { echo -e "${YELLOW}⚠  $1${NC}" >&2; }
log_error() { echo -e "${RED}✖  $1${NC}" >&2; }

has_cmd() { command -v "$1" >/dev/null 2>&1; }
has_font() { fc-list 2>/dev/null | grep -qi "$1"; }

[[ "$EUID" -eq 0 ]] && {
  log_error "Do not run as root. Use sudo where needed."
  exit 1
}

# === Dependency Installation ===
install_dependencies() {
  log_info "Installing dependencies for Arch Linux..."

  if ! has_cmd git || ! has_cmd makepkg; then
    log_info "Installing git and base-devel..."
    sudo pacman -S --needed --noconfirm git base-devel
  fi

  AUR_HELPER=""
  if has_cmd yay; then
    AUR_HELPER="yay"
  elif has_cmd paru; then
    AUR_HELPER="paru"
  else
    log_info "Installing yay-bin..."
    local YAY_TMP="$(mktemp -d)"
    git clone "https://aur.archlinux.org/yay-bin.git" "$YAY_TMP"
    (cd "$YAY_TMP" && makepkg -si --noconfirm)
    rm -rf "$YAY_TMP"
    AUR_HELPER="yay"
  fi

  local PKGS=(
    kitty tmux fuzzel network-manager-applet blueman
    pipewire wireplumber pavucontrol easyeffects ffmpeg x264 playerctl
    qt6-base qt6-declarative qt6-wayland qt6-svg qt6-tools qt6-imageformats qt6-multimedia qt6-shadertools
    libwebp libavif syntax-highlighting breeze-icons hicolor-icon-theme
    brightnessctl ddcutil fontconfig grim slurp imagemagick jq sqlite upower
    wl-clipboard wlsunset wtype zbar glib2 python-pipx zenity inetutils power-profiles-daemon
    python libnotify quickshell-git
    ttf-roboto ttf-roboto-mono ttf-dejavu ttf-liberation noto-fonts noto-fonts-cjk noto-fonts-emoji
    ttf-nerd-fonts-symbols
    matugen gpu-screen-recorder wl-clip-persist mpvpaper gradia
    ttf-phosphor-icons ttf-league-gothic adw-gtk-theme
  )

  log_info "Installing dependencies with $AUR_HELPER..."
  $AUR_HELPER -S --needed --noconfirm "${PKGS[@]}"
}

install_phosphor_fonts() {
  if has_font "Phosphor"; then
    log_success "Phosphor Icons already installed."
    return
  fi

  log_info "Installing Phosphor Icons..."
  local VERSION="2.1.2"
  local TEMP_DIR="$(mktemp -d)"
  local FONT_DIR="$HOME/.local/share/fonts/phosphor"

  curl -sL "https://github.com/phosphor-icons/web/archive/refs/tags/v${VERSION}.zip" -o "$TEMP_DIR/phosphor.zip"
  unzip -q "$TEMP_DIR/phosphor.zip" -d "$TEMP_DIR"
  mkdir -p "$FONT_DIR"
  find "$TEMP_DIR" -name "*.ttf" -exec cp {} "$FONT_DIR/" \;
  rm -rf "$TEMP_DIR"
  fc-cache -f "$FONT_DIR"
  log_success "Phosphor Icons installed successfully."
}

# === Autostart Configuration ===
configure_autostart() {
  log_info "Configuring Elysium Shell to launch on startup..."
  
  local BIN_DIR="/usr/local/bin"
  local LAUNCHER="$BIN_DIR/elysium"
  local EXEC_PATH="$HOME/dev/Rust/slate/shell/cli.sh"
  
  # Ensure cli.sh is executable
  chmod +x "$EXEC_PATH" 2>/dev/null || true

  # 1. Create a global wrapper script
  sudo mkdir -p "$BIN_DIR"
  sudo tee "$LAUNCHER" >/dev/null <<-EOF
#!/usr/bin/env bash
export PATH="\$HOME/.local/bin:\$PATH"
export QML2_IMPORT_PATH="\$HOME/.local/lib/qml:\$QML2_IMPORT_PATH"
export QML_IMPORT_PATH="\$QML2_IMPORT_PATH"
exec "$EXEC_PATH" "\$@"
EOF
  sudo chmod +x "$LAUNCHER"
  log_success "Global launcher 'elysium' created."

  # 2. Create a Systemd User Service linked to the graphical session
  local SYSTEMD_DIR="$HOME/.config/systemd/user"
  local SERVICE_FILE="$SYSTEMD_DIR/elysium-shell.service"
  
  mkdir -p "$SYSTEMD_DIR"
  tee "$SERVICE_FILE" >/dev/null <<-EOF
[Unit]
Description=Elysium Shell (QuickShell)
PartOf=graphical-session.target
After=graphical-session.target

[Service]
ExecStart=$LAUNCHER
Restart=always
RestartSec=2
Environment="WAYLAND_DISPLAY=wayland-1"

[Install]
WantedBy=graphical-session.target
EOF

  systemctl --user daemon-reload
  systemctl --user enable elysium-shell.service
  
  log_success "Elysium Shell is now set to autostart upon login!"
}

# === Main ===
install_dependencies
install_phosphor_fonts
configure_autostart

log_success "Slate & Elysium Shell dependencies installed and configured!"

