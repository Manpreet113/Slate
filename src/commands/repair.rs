use crate::installer;
use anyhow::Result;

pub fn repair() -> Result<()> {
    installer::repair()
}
