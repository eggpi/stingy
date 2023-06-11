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

pub enum NewOrExisting<ModelType> {
    New(ModelType),
    Existing(ModelType),
}

impl<ModelType> NewOrExisting<ModelType> {
    pub fn unwrap(self) -> ModelType {
        match self {
            NewOrExisting::New(m) => m,
            NewOrExisting::Existing(m) => m,
        }
    }
}

pub trait ModelOperations<ModelType> {
    fn get_all(&self) -> Result<Vec<ModelType>> {
        unimplemented!();
    }
    fn update(&self, _: &ModelType) -> Result<()> {
        unimplemented!();
    }
    fn insert_or_get(&self, _: ModelType) -> Result<NewOrExisting<ModelType>> {
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
    pub account: Option<String>,
    pub tags: Vec<String>,
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

#[derive(Default, Debug, FieldNamesAsArray)]
#[field_names_as_array(visibility = "pub")]
pub struct ByMonthRow {
    pub account_name: String,
    pub month: NaiveDate,
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

pub trait QueryOperations<RowType> {
    fn query(&self, filters: QueryFilters) -> Result<QueryResult<RowType>>;
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
    + QueryOperations<DebitsRow>
    + QueryOperations<CreditsRow>
    + QueryOperations<ByMonthRow>
    + QueryOperations<ByTagRow>
    + private::Reset
{
    fn count_transactions(&self) -> Result<usize>;
    fn count_matching_transactions(&self, tag_rule_id: &str) -> Result<usize>;
    fn perform_migrations(&self) -> Result<()>;
    fn insert_test_data(&self);
}
