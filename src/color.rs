use serde::Serialize;
use std::num::ParseIntError;

#[derive(Debug, Clone, Serialize)]
pub struct Color {
    r: u8,
    g: u8,
    b: u8,
    a: u8,
}

#[derive(Debug)]
pub enum ColorError {
    InvalidFormat,
    ParseError(ParseIntError),
}

impl std::fmt::Display for ColorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ColorError::InvalidFormat => write!(f, "Invalid color format (expected #RRGGBB or #RRGGBBAA)"),
            ColorError::ParseError(e) => write!(f, "Failed to parse hex value: {}", e),
        }
    }
}

impl std::error::Error for ColorError {}

impl From<ParseIntError> for ColorError {
    fn from(e: ParseIntError) -> Self {
        ColorError::ParseError(e)
    }
}

impl Color {
    pub fn from_hex(hex: &str) -> Result<Self, ColorError> {
        let hex = hex.trim_start_matches('#');
        let (r, g, b, a) = match hex.len() {
            6 => {
                let r = u8::from_str_radix(&hex[0..2], 16)?;
                let g = u8::from_str_radix(&hex[2..4], 16)?;
                let b = u8::from_str_radix(&hex[4..6], 16)?;
                (r, g, b, 255)
            },
            8 => {
                let r = u8::from_str_radix(&hex[0..2], 16)?;
                let g = u8::from_str_radix(&hex[2..4], 16)?;
                let b = u8::from_str_radix(&hex[4..6], 16)?;
                let a = u8::from_str_radix(&hex[6..8], 16)?;
                (r, g, b, a)
            },
            _ => return Err(ColorError::InvalidFormat),
        };
        Ok(Self { r, g, b, a })
    }

    pub fn css_rgba(&self) -> String {
        format!("rgba({}, {}, {}, {:.2})", self.r, self.g, self.b, self.a as f32 / 255.0)
    }

    pub fn rofi(&self) -> String {
        format!("#{:02x}{:02x}{:02x}{:02x}", self.r, self.g, self.b, self.a)
    }

    pub fn plymouth(&self) -> String {
        format!("0x{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }

    pub fn hex(&self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }

    /// Hyprland format: rgba(rrggbbaa)
    pub fn hyprland(&self) -> String {
        format!("rgba({:02x}{:02x}{:02x}{:02x})", self.r, self.g, self.b, self.a)
    }
}
