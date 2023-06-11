use crate::database::{reset_stingy_database, StingyDatabase};
use anyhow::Result;

pub fn command_reset(db: Box<dyn StingyDatabase>) -> Result<()> {
    reset_stingy_database(db)
}
