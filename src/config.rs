use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SlateConfig {
    pub palette: Palette,
    pub hardware: Hardware,
    pub apps: Vec<App>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Palette {
    #[serde(default = "default_palette_mode")]
    pub mode: String,                // "manual" or "matugen"
    pub bg_void: String,             // Darkest background
    pub bg_void_transparent: String, // Background with alpha
    #[serde(default = "default_bg_surface")]
    pub bg_surface: String,          // Card/input surface
    #[serde(default = "default_bg_overlay")]
    pub bg_overlay: String,          // Overlay/hover layer
    pub foreground: String,          // Primary text
    #[serde(default = "default_foreground_dim")]
    pub foreground_dim: String,      // Dimmed/inactive text
    pub accent: String,              // Primary accent
    #[serde(default = "default_accent_bright")]
    pub accent_bright: String,       // Bright accent (hover)
}

fn default_palette_mode() -> String {
    "manual".to_string()
}
fn default_bg_surface() -> String {
    "#14161c".to_string()
}
fn default_bg_overlay() -> String {
    "#1a1d26".to_string()
}
fn default_foreground_dim() -> String {
    "#555b6e".to_string()
}
fn default_accent_bright() -> String {
    "#7aa2cf".to_string()
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Hardware {
    pub monitor_scale: f32,
    pub root_uuid: String,
    #[serde(default = "default_font")]
    pub font_family: String,
    #[serde(default = "default_wallpaper")]
    pub wallpaper: String,
}

fn default_wallpaper() -> String {
    "~/Pictures/Wallpapers/mist-forest.png".to_string()
}

fn default_font() -> String {
    "Iosevka Nerd Font".to_string()
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct App {
    pub name: String,
    pub enabled: bool,
    pub template_path: String,   // e.g., "waybar/style.css"
    pub config_path: String,     // e.g., "waybar/style.css"
    pub reload_signal: ReloadSignal,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ReloadSignal {
    Hyprctl,
    Hyprpaper,
    Signal { signal: String },
    Makoctl,
    None,
}

impl SlateConfig {
    pub fn load(path: &std::path::Path) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: SlateConfig = toml::from_str(&content)?;
        Ok(config)
    }

    pub fn save(&self, path: &std::path::Path) -> anyhow::Result<()> {
        let content = toml::to_string_pretty(self)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, content)?;
        Ok(())
    }

    /// Returns the wallpaper path with ~ expanded to the absolute home path
    pub fn expanded_wallpaper(&self) -> String {
        shellexpand::tilde(&self.hardware.wallpaper).to_string()
    }
}
