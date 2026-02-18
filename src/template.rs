use crate::color::Color;
use crate::config::SlateConfig;
use std::collections::HashMap;
use tera::{Context, Tera, Value};

pub struct TemplateEngine {
    tera: Tera,
}

impl TemplateEngine {
    pub fn new(templates_dir: &str) -> anyhow::Result<Self> {
        let pattern = format!("{}/**/*", templates_dir);
        let mut tera = Tera::new(&pattern)?;

        // Register custom color filters
        tera.register_filter("css_rgba", Self::filter_css_rgba);
        tera.register_filter("rofi", Self::filter_rofi);
        tera.register_filter("plymouth", Self::filter_plymouth);
        tera.register_filter("hex", Self::filter_hex);
        tera.register_filter("hyprland", Self::filter_hyprland);

        Ok(Self { tera })
    }

    pub fn render(&self, template_path: &str, config: &SlateConfig) -> anyhow::Result<String> {
        let mut context = Context::new();

        // Inject palette
        context.insert("palette", &config.palette);

        // Inject hardware
        context.insert("hardware", &config.hardware);

        let result = self.tera.render(template_path, &context)?;
        Ok(result)
    }

    fn filter_css_rgba(value: &Value, _: &HashMap<String, Value>) -> tera::Result<Value> {
        if let Some(hex) = value.as_str() {
            let color = Color::from_hex(hex)
                .map_err(|e| tera::Error::msg(format!("Invalid hex color: {}", e)))?;
            Ok(Value::String(color.css_rgba()))
        } else {
            Err(tera::Error::msg("css_rgba filter requires a string"))
        }
    }

    fn filter_rofi(value: &Value, _: &HashMap<String, Value>) -> tera::Result<Value> {
        if let Some(hex) = value.as_str() {
            let color = Color::from_hex(hex)
                .map_err(|e| tera::Error::msg(format!("Invalid hex color: {}", e)))?;
            Ok(Value::String(color.rofi()))
        } else {
            Err(tera::Error::msg("rofi filter requires a string"))
        }
    }

    fn filter_plymouth(value: &Value, _: &HashMap<String, Value>) -> tera::Result<Value> {
        if let Some(hex) = value.as_str() {
            let color = Color::from_hex(hex)
                .map_err(|e| tera::Error::msg(format!("Invalid hex color: {}", e)))?;
            Ok(Value::String(color.plymouth()))
        } else {
            Err(tera::Error::msg("plymouth filter requires a string"))
        }
    }

    fn filter_hex(value: &Value, _: &HashMap<String, Value>) -> tera::Result<Value> {
        if let Some(hex) = value.as_str() {
            let color = Color::from_hex(hex)
                .map_err(|e| tera::Error::msg(format!("Invalid hex color: {}", e)))?;
            Ok(Value::String(color.hex()))
        } else {
            Err(tera::Error::msg("hex filter requires a string"))
        }
    }

    fn filter_hyprland(value: &Value, _: &HashMap<String, Value>) -> tera::Result<Value> {
        if let Some(hex) = value.as_str() {
            let color = Color::from_hex(hex)
                .map_err(|e| tera::Error::msg(format!("Invalid hex color: {}", e)))?;
            Ok(Value::String(color.hyprland()))
        } else {
            Err(tera::Error::msg("hyprland filter requires a string"))
        }
    }
}
