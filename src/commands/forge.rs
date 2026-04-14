use crate::system;
use crate::tui;
use anyhow::{bail, Context, Result};

pub fn forge() -> Result<()> {
    let devices = system::list_block_devices().context("Failed to list block devices")?;
    if devices.is_empty() {
        bail!("No installable block devices found");
    }

    tui::run_installer(devices)
}
