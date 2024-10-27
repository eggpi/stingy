use crate::database;
use crate::PreparedQuery;
use anyhow::Result;
use chrono::NaiveDate;

pub struct QueryResult {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

trait ToRowFormat {
    fn to_row_format(&self) -> String;
}

impl ToRowFormat for String {
    fn to_row_format(&self) -> String {
        self.to_string()
    }
}

impl ToRowFormat for NaiveDate {
    fn to_row_format(&self) -> String {
        format!("{}", self.format("%Y/%m/%d"))
    }
}

impl ToRowFormat for i64 {
    fn to_row_format(&self) -> String {
        format!("{:3}", self)
    }
}

impl ToRowFormat for f64 {
    fn to_row_format(&self) -> String {
        format!("{:.02}", self)
    }
}

impl ToRowFormat for str {
    fn to_row_format(&self) -> String {
        self.to_string()
    }
}

impl ToRowFormat for Vec<String> {
    fn to_row_format(&self) -> String {
        self.join("\n")
    }
}

pub fn command_query(
    db: &Box<dyn database::StingyDatabase>,
    query: &PreparedQuery,
    tags: &Vec<String>,
    description_contains: Option<&str>,
    amount_min: Option<f64>,
    amount_max: Option<f64>,
    from: Option<NaiveDate>,
    to: Option<NaiveDate>,
    account: Option<&str>,
) -> Result<QueryResult> {
    let mut filters = database::QueryFilters {
        account: account.map(|a| a.to_string()),
        tags: tags.to_vec(),
        description_contains: description_contains.map(|dc| dc.to_string()),
        amount_min: amount_min,
        amount_max: amount_max,
        date_from: from,
        date_to: to,
        transaction_types: Vec::new(),
    };

    let (columns, rows) = match query {
        PreparedQuery::Debits {
            show_transaction_id,
        } => {
            let query_result = db.query(filters)?;
            let mut columns = vec![
                "Account".to_string(),
                "Tag(s)".to_string(),
                "Debit Amount ↑".to_string(),
                "Description".to_string(),
                "Date".to_string(),
                "Debit (cumulative) ↓".to_string(),
                "% (cumulative) ↓".to_string(),
            ];
            if *show_transaction_id {
                columns.insert(1, "ID".to_string());
            }
            let rows = query_result
                .rows
                .iter()
                .map(|r: &database::DebitsRow| {
                    let mut row = vec![
                        r.account_name.to_row_format(),
                        (&r.tags).to_row_format(),
                        r.debit_amount.to_row_format(),
                        r.description.to_row_format(),
                        r.posted_date.to_row_format(),
                        r.debit_cumulative.to_row_format(),
                        r.debit_pct_cumulative.to_row_format(),
                    ];
                    if *show_transaction_id {
                        row.insert(1, r.transaction_id.to_row_format());
                    }
                    row
                })
                .collect();
            (columns, rows)
        }
        PreparedQuery::Credits {
            show_transaction_id,
        } => {
            let query_result = db.query(filters)?;
            let mut columns = vec![
                "Account".to_string(),
                "Tag(s)".to_string(),
                "Credit Amount ↑".to_string(),
                "Description".to_string(),
                "Date".to_string(),
                "Credit (cumulative) ↓".to_string(),
                "% (cumulative) ↓".to_string(),
            ];
            if *show_transaction_id {
                columns.insert(1, "ID".to_string());
            }
            let rows = query_result
                .rows
                .iter()
                .map(|r: &database::CreditsRow| {
                    let mut row = vec![
                        r.account_name.to_row_format(),
                        (&r.tags).to_row_format(),
                        r.credit_amount.to_row_format(),
                        r.description.to_row_format(),
                        r.posted_date.to_row_format(),
                        r.credit_cumulative.to_row_format(),
                        r.credit_pct_cumulative.to_row_format(),
                    ];
                    if *show_transaction_id {
                        row.insert(1, r.transaction_id.to_row_format());
                    }
                    row
                })
                .collect();
            (columns, rows)
        }
        PreparedQuery::ByMonth => {
            // Balance only really makes sense for some types of filter.
            let show_balance = tags.len() == 0
                && description_contains.is_none()
                && amount_min.is_none()
                && amount_max.is_none();
            let query_result = db.query(filters)?;
            let mut columns = vec![
                "Account".to_string(),
                "Month ↑".to_string(),
                "Credit Amount".to_string(),
                "Debit Amount".to_string(),
                "Credit - Debit".to_string(),
                "Credit (cumulative) ↑".to_string(),
                "Debit (cumulative) ↑".to_string(),
            ];
            if show_balance {
                columns.insert(5, "Balance".to_string());
            }
            let rows = query_result
                .rows
                .iter()
                .map(|r: &database::ByMonthRow| {
                    let mut row = vec![
                        r.account_name.to_row_format(),
                        // FIXME can't use to_row_format() because we want YYYY/MM.
                        format!("{}", r.month.format("%Y/%m")),
                        r.credit_amount.to_row_format(),
                        r.debit_amount.to_row_format(),
                        r.credit_minus_debit.to_row_format(),
                        r.credit_cumulative.to_row_format(),
                        r.debit_cumulative.to_row_format(),
                    ];
                    if show_balance {
                        row.insert(5, r.balance.to_row_format());
                    }
                    row
                })
                .collect();
            (columns, rows)
        }
        PreparedQuery::ByTag { transaction_type } => {
            filters.transaction_types = match transaction_type {
                Some(crate::TransactionType::debit) => vec![
                    database::model::TransactionType::Debit,
                    database::model::TransactionType::DirectDebit,
                ],
                Some(crate::TransactionType::credit) => {
                    vec![database::model::TransactionType::Credit]
                }
                None => Vec::new(),
            };
            let query_result = db.query(filters)?;
            let columns = vec![
                "Tag".to_string(),
                "Debit Amount ↑".to_string(),
                "Debit Amount %".to_string(),
                "Credit Amount".to_string(),
                "Credit Amount %".to_string(),
            ];
            let rows = query_result
                .rows
                .iter()
                .map(|r: &database::ByTagRow| {
                    vec![
                        r.tag.to_row_format(),
                        r.tag_debit.to_row_format(),
                        r.tag_debit_pct.to_row_format(),
                        r.tag_credit.to_row_format(),
                        r.tag_credit_pct.to_row_format(),
                    ]
                })
                .collect();
            (columns, rows)
        }
    };
    Ok(QueryResult { columns, rows })
}

#[cfg(test)]
mod debits_tests {
    use super::*;
    use crate::database::open_stingy_testing_database;

    #[test]
    fn columns() {
        let db = open_stingy_testing_database();
        db.insert_test_data();

        let QueryResult { columns, .. } = command_query(
            &db,
            &PreparedQuery::Debits {
                show_transaction_id: false,
            },
            &vec![],
            None,
            None,
            None,
            None,
            None,
            Some("000000 - 00000000"),
        )
        .unwrap();
        assert_eq!(
            columns,
            vec![
                "Account",
                "Tag(s)",
                "Debit Amount ↑",
                "Description",
                "Date",
                "Debit (cumulative) ↓",
                "% (cumulative) ↓"
            ]
        );
    }

    #[test]
    fn with_transaction_id() {
        let db = open_stingy_testing_database();
        db.insert_test_data();

        let QueryResult { columns, rows } = command_query(
            &db,
            &PreparedQuery::Debits {
                show_transaction_id: true,
            },
            &vec![],
            None,
            None,
            None,
            None,
            None,
            Some("000000 - 00000000"),
        )
        .unwrap();
        assert_eq!(
            columns,
            vec![
                "Account",
                "ID",
                "Tag(s)",
                "Debit Amount ↑",
                "Description",
                "Date",
                "Debit (cumulative) ↓",
                "% (cumulative) ↓"
            ]
        );
        assert_eq!(rows[0][1], "  5"); // This is ordered by amount, not ID.
    }

    #[test]
    fn tags() {
        let db = open_stingy_testing_database();
        db.insert_test_data();
        crate::commands::tags::add_tag_rule(
            &db,
            "coffee",
            None,
            Some("coffee"),
            None,
            None,
            None,
            None,
            None,
        )
        .unwrap();
        crate::commands::tags::add_tag_rule(
            &db,
            "pub",
            None,
            Some("pub"),
            None,
            None,
            None,
            None,
            None,
        )
        .unwrap();
        let QueryResult { rows, .. } = command_query(
            &db,
            &PreparedQuery::Debits {
                show_transaction_id: false,
            },
            &vec!["coffee".to_string(), "pub".to_string()],
            None,
            None,
            None,
            None,
            None,
            Some("000000 - 00000000"),
        )
        .unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0][2], "16.00");
        assert_eq!(rows[1][2], "3.74");
        assert_eq!(rows[2][2], "2.99");
    }

    #[test]
    fn tag_prefix_match() {
        let db = open_stingy_testing_database();
        db.insert_test_data();
        crate::commands::tags::add_tag_rule(
            &db,
            "daily/coffee",
            None,
            Some("coffee"),
            None,
            None,
            None,
            None,
            None,
        )
        .unwrap();
        let QueryResult { rows, .. } = command_query(
            &db,
            &PreparedQuery::Debits {
                show_transaction_id: false,
            },
            &vec!["daily/".to_string()],
            None,
            None,
            None,
            None,
            None,
            Some("000000 - 00000000"),
        )
        .unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0][2], "3.74");
        assert_eq!(rows[0][1], "daily/coffee"); // Tag column
        assert_eq!(rows[1][2], "2.99");
        assert_eq!(rows[1][1], "daily/coffee");
    }

    #[test]
    fn description_contains() {
        let db = open_stingy_testing_database();
        db.insert_test_data();

        let QueryResult { rows, .. } = command_query(
            &db,
            &PreparedQuery::Debits {
                show_transaction_id: false,
            },
            &vec![],
            Some("CoFfEE"), // Should be case-insensitive.
            None,
            None,
            None,
            None,
            Some("000000 - 00000000"),
        )
        .unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0][2], "3.74");
        assert_eq!(rows[1][2], "2.99");
    }

    #[test]
    fn amount_min() {
        let db = open_stingy_testing_database();
        db.insert_test_data();

        let QueryResult { rows, .. } = command_query(
            &db,
            &PreparedQuery::Debits {
                show_transaction_id: false,
            },
            &vec![],
            None,
            Some(16.0),
            None,
            None,
            None,
            Some("000000 - 00000000"),
        )
        .unwrap();
        assert_eq!(rows.len(), 4);
        assert_eq!(rows[0][2], "35.98");
        assert_eq!(rows[1][2], "25.15");
        assert_eq!(rows[2][2], "22.50");
        assert_eq!(rows[3][2], "16.00");
    }

    #[test]
    fn amount_max() {
        let db = open_stingy_testing_database();
        db.insert_test_data();

        let QueryResult { rows, .. } = command_query(
            &db,
            &PreparedQuery::Debits {
                show_transaction_id: false,
            },
            &vec![],
            None,
            None,
            Some(16.0),
            None,
            None,
            Some("000000 - 00000000"),
        )
        .unwrap();
        assert_eq!(rows.len(), 5);
        assert_eq!(rows[0][2], "15.99");
        assert_eq!(rows[1][2], "10.00");
        assert_eq!(rows[2][2], "7.63");
        assert_eq!(rows[3][2], "3.74");
        assert_eq!(rows[4][2], "2.99");
    }

    #[test]
    fn from() {
        let db = open_stingy_testing_database();
        db.insert_test_data();

        let QueryResult { rows, .. } = command_query(
            &db,
            &PreparedQuery::Debits {
                show_transaction_id: false,
            },
            &vec![],
            None,
            None,
            None,
            Some(NaiveDate::from_ymd_opt(2021, 03, 02).unwrap()),
            None,
            Some("000000 - 00000000"),
        )
        .unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0][2], "25.15");
        assert_eq!(rows[1][2], "7.63");
    }

    #[test]
    fn to() {
        let db = open_stingy_testing_database();
        db.insert_test_data();

        let QueryResult { rows, .. } = command_query(
            &db,
            &PreparedQuery::Debits {
                show_transaction_id: false,
            },
            &vec![],
            None,
            None,
            None,
            None,
            Some(NaiveDate::from_ymd_opt(2021, 02, 26).unwrap()),
            Some("000000 - 00000000"),
        )
        .unwrap();
        assert_eq!(rows.len(), 4);
        assert_eq!(rows[0][2], "35.98");
        assert_eq!(rows[1][2], "22.50");
        assert_eq!(rows[2][2], "10.00");
        assert_eq!(rows[3][2], "3.74");
    }

    #[test]
    fn cumulative_debit() {
        let db = open_stingy_testing_database();
        db.insert_test_data();

        let QueryResult { rows, .. } = command_query(
            &db,
            &PreparedQuery::Debits {
                show_transaction_id: false,
            },
            &vec![],
            None,
            None,
            None,
            None,
            None,
            Some("000000 - 00000000"),
        )
        .unwrap();
        assert_eq!(rows.len(), 9);
        assert_eq!(rows[0][5], "35.98");
        assert_eq!(rows[1][5], "61.13");
        assert_eq!(rows[2][5], "83.63");
        assert_eq!(rows[3][5], "99.63");
        assert_eq!(rows[4][5], "115.62");
        assert_eq!(rows[5][5], "125.62");
        assert_eq!(rows[6][5], "133.25");
        assert_eq!(rows[7][5], "136.99");
        assert_eq!(rows[8][5], "139.98");
    }

    #[test]
    fn cumulative_percentage() {
        let db = open_stingy_testing_database();
        db.insert_test_data();

        let QueryResult { rows, .. } = command_query(
            &db,
            &PreparedQuery::Debits {
                show_transaction_id: false,
            },
            &vec![],
            None,
            None,
            None,
            None,
            None,
            Some("000000 - 00000000"),
        )
        .unwrap();
        assert_eq!(rows.len(), 9);
        assert_eq!(rows[0][6], "25.70");
        assert_eq!(rows[1][6], "43.67");
        assert_eq!(rows[2][6], "59.74");
        assert_eq!(rows[3][6], "71.17");
        assert_eq!(rows[4][6], "82.60");
        assert_eq!(rows[5][6], "89.74");
        assert_eq!(rows[6][6], "95.19");
        assert_eq!(rows[7][6], "97.86");
        assert_eq!(rows[8][6], "100.00");
    }

    #[test]
    fn date_format() {
        let db = open_stingy_testing_database();
        db.insert_test_data();

        let QueryResult { rows, .. } = command_query(
            &db,
            &PreparedQuery::Debits {
                show_transaction_id: false,
            },
            &vec![],
            None,
            None,
            None,
            None,
            None,
            Some("000000 - 00000000"),
        )
        .unwrap();
        assert_eq!(rows[0][4], "2021/02/26");
        // The first row is the one with the highest debit. The assertion
        // below is just for context, we're actually testing the date
        // format with the assertion above.
        assert_eq!(rows[0][3], "GROCERIES");
    }

    #[test]
    fn account() {
        let db = open_stingy_testing_database();
        db.insert_test_data();

        let QueryResult { rows, .. } = command_query(
            &db,
            &PreparedQuery::Debits {
                show_transaction_id: false,
            },
            &vec![],
            None,
            None,
            None,
            None,
            None,
            Some("000000 - 00000000"),
        )
        .unwrap();
        assert_eq!(rows[0][0], "000000 - 00000000");
    }
}

#[cfg(test)]
mod credits_tests {
    use super::*;
    use crate::database::open_stingy_testing_database;

    #[test]
    fn columns() {
        let db = open_stingy_testing_database();
        db.insert_test_data();

        let QueryResult { columns, .. } = command_query(
            &db,
            &PreparedQuery::Credits {
                show_transaction_id: false,
            },
            &vec![],
            None,
            None,
            None,
            None,
            None,
            Some("000000 - 00000000"),
        )
        .unwrap();
        assert_eq!(
            columns,
            vec![
                "Account",
                "Tag(s)",
                "Credit Amount ↑",
                "Description",
                "Date",
                "Credit (cumulative) ↓",
                "% (cumulative) ↓"
            ]
        );
    }

    #[test]
    fn with_transaction_id() {
        let db = open_stingy_testing_database();
        db.insert_test_data();

        let QueryResult { columns, rows } = command_query(
            &db,
            &PreparedQuery::Credits {
                show_transaction_id: true,
            },
            &vec![],
            None,
            None,
            None,
            None,
            None,
            Some("000000 - 00000000"),
        )
        .unwrap();
        assert_eq!(
            columns,
            vec![
                "Account",
                "ID",
                "Tag(s)",
                "Credit Amount ↑",
                "Description",
                "Date",
                "Credit (cumulative) ↓",
                "% (cumulative) ↓"
            ]
        );
        assert_eq!(rows[0][1], "  1");
    }

    #[test]
    fn date_format() {
        let db = open_stingy_testing_database();
        db.insert_test_data();

        let QueryResult { rows, .. } = command_query(
            &db,
            &PreparedQuery::Credits {
                show_transaction_id: false,
            },
            &vec![],
            None,
            None,
            None,
            None,
            None,
            Some("000000 - 00000000"),
        )
        .unwrap();
        assert_eq!(rows[0][4], "2021/02/25");
    }
}

#[cfg(test)]
mod by_month_tests {
    use super::*;
    use crate::database::open_stingy_testing_database;

    #[test]
    fn columns() {
        let db = open_stingy_testing_database();
        db.insert_test_data();
        let QueryResult { columns, .. } = command_query(
            &db,
            &PreparedQuery::ByMonth {},
            &vec![],
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .unwrap();
        assert_eq!(
            columns,
            vec![
                "Account",
                "Month ↑",
                "Credit Amount",
                "Debit Amount",
                "Credit - Debit",
                "Balance",
                "Credit (cumulative) ↑",
                "Debit (cumulative) ↑",
            ]
        );
    }

    #[test]
    fn hide_balance_when_filtering_by_tag() {
        let db = open_stingy_testing_database();
        db.insert_test_data();

        let QueryResult { columns, .. } = command_query(
            &db,
            &PreparedQuery::ByMonth {},
            &vec!["coffee".to_string()],
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .unwrap();
        assert_eq!(
            columns,
            vec![
                "Account",
                "Month ↑",
                "Credit Amount",
                "Debit Amount",
                "Credit - Debit",
                "Credit (cumulative) ↑",
                "Debit (cumulative) ↑",
            ]
        );
    }

    #[test]
    fn hide_balance_when_filtering_by_description() {
        let db = open_stingy_testing_database();
        db.insert_test_data();

        let QueryResult { columns, .. } = command_query(
            &db,
            &PreparedQuery::ByMonth {},
            &vec![],
            Some("coffee"),
            None,
            None,
            None,
            None,
            None,
        )
        .unwrap();
        assert_eq!(
            columns,
            vec![
                "Account",
                "Month ↑",
                "Credit Amount",
                "Debit Amount",
                "Credit - Debit",
                "Credit (cumulative) ↑",
                "Debit (cumulative) ↑",
            ]
        );
    }

    #[test]
    fn hide_balance_when_filtering_by_amount() {
        let db = open_stingy_testing_database();
        db.insert_test_data();

        // amount_min
        let QueryResult { columns, .. } = command_query(
            &db,
            &PreparedQuery::ByMonth {},
            &vec![],
            None,
            Some(0.0),
            None,
            None,
            None,
            None,
        )
        .unwrap();
        assert_eq!(
            columns,
            vec![
                "Account",
                "Month ↑",
                "Credit Amount",
                "Debit Amount",
                "Credit - Debit",
                "Credit (cumulative) ↑",
                "Debit (cumulative) ↑",
            ]
        );

        // amount_max
        let QueryResult { columns, .. } = command_query(
            &db,
            &PreparedQuery::ByMonth {},
            &vec![],
            None,
            None,
            Some(10.0),
            None,
            None,
            None,
        )
        .unwrap();
        assert_eq!(
            columns,
            vec![
                "Account",
                "Month ↑",
                "Credit Amount",
                "Debit Amount",
                "Credit - Debit",
                "Credit (cumulative) ↑",
                "Debit (cumulative) ↑",
            ]
        );
    }

    #[test]
    fn multiple_months() {
        let db = open_stingy_testing_database();
        db.insert_test_data();

        let QueryResult { rows, .. } = command_query(
            &db,
            &PreparedQuery::ByMonth {},
            &vec![],
            None,
            None,
            None,
            None,
            None,
            Some("000000 - 00000000"),
        )
        .unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(
            rows[0],
            vec![
                "000000 - 00000000",
                "2021/03",
                "0.00",
                "67.76",
                "-67.76",
                "9852.76",
                "1000.00",
                "139.98"
            ]
        );
        assert_eq!(
            rows[1],
            vec![
                "000000 - 00000000",
                "2021/02",
                "1000.00",
                "72.22",
                "927.78",
                "9927.52",
                "1000.00",
                "72.22"
            ]
        );
    }

    #[test]
    fn multiple_accounts() {
        let db = open_stingy_testing_database();
        db.insert_test_data();

        let QueryResult { rows, .. } = command_query(
            &db,
            &PreparedQuery::ByMonth {},
            &vec![],
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .unwrap();
        assert_eq!(rows.len(), 3);
        // First account, March 2021.
        assert_eq!(
            rows[0],
            vec![
                "000000 - 00000000",
                "2021/03",
                "0.00",
                "67.76",
                "-67.76",
                "9852.76",
                // The cumulative columns go across accounts.
                "1100.00",
                "139.98"
            ]
        );
        // Second account, March 2021.
        assert_eq!(
            rows[1],
            vec![
                "111111 - 11111111",
                "2021/03",
                "100.00",
                "0.00",
                "100.00",
                "100.00",
                "1100.00",
                "72.22"
            ]
        );
        // First account, Feb 2021.
        assert_eq!(
            rows[2],
            vec![
                "000000 - 00000000",
                "2021/02",
                "1000.00",
                "72.22",
                "927.78",
                "9927.52",
                "1000.00",
                "72.22"
            ]
        );
        // There are no other transactions in either account.
    }

    #[test]
    fn display_account_alias_where_available() {
        let db = open_stingy_testing_database();
        db.insert_test_data();
        crate::commands::accounts::alias(&db, "000000 - 00000000", "Alias").unwrap();

        let QueryResult { rows, .. } = command_query(
            &db,
            &PreparedQuery::ByMonth {},
            &vec![],
            None,
            None,
            None,
            None,
            None,
            Some("000000 - 00000000"),
        )
        .unwrap();
        assert_eq!(rows[0][0], "Alias");
    }

    #[test]
    fn tagged_by_multiple_rules() {
        let db = open_stingy_testing_database();
        db.insert_test_data();
        crate::commands::tags::add_tag_rule(
            &db,
            "going to the coffee shop",
            None,
            Some("cof"),
            None,
            None,
            None,
            None,
            None,
        )
        .unwrap();
        crate::commands::tags::add_tag_rule(
            &db,
            "ordering a coffee",
            None,
            Some("coffee"),
            None,
            None,
            None,
            None,
            None,
        )
        .unwrap();
        let QueryResult { rows, .. } = command_query(
            &db,
            &PreparedQuery::ByMonth {},
            &vec![],
            None,
            None,
            None,
            None,
            None,
            Some("000000 - 00000000"),
        )
        .unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[1][1], "2021/02");
        /* The coffee transaction is tagged twice, but aggregated only once. */
        assert_eq!(rows[1][3], "72.22");
    }
}

#[cfg(test)]
mod by_tag_tests {
    use super::*;
    use crate::database::open_stingy_testing_database;

    #[test]
    fn columns() {
        let db = open_stingy_testing_database();
        db.insert_test_data();
        let QueryResult { columns, .. } = command_query(
            &db,
            &PreparedQuery::ByTag {
                transaction_type: None,
            },
            &vec![],
            None,
            None,
            None,
            None,
            None,
            Some("000000 - 00000000"),
        )
        .unwrap();
        assert_eq!(
            columns,
            vec![
                "Tag",
                "Debit Amount ↑",
                "Debit Amount %",
                "Credit Amount",
                "Credit Amount %"
            ]
        );
    }

    #[test]
    fn simple_display_all_tags() {
        let db = open_stingy_testing_database();
        db.insert_test_data();
        crate::commands::tags::add_tag_rule(
            &db,
            "coffee",
            None,
            Some("coffee"),
            None,
            None,
            None,
            None,
            None,
        )
        .unwrap();
        crate::commands::tags::add_tag_rule(
            &db,
            "pub",
            None,
            Some("pub"),
            None,
            None,
            None,
            None,
            None,
        )
        .unwrap();
        let QueryResult { rows, .. } = command_query(
            &db,
            &PreparedQuery::ByTag {
                transaction_type: None,
            },
            &vec![],
            None,
            None,
            None,
            Some(NaiveDate::from_ymd_opt(2021, 02, 25).unwrap()),
            None,
            Some("000000 - 00000000"),
        )
        .unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0], vec!["", "117.25", "83.76", "1000.00", "100.00"]); // untagged
        assert_eq!(rows[1], vec!["pub", "16.00", "11.43", "0.00", "0.00"]);
        assert_eq!(rows[2], vec!["coffee", "6.73", "4.81", "0.00", "0.00"]);
    }

    #[test]
    fn multiple_accounts() {
        let db = open_stingy_testing_database();
        db.insert_test_data();
        crate::commands::tags::add_tag_rule(
            &db,
            "credit",
            None,
            None,
            Some(database::model::TransactionType::Credit),
            None,
            None,
            None,
            None,
        )
        .unwrap();
        let QueryResult { rows, .. } = command_query(
            &db,
            &PreparedQuery::ByTag {
                transaction_type: None,
            },
            &vec![],
            None,
            None,
            None,
            Some(NaiveDate::from_ymd_opt(2021, 02, 25).unwrap()),
            None,
            None,
        )
        .unwrap();
        assert_eq!(rows.len(), 2);
        // Tags are aggregated across accounts.
        assert_eq!(rows[1][0], "credit");
        assert_eq!(rows[1][3], "1100.00");
    }

    #[test]
    fn with_transaction_type() {
        let db = open_stingy_testing_database();
        db.insert_test_data();

        let QueryResult { rows, .. } = command_query(
            &db,
            &PreparedQuery::ByTag {
                transaction_type: Some(crate::TransactionType::debit),
            },
            &vec![],
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .unwrap();
        assert_eq!(rows[0], ["", "139.98", "100.00", "0.00", "0.00"]);
        //                                                   ^^^^^^
        // We want to check that there's no division by zero in the
        // % fields as no credit transactions are selected.
    }

    #[test]
    fn tagged_by_multiple_rules() {
        let db = open_stingy_testing_database();
        db.insert_test_data();
        crate::commands::tags::add_tag_rule(
            &db,
            "pub",
            None,
            Some("pub"),
            None,
            None,
            None,
            None,
            None,
        )
        .unwrap();
        crate::commands::tags::add_tag_rule(
            &db,
            "pub",
            None,
            Some("pu"),
            None,
            None,
            None,
            None,
            None,
        )
        .unwrap();
        let QueryResult { rows, .. } = command_query(
            &db,
            &PreparedQuery::ByTag {
                transaction_type: None,
            },
            &vec![],
            None,
            None,
            None,
            Some(NaiveDate::from_ymd_opt(2021, 02, 25).unwrap()),
            None,
            Some("000000 - 00000000"),
        )
        .unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[1], vec!["pub", "16.00", "11.43", "0.00", "0.00"]);
    }
}
