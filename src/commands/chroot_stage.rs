use crate::installer;
use anyhow::Result;

pub fn chroot_stage() -> Result<()> {
    installer::run_stage_apply()
}
