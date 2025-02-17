use crate::database;
use crate::database::model;
use crate::output::{chart, table, Output, OutputForTesting};
use crate::PreparedQuery;
use anyhow::Result;
use chrono::NaiveDate;
use std::io::Write;

pub fn command_query<W>(
    db: &Box<dyn database::StingyDatabase>,
    writer: &mut W,
    query: &PreparedQuery,
    tags: &Vec<String>,
    not_tags: &Vec<String>,
    description_contains: Option<&str>,
    amount_min: Option<f64>,
    amount_max: Option<f64>,
    from: Option<NaiveDate>,
    to: Option<NaiveDate>,
    accounts: Vec<&str>,
) -> Result<OutputForTesting>
where
    W: Write,
{
    let mut filters = database::QueryFilters {
        accounts: accounts.iter().map(|a| a.to_string()).collect(),
        tags: tags.to_vec(),
        not_tags: not_tags.to_vec(),
        description_contains: description_contains.map(|dc| dc.to_string()),
        amount_min: amount_min,
        amount_max: amount_max,
        date_from: from,
        date_to: to,
        transaction_types: Vec::new(),
    };

    match query {
        PreparedQuery::Debits {
            show_transaction_id,
        } => {
            let query_result = db.query(filters)?;
            let mut to = table::TableOutput::new(writer, None);
            to.render_debits(&query_result.rows, *show_transaction_id)
        }
        PreparedQuery::Credits {
            show_transaction_id,
        } => {
            let query_result = db.query(filters)?;
            let mut to = table::TableOutput::new(writer, None);
            to.render_credits(&query_result.rows, *show_transaction_id)
        }
        PreparedQuery::ByMonth { table } => {
            // Balance only really makes sense for some types of filter.
            let show_balance = tags.len() == 0
                && not_tags.len() == 0
                && description_contains.is_none()
                && amount_min.is_none()
                && amount_max.is_none();
            let query_result = db.query(filters)?;
            if *table {
                let mut to = table::TableOutput::new(writer, None);
                to.render_by_month(&query_result.rows, show_balance)
            } else {
                let mut co = chart::ChartOutput::new(writer, None);
                co.render_by_month(&query_result.rows, show_balance)
            }
        }
        PreparedQuery::ByTag {
            transaction_type,
            table,
        } => {
            filters.transaction_types = match transaction_type {
                Some(crate::TransactionType::debit) => vec![
                    model::TransactionType::Debit,
                    model::TransactionType::DirectDebit,
                ],
                Some(crate::TransactionType::credit) => {
                    vec![model::TransactionType::Credit]
                }
                None => Vec::new(),
            };
            let query_result = db.query(filters)?;
            if *table {
                let mut to = table::TableOutput::new(writer, None);
                to.render_by_tag(&query_result.rows)
            } else {
                let mut co = chart::ChartOutput::new(writer, None);
                co.render_by_tag(&query_result.rows)
            }
        }
    }
}

#[cfg(test)]
mod debits_tests {
    use super::*;
    use crate::database::open_stingy_testing_database;
    use std::io::Cursor;

    #[test]
    fn columns() {
        let db = open_stingy_testing_database();
        db.insert_test_data();

        let output_for_testing = command_query(
            &db,
            &mut Cursor::new(vec![]),
            &PreparedQuery::Debits {
                show_transaction_id: false,
            },
            &vec![],
            &vec![],
            None,
            None,
            None,
            None,
            None,
            vec!["000000 - 00000000"],
        )
        .unwrap();
        if let OutputForTesting::Table((columns, _)) = output_for_testing {
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
        } else {
            unimplemented!();
        }
    }

    #[test]
    fn with_transaction_id() {
        let db = open_stingy_testing_database();
        db.insert_test_data();

        let output_for_testing = command_query(
            &db,
            &mut Cursor::new(vec![]),
            &PreparedQuery::Debits {
                show_transaction_id: true,
            },
            &vec![],
            &vec![],
            None,
            None,
            None,
            None,
            None,
            vec!["000000 - 00000000"],
        )
        .unwrap();
        if let OutputForTesting::Table((columns, rows)) = output_for_testing {
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
        let output_for_testing = command_query(
            &db,
            &mut Cursor::new(vec![]),
            &PreparedQuery::Debits {
                show_transaction_id: false,
            },
            &vec!["coffee".to_string(), "pub".to_string()],
            &vec![],
            None,
            None,
            None,
            None,
            None,
            vec!["000000 - 00000000"],
        )
        .unwrap();
        if let OutputForTesting::Table((_, rows)) = output_for_testing {
            assert_eq!(rows.len(), 3);
            assert_eq!(rows[0][2], "16.00");
            assert_eq!(rows[1][2], "3.74");
            assert_eq!(rows[2][2], "2.99");
        } else {
            unimplemented!();
        }
    }

    #[test]
    fn tags_deduplicate() {
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
            "coffee",
            None,
            Some("cof"),
            None,
            None,
            None,
            None,
            None,
        )
        .unwrap();
        let output_for_testing = command_query(
            &db,
            &mut Cursor::new(vec![]),
            &PreparedQuery::Debits {
                show_transaction_id: false,
            },
            &vec![],
            &vec![],
            Some("coffee"),
            Some(3.00),
            None,
            None,
            None,
            vec!["000000 - 00000000"],
        )
        .unwrap();
        if let OutputForTesting::Table((_, rows)) = output_for_testing {
            assert_eq!(rows.len(), 1);
            // Two rules add the same tag 'coffee', we want it returned only once.
            assert_eq!(rows[0][1], "coffee");
        } else {
            unimplemented!();
        }
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
        let output_for_testing = command_query(
            &db,
            &mut Cursor::new(vec![]),
            &PreparedQuery::Debits {
                show_transaction_id: false,
            },
            &vec!["daily/".to_string()],
            &vec![],
            None,
            None,
            None,
            None,
            None,
            vec!["000000 - 00000000"],
        )
        .unwrap();
        if let OutputForTesting::Table((_, rows)) = output_for_testing {
            assert_eq!(rows.len(), 2);
            assert_eq!(rows[0][2], "3.74");
            assert_eq!(rows[0][1], "daily/coffee"); // Tag column
            assert_eq!(rows[1][2], "2.99");
            assert_eq!(rows[1][1], "daily/coffee");
        } else {
            unimplemented!()
        }
    }

    #[test]
    fn description_contains() {
        let db = open_stingy_testing_database();
        db.insert_test_data();

        let output_for_testing = command_query(
            &db,
            &mut Cursor::new(vec![]),
            &PreparedQuery::Debits {
                show_transaction_id: false,
            },
            &vec![],
            &vec![],
            Some("CoFfEE"), // Should be case-insensitive.
            None,
            None,
            None,
            None,
            vec!["000000 - 00000000"],
        )
        .unwrap();
        if let OutputForTesting::Table((_, rows)) = output_for_testing {
            assert_eq!(rows.len(), 2);
            assert_eq!(rows[0][2], "3.74");
            assert_eq!(rows[1][2], "2.99");
        } else {
            unimplemented!()
        }
    }

    #[test]
    fn amount_min() {
        let db = open_stingy_testing_database();
        db.insert_test_data();

        let output_for_testing = command_query(
            &db,
            &mut Cursor::new(vec![]),
            &PreparedQuery::Debits {
                show_transaction_id: false,
            },
            &vec![],
            &vec![],
            None,
            Some(16.0),
            None,
            None,
            None,
            vec!["000000 - 00000000"],
        )
        .unwrap();
        if let OutputForTesting::Table((_, rows)) = output_for_testing {
            assert_eq!(rows.len(), 4);
            assert_eq!(rows[0][2], "35.98");
            assert_eq!(rows[1][2], "25.15");
            assert_eq!(rows[2][2], "22.50");
            assert_eq!(rows[3][2], "16.00");
        } else {
            unimplemented!()
        }
    }

    #[test]
    fn amount_max() {
        let db = open_stingy_testing_database();
        db.insert_test_data();

        let output_for_testing = command_query(
            &db,
            &mut Cursor::new(vec![]),
            &PreparedQuery::Debits {
                show_transaction_id: false,
            },
            &vec![],
            &vec![],
            None,
            None,
            Some(16.0),
            None,
            None,
            vec!["000000 - 00000000"],
        )
        .unwrap();
        if let OutputForTesting::Table((_, rows)) = output_for_testing {
            assert_eq!(rows.len(), 5);
            assert_eq!(rows[0][2], "15.99");
            assert_eq!(rows[1][2], "10.00");
            assert_eq!(rows[2][2], "7.63");
            assert_eq!(rows[3][2], "3.74");
            assert_eq!(rows[4][2], "2.99");
        } else {
            unimplemented!()
        }
    }

    #[test]
    fn from() {
        let db = open_stingy_testing_database();
        db.insert_test_data();

        let output_for_testing = command_query(
            &db,
            &mut Cursor::new(vec![]),
            &PreparedQuery::Debits {
                show_transaction_id: false,
            },
            &vec![],
            &vec![],
            None,
            None,
            None,
            Some(NaiveDate::from_ymd_opt(2021, 03, 02).unwrap()),
            None,
            vec!["000000 - 00000000"],
        )
        .unwrap();
        if let OutputForTesting::Table((_, rows)) = output_for_testing {
            assert_eq!(rows.len(), 2);
            assert_eq!(rows[0][2], "25.15");
            assert_eq!(rows[1][2], "7.63");
        } else {
            unimplemented!()
        }
    }

    #[test]
    fn to() {
        let db = open_stingy_testing_database();
        db.insert_test_data();

        let output_for_testing = command_query(
            &db,
            &mut Cursor::new(vec![]),
            &PreparedQuery::Debits {
                show_transaction_id: false,
            },
            &vec![],
            &vec![],
            None,
            None,
            None,
            None,
            Some(NaiveDate::from_ymd_opt(2021, 02, 26).unwrap()),
            vec!["000000 - 00000000"],
        )
        .unwrap();
        if let OutputForTesting::Table((_, rows)) = output_for_testing {
            assert_eq!(rows.len(), 4);
            assert_eq!(rows[0][2], "35.98");
            assert_eq!(rows[1][2], "22.50");
            assert_eq!(rows[2][2], "10.00");
            assert_eq!(rows[3][2], "3.74");
        } else {
            unimplemented!()
        }
    }

    #[test]
    fn cumulative_debit() {
        let db = open_stingy_testing_database();
        db.insert_test_data();

        let output_for_testing = command_query(
            &db,
            &mut Cursor::new(vec![]),
            &PreparedQuery::Debits {
                show_transaction_id: false,
            },
            &vec![],
            &vec![],
            None,
            None,
            None,
            None,
            None,
            vec!["000000 - 00000000"],
        )
        .unwrap();
        if let OutputForTesting::Table((_, rows)) = output_for_testing {
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
        } else {
            unimplemented!()
        }
    }

    #[test]
    fn cumulative_percentage() {
        let db = open_stingy_testing_database();
        db.insert_test_data();

        let output_for_testing = command_query(
            &db,
            &mut Cursor::new(vec![]),
            &PreparedQuery::Debits {
                show_transaction_id: false,
            },
            &vec![],
            &vec![],
            None,
            None,
            None,
            None,
            None,
            vec!["000000 - 00000000"],
        )
        .unwrap();
        if let OutputForTesting::Table((_, rows)) = output_for_testing {
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
        } else {
            unimplemented!()
        }
    }

    #[test]
    fn date_format() {
        let db = open_stingy_testing_database();
        db.insert_test_data();

        let output_for_testing = command_query(
            &db,
            &mut Cursor::new(vec![]),
            &PreparedQuery::Debits {
                show_transaction_id: false,
            },
            &vec![],
            &vec![],
            None,
            None,
            None,
            None,
            None,
            vec!["000000 - 00000000"],
        )
        .unwrap();
        if let OutputForTesting::Table((_, rows)) = output_for_testing {
            assert_eq!(rows[0][4], "2021/02/26");
            // The first row is the one with the highest debit. The assertion
            // below is just for context, we're actually testing the date
            // format with the assertion above.
            assert_eq!(rows[0][3], "GROCERIES");
        } else {
            unimplemented!()
        }
    }

    #[test]
    fn account() {
        let db = open_stingy_testing_database();
        db.insert_test_data();

        let output_for_testing = command_query(
            &db,
            &mut Cursor::new(vec![]),
            &PreparedQuery::Debits {
                show_transaction_id: false,
            },
            &vec![],
            &vec![],
            None,
            None,
            None,
            None,
            None,
            vec!["000000 - 00000000"],
        )
        .unwrap();
        if let OutputForTesting::Table((_, rows)) = output_for_testing {
            assert_eq!(rows[0][0], "000000 - 00000000");
        } else {
            unimplemented!()
        }
    }

    #[test]
    fn not_tags() {
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
        let output_for_testing = command_query(
            &db,
            &mut Cursor::new(vec![]),
            &PreparedQuery::Debits {
                show_transaction_id: false,
            },
            &vec![],
            &vec!["daily/cof".to_string()],
            None,
            None,
            None,
            None,
            None,
            vec!["000000 - 00000000"],
        )
        .unwrap();
        if let OutputForTesting::Table((_, rows)) = output_for_testing {
            assert_eq!(rows.len(), 7);
        } else {
            unimplemented!()
        }
    }

    #[test]
    fn tags_and_not_tags() {
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
        let output_for_testing = command_query(
            &db,
            &mut Cursor::new(vec![]),
            &PreparedQuery::Debits {
                show_transaction_id: false,
            },
            &vec!["daily/cof".to_string()],
            &vec!["daily/cof".to_string()],
            None,
            None,
            None,
            None,
            None,
            vec!["000000 - 00000000"],
        )
        .unwrap();
        if let OutputForTesting::Table((_, rows)) = output_for_testing {
            assert_eq!(rows.len(), 0);
        } else {
            unimplemented!()
        }
    }
}

#[cfg(test)]
mod credits_tests {
    use super::*;
    use crate::database::open_stingy_testing_database;
    use std::io::Cursor;

    #[test]
    fn columns() {
        let db = open_stingy_testing_database();
        db.insert_test_data();

        let output_for_testing = command_query(
            &db,
            &mut Cursor::new(vec![]),
            &PreparedQuery::Credits {
                show_transaction_id: false,
            },
            &vec![],
            &vec![],
            None,
            None,
            None,
            None,
            None,
            vec!["000000 - 00000000"],
        )
        .unwrap();
        if let OutputForTesting::Table((columns, _)) = output_for_testing {
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
        } else {
            unimplemented!()
        }
    }

    #[test]
    fn with_transaction_id() {
        let db = open_stingy_testing_database();
        db.insert_test_data();

        let output_for_testing = command_query(
            &db,
            &mut Cursor::new(vec![]),
            &PreparedQuery::Credits {
                show_transaction_id: true,
            },
            &vec![],
            &vec![],
            None,
            None,
            None,
            None,
            None,
            vec!["000000 - 00000000"],
        )
        .unwrap();
        if let OutputForTesting::Table((columns, rows)) = output_for_testing {
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
        } else {
            unimplemented!()
        }
    }

    #[test]
    fn date_format() {
        let db = open_stingy_testing_database();
        db.insert_test_data();

        let output_for_testing = command_query(
            &db,
            &mut Cursor::new(vec![]),
            &PreparedQuery::Credits {
                show_transaction_id: false,
            },
            &vec![],
            &vec![],
            None,
            None,
            None,
            None,
            None,
            vec!["000000 - 00000000"],
        )
        .unwrap();
        if let OutputForTesting::Table((_, rows)) = output_for_testing {
            assert_eq!(rows[0][4], "2021/02/25");
        } else {
            unimplemented!()
        }
    }

    #[test]
    fn not_tags() {
        let db = open_stingy_testing_database();
        db.insert_test_data();

        crate::commands::tags::add_tag_rule(
            &db,
            "insurance",
            None,
            Some("insurance"),
            None,
            None,
            None,
            None,
            None,
        )
        .unwrap();
        let output_for_testing = command_query(
            &db,
            &mut Cursor::new(vec![]),
            &PreparedQuery::Credits {
                show_transaction_id: false,
            },
            &vec![],
            &vec!["insur".to_string()],
            None,
            None,
            None,
            None,
            None,
            vec![],
        )
        .unwrap();
        if let OutputForTesting::Table((_, rows)) = output_for_testing {
            println!("{:#?}", rows);
            assert_eq!(rows.len(), 3);
        } else {
            unimplemented!()
        }
    }
}

#[cfg(test)]
mod by_month_tests {
    use super::*;
    use crate::database::open_stingy_testing_database;
    use std::io::Cursor;

    #[test]
    fn columns() {
        let db = open_stingy_testing_database();
        db.insert_test_data();
        let output_for_testing = command_query(
            &db,
            &mut Cursor::new(vec![]),
            &PreparedQuery::ByMonth { table: true },
            &vec![],
            &vec![],
            None,
            None,
            None,
            None,
            None,
            vec![],
        )
        .unwrap();
        if let OutputForTesting::Table((columns, _)) = output_for_testing {
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
        } else {
            unimplemented!()
        }
    }

    #[test]
    fn hide_balance_when_filtering_by_tag() {
        let db = open_stingy_testing_database();
        db.insert_test_data();

        let output_for_testing = command_query(
            &db,
            &mut Cursor::new(vec![]),
            &PreparedQuery::ByMonth { table: true },
            &vec!["coffee".to_string()],
            &vec![],
            None,
            None,
            None,
            None,
            None,
            vec![],
        )
        .unwrap();
        if let OutputForTesting::Table((columns, _)) = output_for_testing {
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
        } else {
            unimplemented!()
        }
    }

    #[test]
    fn hide_balance_when_filtering_by_description() {
        let db = open_stingy_testing_database();
        db.insert_test_data();

        let output_for_testing = command_query(
            &db,
            &mut Cursor::new(vec![]),
            &PreparedQuery::ByMonth { table: true },
            &vec![],
            &vec![],
            Some("coffee"),
            None,
            None,
            None,
            None,
            vec![],
        )
        .unwrap();
        if let OutputForTesting::Table((columns, _)) = output_for_testing {
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
        } else {
            unimplemented!()
        }
    }

    #[test]
    fn hide_balance_when_filtering_by_amount() {
        let db = open_stingy_testing_database();
        db.insert_test_data();

        // amount_min
        let output_for_testing = command_query(
            &db,
            &mut Cursor::new(vec![]),
            &PreparedQuery::ByMonth { table: true },
            &vec![],
            &vec![],
            None,
            Some(0.0),
            None,
            None,
            None,
            vec![],
        )
        .unwrap();
        if let OutputForTesting::Table((columns, _)) = output_for_testing {
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
        } else {
            unimplemented!()
        }

        // amount_max
        let output_for_testing = command_query(
            &db,
            &mut Cursor::new(vec![]),
            &PreparedQuery::ByMonth { table: true },
            &vec![],
            &vec![],
            None,
            None,
            Some(10.0),
            None,
            None,
            vec![],
        )
        .unwrap();
        if let OutputForTesting::Table((columns, _)) = output_for_testing {
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
        } else {
            unimplemented!()
        }
    }

    #[test]
    fn multiple_months() {
        let db = open_stingy_testing_database();
        db.insert_test_data();

        let output_for_testing = command_query(
            &db,
            &mut Cursor::new(vec![]),
            &PreparedQuery::ByMonth { table: true },
            &vec![],
            &vec![],
            None,
            None,
            None,
            None,
            None,
            vec!["000000 - 00000000"],
        )
        .unwrap();
        if let OutputForTesting::Table((_, rows)) = output_for_testing {
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
        } else {
            unimplemented!()
        }
    }

    #[test]
    fn all_accounts() {
        let db = open_stingy_testing_database();
        db.insert_test_data();

        let output_for_testing = command_query(
            &db,
            &mut Cursor::new(vec![]),
            &PreparedQuery::ByMonth { table: true },
            &vec![],
            &vec![],
            None,
            None,
            None,
            None,
            None,
            vec![],
        )
        .unwrap();
        if let OutputForTesting::Table((_, rows)) = output_for_testing {
            assert_eq!(rows.len(), 5);
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
                    "1102.00",
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
                    "1102.00",
                    "72.22"
                ]
            );
            // Third account, March 2021
            assert_eq!(
                rows[2],
                vec![
                    "222222 - 22222222",
                    "2021/03",
                    "1.00",
                    "0.00",
                    "1.00",
                    "2.00",
                    "1002.00",
                    "72.22"
                ]
            );

            // First account, Feb 2021.
            assert_eq!(
                rows[3],
                vec![
                    "000000 - 00000000",
                    "2021/02",
                    "1000.00",
                    "72.22",
                    "927.78",
                    "9927.52",
                    "1001.00",
                    "72.22"
                ]
            );
            // Third account, Feb 2021.
            assert_eq!(
                rows[4],
                vec![
                    "222222 - 22222222",
                    "2021/02",
                    "1.00",
                    "0.00",
                    "1.00",
                    "1.00",
                    "1.00",
                    "0.00"
                ]
            );
            // There are no other transactions in any account.
        } else {
            unimplemented!()
        }
    }

    #[test]
    fn two_accounts() {
        let db = open_stingy_testing_database();
        db.insert_test_data();

        let output_for_testing = command_query(
            &db,
            &mut Cursor::new(vec![]),
            &PreparedQuery::ByMonth { table: true },
            &vec![],
            &vec![],
            None,
            None,
            None,
            None,
            None,
            vec!["000000 - 00000000", "111111 - 11111111"],
        )
        .unwrap();
        if let OutputForTesting::Table((_, rows)) = output_for_testing {
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
            // There are no other transactions in the accounts.
        } else {
            unimplemented!()
        }
    }

    #[test]
    fn display_account_alias_where_available() {
        let db = open_stingy_testing_database();
        db.insert_test_data();
        crate::commands::accounts::alias(&db, "000000 - 00000000", "Alias").unwrap();

        let output_for_testing = command_query(
            &db,
            &mut Cursor::new(vec![]),
            &PreparedQuery::ByMonth { table: true },
            &vec![],
            &vec![],
            None,
            None,
            None,
            None,
            None,
            vec!["000000 - 00000000"],
        )
        .unwrap();
        if let OutputForTesting::Table((_, rows)) = output_for_testing {
            assert_eq!(rows[0][0], "Alias");
        } else {
            unimplemented!()
        }
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
        let output_for_testing = command_query(
            &db,
            &mut Cursor::new(vec![]),
            &PreparedQuery::ByMonth { table: true },
            &vec![],
            &vec![],
            None,
            None,
            None,
            None,
            None,
            vec!["000000 - 00000000"],
        )
        .unwrap();
        if let OutputForTesting::Table((_, rows)) = output_for_testing {
            assert_eq!(rows.len(), 2);
            assert_eq!(rows[1][1], "2021/02");
            /* The coffee transaction is tagged twice, but aggregated only once. */
            assert_eq!(rows[1][3], "72.22");
        } else {
            unimplemented!()
        }
    }

    #[test]
    fn not_tags() {
        let db = open_stingy_testing_database();
        db.insert_test_data();
        crate::commands::tags::add_tag_rule(
            &db,
            "daily/coffee",
            None,
            Some("cof"),
            None,
            None,
            None,
            None,
            None,
        )
        .unwrap();
        let output_for_testing = command_query(
            &db,
            &mut Cursor::new(vec![]),
            &PreparedQuery::ByMonth { table: true },
            &vec![],
            &vec!["daily".to_string()],
            None,
            None,
            None,
            None,
            None,
            vec!["000000 - 00000000"],
        )
        .unwrap();
        if let OutputForTesting::Table((columns, rows)) = output_for_testing {
            assert_eq!(rows.len(), 2);
            assert_eq!(rows[1][1], "2021/02");
            /* The coffee transaction is omitted: 72.22 - 3.74 = 68.48. */
            assert_eq!(rows[1][3], "68.48");
            /* Make sure we also omit the balance here. */
            assert_eq!(
                columns
                    .iter()
                    .filter(|c| c.starts_with("Balance"))
                    .collect::<Vec<_>>()
                    .len(),
                0
            );
        } else {
            unimplemented!()
        }
    }
}

#[cfg(test)]
mod by_tag_tests {
    use super::*;
    use crate::database::open_stingy_testing_database;
    use std::io::Cursor;

    #[test]
    fn columns() {
        let db = open_stingy_testing_database();
        db.insert_test_data();
        let output_for_testing = command_query(
            &db,
            &mut Cursor::new(vec![]),
            &PreparedQuery::ByTag {
                transaction_type: None,
                table: true,
            },
            &vec![],
            &vec![],
            None,
            None,
            None,
            None,
            None,
            vec!["000000 - 00000000"],
        )
        .unwrap();
        if let OutputForTesting::Table((columns, _)) = output_for_testing {
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
        } else {
            unimplemented!()
        }
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
        let output_for_testing = command_query(
            &db,
            &mut Cursor::new(vec![]),
            &PreparedQuery::ByTag {
                transaction_type: None,
                table: true,
            },
            &vec![],
            &vec![],
            None,
            None,
            None,
            Some(NaiveDate::from_ymd_opt(2021, 02, 25).unwrap()),
            None,
            vec!["000000 - 00000000"],
        )
        .unwrap();
        if let OutputForTesting::Table((_, rows)) = output_for_testing {
            assert_eq!(rows.len(), 3);
            assert_eq!(rows[0], vec!["", "117.25", "83.76", "1000.00", "100.00"]); // untagged
            assert_eq!(rows[1], vec!["pub", "16.00", "11.43", "0.00", "0.00"]);
            assert_eq!(rows[2], vec!["coffee", "6.73", "4.81", "0.00", "0.00"]);
        } else {
            unimplemented!()
        }
    }

    #[test]
    fn all_accounts_tags() {
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
        let output_for_testing = command_query(
            &db,
            &mut Cursor::new(vec![]),
            &PreparedQuery::ByTag {
                transaction_type: None,
                table: true,
            },
            &vec![],
            &vec![],
            None,
            None,
            None,
            Some(NaiveDate::from_ymd_opt(2021, 02, 25).unwrap()),
            None,
            vec![],
        )
        .unwrap();
        if let OutputForTesting::Table((_, rows)) = output_for_testing {
            assert_eq!(rows.len(), 2);
            // Tags are aggregated across accounts.
            assert_eq!(rows[1][0], "credit");
            assert_eq!(rows[1][3], "1101.00");
        } else {
            unimplemented!()
        }
    }

    #[test]
    fn with_transaction_type() {
        let db = open_stingy_testing_database();
        db.insert_test_data();

        let output_for_testing = command_query(
            &db,
            &mut Cursor::new(vec![]),
            &PreparedQuery::ByTag {
                transaction_type: Some(crate::TransactionType::debit),
                table: true,
            },
            &vec![],
            &vec![],
            None,
            None,
            None,
            None,
            None,
            vec![],
        )
        .unwrap();
        if let OutputForTesting::Table((_, rows)) = output_for_testing {
            assert_eq!(rows[0], ["", "139.98", "100.00", "0.00", "0.00"]);
            //                                                   ^^^^^^
            // We want to check that there's no division by zero in the
            // % fields as no credit transactions are selected.
        } else {
            unimplemented!()
        }
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
        let output_for_testing = command_query(
            &db,
            &mut Cursor::new(vec![]),
            &PreparedQuery::ByTag {
                transaction_type: None,
                table: true,
            },
            &vec![],
            &vec![],
            None,
            None,
            None,
            Some(NaiveDate::from_ymd_opt(2021, 02, 25).unwrap()),
            None,
            vec!["000000 - 00000000"],
        )
        .unwrap();
        if let OutputForTesting::Table((_, rows)) = output_for_testing {
            assert_eq!(rows.len(), 2);
            assert_eq!(rows[1], vec!["pub", "16.00", "11.43", "0.00", "0.00"]);
        } else {
            unimplemented!()
        }
    }

    #[test]
    fn not_tags() {
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
        let output_for_testing = command_query(
            &db,
            &mut Cursor::new(vec![]),
            &PreparedQuery::ByTag {
                transaction_type: None,
                table: true,
            },
            &vec![],
            &vec!["coffee".to_string()],
            None,
            None,
            None,
            Some(NaiveDate::from_ymd_opt(2021, 02, 25).unwrap()),
            None,
            vec!["000000 - 00000000"],
        )
        .unwrap();
        if let OutputForTesting::Table((_, rows)) = output_for_testing {
            assert_eq!(rows.len(), 2);
            assert_eq!(rows[0], vec!["", "117.25", "87.99", "1000.00", "100.00"]); // untagged
            assert_eq!(rows[1], vec!["pub", "16.00", "12.01", "0.00", "0.00"]);
        } else {
            unimplemented!()
        }
    }
}

#[cfg(test)]
mod chart_tests {
    use super::*;
    use crate::database::open_stingy_testing_database;
    use serde_json;
    use std::io::Cursor;

    #[test]
    fn by_month_simple() {
        let db = open_stingy_testing_database();
        db.insert_test_data();

        let output_for_testing = command_query(
            &db,
            &mut Cursor::new(vec![]),
            &PreparedQuery::ByMonth { table: false },
            &vec![],
            &vec![],
            None,
            None,
            None,
            None,
            None,
            vec![],
        )
        .unwrap();
        if let OutputForTesting::Chart(chart_json) = output_for_testing {
            let chart = serde_json::from_str::<serde_json::Value>(&chart_json).unwrap();
            assert_eq!(chart.get("xAxis").unwrap().as_array().unwrap().len(), 2);
            assert_eq!(chart.get("yAxis").unwrap().as_array().unwrap().len(), 2);
        } else {
            unimplemented!()
        }
    }

    #[test]
    fn by_month_hide_balance_when_filtering_by_tag() {
        let db = open_stingy_testing_database();
        db.insert_test_data();

        let output_for_testing = command_query(
            &db,
            &mut Cursor::new(vec![]),
            &PreparedQuery::ByMonth { table: false },
            &vec!["coffee".to_string()],
            &vec![],
            None,
            None,
            None,
            None,
            None,
            vec![],
        )
        .unwrap();
        if let OutputForTesting::Chart(chart_json) = output_for_testing {
            let chart = serde_json::from_str::<serde_json::Value>(&chart_json).unwrap();
            assert_eq!(chart.get("xAxis").unwrap().as_array().unwrap().len(), 1);
            assert_eq!(chart.get("yAxis").unwrap().as_array().unwrap().len(), 1);
        } else {
            unimplemented!()
        }
    }
}
