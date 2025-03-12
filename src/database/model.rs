use chrono::NaiveDate;
// https://stackoverflow.com/questions/29986057#73375434
use struct_field_names_as_array::FieldNamesAsArray;

#[derive(Debug, Default, Clone, PartialEq)]
pub enum TransactionType {
    #[default]
    Debit,
    Credit,
    DirectDebit,
}

#[derive(Debug, Clone, FieldNamesAsArray)]
#[field_names_as_array(visibility = "pub")]
pub struct Account {
    pub id: Option<i64>,
    pub name: String,
    pub alias: Option<String>,
    pub selected: bool,
}

#[derive(Default, Debug, Clone, PartialEq, FieldNamesAsArray)]
#[field_names_as_array(visibility = "pub")]
pub struct Transaction {
    pub id: Option<i64>,
    pub account_name: String,
    pub posted_date: NaiveDate,
    pub description: String,
    pub debit_amount: f64,
    pub credit_amount: f64,
    pub balance: f64,
    pub transaction_type: TransactionType,
    pub currency: String,
}

#[derive(Default, Debug, Clone, FieldNamesAsArray)]
#[field_names_as_array(visibility = "pub")]
pub struct TagRule {
    pub id: Option<i64>,
    pub tag: String,
    pub human_readable: String,
    pub transaction_id: Option<i64>,
    pub description_contains: Option<String>,
    pub transaction_type: Option<TransactionType>,
    pub amount_min: Option<f64>,
    pub amount_max: Option<f64>,
    pub from_date: Option<NaiveDate>,
    pub to_date: Option<NaiveDate>,
}
