use crate::system::BlockDevice;
use dialoguer::{theme::ColorfulTheme, Input, Password, Select};
use anyhow::Result;
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInfo {
    pub username: String,
    pub password: String,
    pub hostname: String,
}

pub fn select_disk(devices: &[BlockDevice]) -> Result<BlockDevice> {
    let items: Vec<String> = devices
        .iter()
        .map(|d| format!("{} - {} ({})", d.path, d.model, d.size))
        .collect();

    let selection = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("Select target disk for installation")
        .default(0)
        .items(&items[..])
        .interact()?;

    Ok(devices[selection].clone())
}

pub fn get_user_info() -> Result<UserInfo> {
    let hostname: String = Input::with_theme(&ColorfulTheme::default())
        .with_prompt("Enter hostname")
        .default("archlinux".into())
        .interact_text()?;

    let username: String = Input::with_theme(&ColorfulTheme::default())
        .with_prompt("Enter username")
        .interact_text()?;

    let password = Password::with_theme(&ColorfulTheme::default())
        .with_prompt("Enter user password")
        .with_confirmation("Confirm password", "Passwords do not match")
        .interact()?;

    Ok(UserInfo {
        username,
        password,
        hostname,
    })
}
