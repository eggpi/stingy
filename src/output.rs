use crate::database;
use anyhow::Result;
use std::io::Write;

pub mod chart;
pub mod format;
pub mod table;

pub enum OutputForTesting {
    Table((Vec<String>, Vec<Vec<String>>)),
    Chart(String),
}

pub trait Output<W>
where
    W: Write,
{
    fn new(writer: W, termwidth: Option<usize>) -> Self;
    fn render_debits(
        &mut self,
        rows: &[database::DebitsRow],
        show_transaction_id: bool,
    ) -> Result<OutputForTesting>;
    fn render_credits(
        &mut self,
        rows: &[database::CreditsRow],
        show_transaction_id: bool,
    ) -> Result<OutputForTesting>;
    fn render_by_time(
        &mut self,
        rows: &[database::ByTimeRow],
        show_balance: bool,
        aggregation: &database::TimeAggregation,
    ) -> Result<OutputForTesting>;
    fn render_by_tag(&mut self, rows: &[database::ByTagRow]) -> Result<OutputForTesting>;
}
