use anyhow::Result;
use chrono::NaiveDate;
use struct_field_names_as_array::FieldNamesAsArray;

pub mod model;
mod sqlite_impl;

use sqlite_impl::SQLiteStingyDatabase;

pub fn open_stingy_database() -> Result<Box<dyn StingyDatabase>> {
    // This could be configurable, but we just hardcode SQLite for now.
    SQLiteStingyDatabase::new()
}

pub fn reset_stingy_database(db: Box<dyn StingyDatabase>) -> Result<()> {
    db.reset()
}

#[cfg(test)]
pub fn open_stingy_testing_database() -> Box<dyn StingyDatabase> {
    SQLiteStingyDatabase::new_for_testing()
}

#[derive(Debug)]
pub enum NewOrExisting<ModelType> {
    New(ModelType),
    Existing,
}

#[derive(Debug, PartialEq)]
pub enum TimeAggregation {
    Week,
    Month,
}

pub trait ModelOperations<ModelType> {
    fn get_all(&self) -> Result<Vec<ModelType>> {
        unimplemented!();
    }
    fn update(&self, _: &ModelType) -> Result<()> {
        unimplemented!();
    }
    fn insert(&self, _: ModelType) -> Result<NewOrExisting<ModelType>> {
        unimplemented!();
    }
    fn delete(&self, _: ModelType) -> Result<usize> {
        unimplemented!();
    }
}

pub struct QueryResult<RowType> {
    pub rows: Vec<RowType>,
}

#[derive(Default)]
pub struct QueryFilters {
    pub accounts: Vec<String>,
    pub tags: Vec<String>,
    pub not_tags: Vec<String>,
    pub description_contains: Option<String>,
    pub amount_min: Option<f64>,
    pub amount_max: Option<f64>,
    pub date_from: Option<NaiveDate>,
    pub date_to: Option<NaiveDate>,
    pub transaction_types: Vec<model::TransactionType>,
}

#[derive(Default, Debug, FieldNamesAsArray)]
pub struct DebitsRow {
    pub account_name: String,
    pub transaction_id: i64,
    pub tags: Vec<String>,
    pub debit_amount: f64,
    pub description: String,
    pub posted_date: NaiveDate,
    pub debit_cumulative: f64,
    pub debit_pct_cumulative: f64,
}

#[derive(Default, Debug, FieldNamesAsArray)]
#[field_names_as_array(visibility = "pub")]
pub struct CreditsRow {
    pub account_name: String,
    pub transaction_id: i64,
    pub tags: Vec<String>,
    pub credit_amount: f64,
    pub description: String,
    pub posted_date: NaiveDate,
    pub credit_cumulative: f64,
    pub credit_pct_cumulative: f64,
}

#[derive(Default, Debug, Clone, FieldNamesAsArray)]
#[field_names_as_array(visibility = "pub")]
pub struct ByTimeRow {
    pub account_name: String,
    pub aggregation_window_end: NaiveDate,
    pub credit_amount: f64,
    pub debit_amount: f64,
    pub credit_minus_debit: f64,
    pub balance: f64,
    pub credit_cumulative: f64,
    pub debit_cumulative: f64,
}

#[derive(Default, Debug, FieldNamesAsArray)]
#[field_names_as_array(visibility = "pub")]
pub struct ByTagRow {
    pub tag: String,
    pub tag_debit: f64,
    pub tag_debit_pct: f64,
    pub tag_credit: f64,
    pub tag_credit_pct: f64,
}

pub trait QueryOperations {
    // FIXME These should be query_by_transaction?
    fn query_debits(&self, filters: QueryFilters) -> Result<QueryResult<DebitsRow>>;
    fn query_credits(&self, filters: QueryFilters) -> Result<QueryResult<CreditsRow>>;
    fn query_by_tag(&self, filters: QueryFilters) -> Result<QueryResult<ByTagRow>>;
    fn query_by_time(
        &self,
        filters: QueryFilters,
        aggregation: &TimeAggregation,
    ) -> Result<QueryResult<ByTimeRow>>;
}

pub trait UndoOperations {
    fn begin_undo_step(&self, undo_step_name: &str, max_undo_steps: usize) -> Result<()>;
    fn undo_last_step(&self) -> Result<()>;
    fn get_last_undo_step(&self) -> Result<String>;
}

mod private {
    pub trait Reset {
        fn reset(&self) -> anyhow::Result<()>;
    }
}

pub trait StingyDatabase:
    ModelOperations<model::Account>
    + ModelOperations<model::Transaction>
    + ModelOperations<model::TagRule>
    + QueryOperations
    + UndoOperations
    + private::Reset
{
    fn get_uri(&self) -> String;
    fn count_transactions(&self) -> Result<usize>;
    fn lookup_tag_rule(&self, model: &model::TagRule) -> Result<Option<i64>>;
    fn count_matching_transactions(&self, tag_rule_id: &str) -> Result<usize>;
    #[cfg(test)]
    fn insert_test_data(&self);
}
