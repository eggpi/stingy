use crate::database::{model, NewOrExisting, StingyDatabase};
use anyhow::{anyhow, bail, Result};
use chrono::NaiveDate;

#[derive(Debug)]
pub struct ListTagRulesResult {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

pub fn list_tag_rules(
    db: &Box<dyn StingyDatabase>,
    tag: Option<&str>,
) -> Result<ListTagRulesResult> {
    let mut tag_rules: Vec<model::TagRule> = db
        .get_all()?
        .into_iter()
        // Option::filter(f) returns None if f returns false.
        .filter(|tr: &model::TagRule| tag.filter(|t| !tr.tag.starts_with(*t)).is_none())
        .collect();
    tag_rules.sort_by_key(|tr| tr.id);
    let columns = vec![
        "ID".to_string(),
        "Tag".to_string(),
        "Description".to_string(),
    ];
    let rows: Vec<Vec<String>> = tag_rules
        .iter()
        .map(|tr| {
            vec![
                format!("{}", tr.id.unwrap()),
                tr.tag.clone(),
                tr.human_readable.clone(),
            ]
        })
        .collect();
    Ok(ListTagRulesResult { columns, rows })
}

#[derive(PartialEq, Debug)]
pub enum AddTagRuleResult {
    Added {
        tag_rule_id: i64,
        tagged_transactions: usize,
    },
    NotUnique {
        tag_rule_id: i64,
    },
}

// FIXME This would look better with a builder object instead of a bunch
// of Optionals.
pub fn add_tag_rule(
    db: &Box<dyn StingyDatabase>,
    tag: &str,
    transaction_id: Option<usize>,
    description_contains: Option<&str>,
    transaction_type: Option<model::TransactionType>,
    amount_min: Option<f64>,
    amount_max: Option<f64>,
    from: Option<NaiveDate>,
    to: Option<NaiveDate>,
) -> Result<AddTagRuleResult> {
    let mut human_readable = Vec::new();

    if let Some(tid) = transaction_id {
        human_readable.push(format!("the transaction id is '{tid}'"));
    }

    if let Some(dc) = description_contains {
        human_readable.push(format!("the description contains '{dc}'"));
    }
    if let Some(amin) = amount_min {
        if amin != f64::MIN {
            human_readable.push(format!("the amount is larger or equal to '{amin}'"));
        }
    }
    if let Some(amax) = amount_max {
        if amax != f64::MAX {
            human_readable.push(format!("the amount is smaller than '{amax}'"));
        }
    }

    match (from, to) {
        (Some(from), None) => {
            if from != NaiveDate::MIN {
                human_readable.push(format!("the date is after {}", from.format("%Y/%m/%d")));
            }
        }
        (None, Some(to)) => {
            if to != NaiveDate::MAX {
                human_readable.push(format!("the date is before {}", to.format("%Y/%m/%d")));
            }
        }
        (Some(from), Some(to)) => {
            human_readable.push(format!(
                "the date is between {} and {}",
                from.format("%Y/%m/%d"),
                to.format("%Y/%m/%d")
            ));
        }
        _ => {}
    }

    let human_readable = format!(
        "Apply tag '{tag}' to {} transactions where {}.",
        if let Some(ref tt) = transaction_type {
            format!("{tt:?} ")
        } else {
            "any".to_string()
        },
        human_readable.join(", and ")
    );

    let model = model::TagRule {
        id: None,
        tag: tag.to_string(),
        human_readable: human_readable,
        transaction_id: transaction_id.map(|t| t as i64),
        description_contains: description_contains.map(|s| s.to_string()),
        transaction_type: transaction_type,
        amount_min: amount_min,
        amount_max: amount_max,
        from_date: from,
        to_date: to,
    };

    match db.lookup_tag_rule(&model)? {
        Some(tag_rule_id) => Ok(AddTagRuleResult::NotUnique {
            tag_rule_id: tag_rule_id,
        }),
        None => {
            if let NewOrExisting::New(model) = db.insert(model)? {
                let tag_rule_id = model.id.unwrap();
                let tagged_transactions =
                    db.count_matching_transactions(&format!("{tag_rule_id}"))?;
                Ok(AddTagRuleResult::Added {
                    tag_rule_id,
                    tagged_transactions,
                })
            } else {
                bail!("Tag can't be looked up, but also can't be inserted?");
            }
        }
    }
}

pub fn delete_tag_rule(db: &Box<dyn StingyDatabase>, id: &str) -> Result<usize> {
    let mut model = model::TagRule::default();
    model.id = Some(id.parse().map_err(|_| anyhow!("id is not a number"))?);
    db.delete(model)
}

#[cfg(test)]
mod tags_tests {
    use super::*;
    use crate::database::open_stingy_testing_database;

    #[test]
    fn add_tag_rule_description_contains() {
        let db = open_stingy_testing_database();
        db.insert_test_data();
        let result = add_tag_rule(
            &db,
            "test",
            None,
            Some("GRoCeRiEs"), // Should be case-insensitive.
            None,
            None,
            None,
            None,
            None,
        )
        .unwrap();
        let expected = AddTagRuleResult::Added {
            tag_rule_id: 1,
            tagged_transactions: 2,
        };
        assert_eq!(result, expected);
    }

    #[test]
    fn add_tag_rule_transaction_type() {
        let db = open_stingy_testing_database();
        db.insert_test_data();
        let result = add_tag_rule(
            &db,
            "test",
            None,
            None,
            Some(model::TransactionType::Debit),
            None,
            None,
            None,
            None,
        )
        .unwrap();
        let expected = AddTagRuleResult::Added {
            tag_rule_id: 1,
            tagged_transactions: 9, // Also matches direct debit.
        };
        assert_eq!(result, expected);
    }

    #[test]
    fn add_tag_rule_amount_min() {
        let db = open_stingy_testing_database();
        db.insert_test_data();
        let result =
            add_tag_rule(&db, "test", None, None, None, Some(20.0), None, None, None).unwrap();
        let expected = AddTagRuleResult::Added {
            tag_rule_id: 1,
            tagged_transactions: 5, // Matches only the top transactions, of any type.
        };
        assert_eq!(result, expected);
    }

    #[test]
    fn add_tag_rule_amount_max() {
        let db = open_stingy_testing_database();
        db.insert_test_data();
        let result =
            add_tag_rule(&db, "test", None, None, None, None, Some(10.0), None, None).unwrap();
        let expected = AddTagRuleResult::Added {
            tag_rule_id: 1,
            tagged_transactions: 5, // 3 Debits, 2 Credits in different accounts.
        };
        assert_eq!(result, expected);
    }

    #[test]
    fn add_tag_rule_from() {
        let db = open_stingy_testing_database();
        db.insert_test_data();
        let result = add_tag_rule(
            &db,
            "test",
            None,
            None,
            None,
            None,
            None,
            Some(NaiveDate::from_ymd_opt(2021, 02, 26).unwrap()),
            None,
        )
        .unwrap();
        let expected = AddTagRuleResult::Added {
            tag_rule_id: 1,
            tagged_transactions: 10,
        };
        assert_eq!(result, expected);
    }

    #[test]
    fn add_tag_rule_to() {
        let db = open_stingy_testing_database();
        db.insert_test_data();
        let result = add_tag_rule(
            &db,
            "test",
            None,
            None,
            None,
            None,
            None,
            None,
            Some(NaiveDate::from_ymd_opt(2021, 02, 26).unwrap()),
        )
        .unwrap();
        let expected = AddTagRuleResult::Added {
            tag_rule_id: 1,
            tagged_transactions: 6,
        };
        assert_eq!(result, expected);
    }

    #[test]
    fn add_tag_rule_transaction_id() {
        let db = open_stingy_testing_database();
        db.insert_test_data();
        let result =
            add_tag_rule(&db, "test", Some(8), None, None, None, None, None, None).unwrap();
        let expected = AddTagRuleResult::Added {
            tag_rule_id: 1,
            tagged_transactions: 1,
        };
        assert_eq!(result, expected);
        assert_eq!(db.count_matching_transactions("1").unwrap(), 1);
    }

    #[test]
    fn add_tag_match_after() {
        let db = open_stingy_testing_database();
        let result = add_tag_rule(
            &db,
            "test",
            None,
            None,
            None,
            None,
            None,
            None,
            Some(NaiveDate::from_ymd_opt(2021, 02, 25).unwrap()),
        )
        .unwrap();
        let expected = AddTagRuleResult::Added {
            tag_rule_id: 1,
            tagged_transactions: 0, // The database is empty.
        };
        assert_eq!(result, expected);

        db.insert_test_data();
        assert_eq!(db.count_matching_transactions("1").unwrap(), 3); // Now we have matches.
    }

    #[test]
    fn add_tag_rule_duplicate() {
        let db = open_stingy_testing_database();
        add_tag_rule(
            &db,
            "test",
            None,
            None,
            Some(model::TransactionType::Debit),
            None,
            None,
            None,
            Some(NaiveDate::from_ymd_opt(2021, 02, 26).unwrap()),
        )
        .unwrap();
        let result = add_tag_rule(
            &db,
            "test",
            None,
            None,
            Some(model::TransactionType::Debit),
            None,
            None,
            None,
            Some(NaiveDate::from_ymd_opt(2021, 02, 26).unwrap()),
        );
        assert_eq!(
            result.unwrap(),
            AddTagRuleResult::NotUnique { tag_rule_id: 1 }
        );
    }

    #[test]
    fn list_tag_rules_none() {
        let db = open_stingy_testing_database();
        let result = list_tag_rules(&db, None).unwrap();
        assert_eq!(result.columns, vec!["ID", "Tag", "Description",]);
        assert_eq!(result.rows.len(), 0);
    }

    #[test]
    fn list_tag_rules_multiple() {
        let db = open_stingy_testing_database();
        add_tag_rule(
            &db,
            "test1",
            None,
            None,
            None,
            None,
            None,
            None,
            Some(NaiveDate::from_ymd_opt(2021, 02, 24).unwrap()),
        )
        .unwrap();
        add_tag_rule(
            &db,
            "test2",
            None,
            None,
            None,
            None,
            None,
            None,
            Some(NaiveDate::from_ymd_opt(2021, 02, 25).unwrap()),
        )
        .unwrap();
        add_tag_rule(
            &db,
            "test3",
            None,
            None,
            None,
            None,
            None,
            None,
            Some(NaiveDate::from_ymd_opt(2021, 02, 26).unwrap()),
        )
        .unwrap();
        let result = list_tag_rules(&db, None).unwrap();
        assert_eq!(result.rows.len(), 3);
        assert_eq!(result.rows[0][0], "1");
        assert_eq!(result.rows[0][1], "test1");
        assert_eq!(result.rows[1][0], "2");
        assert_eq!(result.rows[1][1], "test2");
        assert_eq!(result.rows[2][0], "3");
        assert_eq!(result.rows[2][1], "test3");
    }

    #[test]
    fn list_tag_rules_filter() {
        let db = open_stingy_testing_database();
        add_tag_rule(
            &db,
            "test1",
            None,
            None,
            None,
            None,
            None,
            None,
            Some(NaiveDate::from_ymd_opt(2021, 02, 24).unwrap()),
        )
        .unwrap();
        add_tag_rule(
            &db,
            "test2",
            None,
            None,
            None,
            None,
            None,
            None,
            Some(NaiveDate::from_ymd_opt(2021, 02, 25).unwrap()),
        )
        .unwrap();
        // Prefix match.
        let result = list_tag_rules(&db, Some("test")).unwrap();
        assert_eq!(result.rows.len(), 2);
        assert_eq!(result.rows[0][1], "test1");
        assert_eq!(result.rows[1][1], "test2");

        let result = list_tag_rules(&db, Some("test1")).unwrap();
        assert_eq!(result.rows.len(), 1);
        assert_eq!(result.rows[0][1], "test1");

        let result = list_tag_rules(&db, Some("test2")).unwrap();
        assert_eq!(result.rows.len(), 1);
        assert_eq!(result.rows[0][1], "test2");

        let result = list_tag_rules(&db, Some("test3")).unwrap();
        assert_eq!(result.rows.len(), 0);
    }

    #[test]
    fn delete_one_tag_rule() {
        let db = open_stingy_testing_database();
        add_tag_rule(
            &db,
            "test",
            None,
            Some("transfer"),
            None,
            None,
            None,
            None,
            None,
        )
        .unwrap();
        assert_eq!(delete_tag_rule(&db, &1.to_string()).unwrap(), 1);

        // Check that we don't panic for invalid ids.
        assert_eq!(delete_tag_rule(&db, &1.to_string()).unwrap(), 0);
        assert_eq!(delete_tag_rule(&db, &11.to_string()).unwrap(), 0);

        // Check that we return an error when we can't parse.
        assert!(delete_tag_rule(&db, "eleven").is_err());
    }

    #[test]
    fn untag_after_deleting() {
        let db = open_stingy_testing_database();
        db.insert_test_data();
        add_tag_rule(&db, "test", None, Some("PUB"), None, None, None, None, None).unwrap();
        add_tag_rule(
            &db,
            "test",
            None,
            Some("TRANSFER"),
            None,
            None,
            None,
            None,
            None,
        )
        .unwrap();

        // One match for each rule.
        assert_eq!(db.count_matching_transactions("1").unwrap(), 1);
        assert_eq!(db.count_matching_transactions("2").unwrap(), 1);

        assert_eq!(delete_tag_rule(&db, "1").unwrap(), 1);

        // Only rule 2 still matches.
        assert_eq!(db.count_matching_transactions("1").unwrap(), 0);
        assert_eq!(db.count_matching_transactions("2").unwrap(), 1);
    }

    #[test]
    fn transaction_id_tag_overrides_other_tags() {
        let db = open_stingy_testing_database();
        db.insert_test_data();

        // Tag transaction 7 using a rule (that doesn't refer to its ID).
        assert_eq!(
            add_tag_rule(&db, "pub", None, Some("PUB"), None, None, None, None, None).unwrap(),
            AddTagRuleResult::Added {
                tag_rule_id: 1,
                tagged_transactions: 1
            }
        );
        assert_eq!(db.count_matching_transactions("1").unwrap(), 1);

        // Now tag it by ID, the previous rule disappears.
        assert_eq!(
            add_tag_rule(&db, "not pub", Some(7), None, None, None, None, None, None).unwrap(),
            AddTagRuleResult::Added {
                tag_rule_id: 2,
                tagged_transactions: 1
            }
        );
        assert_eq!(db.count_matching_transactions("1").unwrap(), 0);
        assert_eq!(db.count_matching_transactions("2").unwrap(), 1);

        // Add another tag using the transaction ID, both should stay.
        assert_eq!(
            add_tag_rule(&db, "not cafe", Some(7), None, None, None, None, None, None).unwrap(),
            AddTagRuleResult::Added {
                tag_rule_id: 3,
                tagged_transactions: 1
            }
        );
        assert_eq!(db.count_matching_transactions("1").unwrap(), 0);
        assert_eq!(db.count_matching_transactions("2").unwrap(), 1);
        assert_eq!(db.count_matching_transactions("3").unwrap(), 1);

        // Remove one of the rules we added, the other should stay.
        delete_tag_rule(&db, "2").unwrap();
        assert_eq!(db.count_matching_transactions("1").unwrap(), 0);
        assert_eq!(db.count_matching_transactions("2").unwrap(), 0);
        assert_eq!(db.count_matching_transactions("3").unwrap(), 1);

        // Remove the other rule, now the original rule should return.
        delete_tag_rule(&db, "3").unwrap();
        assert_eq!(db.count_matching_transactions("1").unwrap(), 1);
        assert_eq!(db.count_matching_transactions("2").unwrap(), 0);
        assert_eq!(db.count_matching_transactions("3").unwrap(), 0);
    }

    #[test]
    fn transaction_id_tag_not_unique() {
        let db = open_stingy_testing_database();
        db.insert_test_data();

        assert_eq!(
            add_tag_rule(
                &db,
                "not pub",
                None,
                Some("PUB"),
                None,
                None,
                None,
                None,
                None
            )
            .unwrap(),
            AddTagRuleResult::Added {
                tag_rule_id: 1,
                tagged_transactions: 1
            }
        );
        assert_eq!(db.count_matching_transactions("1").unwrap(), 1);

        // A rule with transaction ID overrides a rule without, even if they set
        // the same tag.
        assert_eq!(
            add_tag_rule(&db, "not pub", Some(7), None, None, None, None, None, None).unwrap(),
            AddTagRuleResult::Added {
                tag_rule_id: 2,
                tagged_transactions: 1
            }
        );
        assert_eq!(db.count_matching_transactions("1").unwrap(), 0);
        assert_eq!(db.count_matching_transactions("2").unwrap(), 1);
    }

    // TODO Verify human_readable behavior
}
