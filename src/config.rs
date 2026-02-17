use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SlateConfig {
    pub palette: Palette,
    pub hardware: Hardware,
    pub apps: Vec<App>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Palette {
    pub bg_void: String,      // e.g., "#0b0c10"
    pub bg_void_transparent: String, // e.g., "#0b0c1099"
    pub foreground: String,   // e.g., "#aeb3c2"
    pub accent: String,       // e.g., "#ffffff"
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Hardware {
    pub monitor_scale: f32,
    pub root_uuid: String,
    #[serde(default = "default_font")]
    pub font_family: String,
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
}
