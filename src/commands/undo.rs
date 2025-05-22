use crate::database::StingyDatabase;
use crate::{with_confirmation, WARN};
use anyhow::Result;

const MAX_UNDO_STEPS: usize = 128;

pub fn begin_undo_step(db: &Box<dyn StingyDatabase>, name: &str) -> Result<()> {
    db.begin_undo_step(name, MAX_UNDO_STEPS)
}

pub fn command_undo(db: &Box<dyn StingyDatabase>) -> Result<()> {
    let undo_step = db.get_last_undo_step()?;
    let prompt = format!("{WARN} Undo the results of command `{undo_step}`?");
    with_confirmation(&prompt, || db.undo_last_step())?;
    Ok(())
}

#[cfg(test)]
mod undo_tests {
    use super::*;
    use crate::commands::{accounts, tags};
    use crate::database::open_stingy_testing_database;
    use crate::model;
    use chrono::NaiveDate;

    #[test]
    fn undo_empty() {
        let db = open_stingy_testing_database();
        assert!(command_undo(&db).is_err());
    }

    #[test]
    fn undo_empty_step() {
        let db = open_stingy_testing_database();
        begin_undo_step(&db, "empty").unwrap();
        assert!(command_undo(&db).is_err());
    }

    #[test]
    fn undo_tag_rule() {
        let db = open_stingy_testing_database();
        begin_undo_step(&db, "undo_tag_rule").unwrap();

        tags::add_tag_rule(
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
        command_undo(&db).unwrap();
        let tag_rules = tags::list_tag_rules(&db, None).unwrap();
        assert_eq!(tag_rules.rows.len(), 0);
    }

    #[test]
    fn undo_tag_rule_deletion() {
        let db = open_stingy_testing_database();

        tags::add_tag_rule(
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
        begin_undo_step(&db, "undo_tag_rule_deletion").unwrap();
        tags::delete_tag_rule(&db, "1").unwrap();

        command_undo(&db).unwrap();
        let tag_rules = tags::list_tag_rules(&db, None).unwrap();
        assert_eq!(tag_rules.rows.len(), 1);
    }

    #[test]
    fn undo_tagged_transaction_import() {
        let db = open_stingy_testing_database();
        db.insert_test_data();
        tags::add_tag_rule(
            &db,
            "test1",
            None,
            Some("coffee"),
            None,
            None,
            None,
            None,
            None,
        )
        .unwrap();

        begin_undo_step(&db, &format!("import")).unwrap();

        let mut transaction = model::Transaction::default();
        transaction.account_name = "000000 - 00000000".to_string();
        transaction.description = "coffee".to_string();
        match db.insert(transaction).unwrap() {
            crate::database::NewOrExisting::New(_) => {}
            crate::database::NewOrExisting::Existing => {
                assert!(false);
            }
        }

        let before = db.count_transactions().unwrap();
        command_undo(&db).unwrap();
        assert_eq!(before, db.count_transactions().unwrap() + 1);
    }

    #[test]
    fn undo_account_select() {
        let db = open_stingy_testing_database();
        db.insert_test_data();

        begin_undo_step(&db, &format!("select")).unwrap();
        assert_eq!(
            accounts::get_account_or_selected(&db, None).unwrap().len(),
            0
        );
        accounts::select(&db, "000000 - 00000000").unwrap();
        assert_eq!(
            accounts::get_account_or_selected(&db, None).unwrap().len(),
            1
        );
        command_undo(&db).unwrap();
        assert_eq!(
            accounts::get_account_or_selected(&db, None).unwrap().len(),
            0
        );
    }

    #[test]
    fn undo_account_unselect() {
        let db = open_stingy_testing_database();
        db.insert_test_data();

        assert_eq!(
            accounts::get_account_or_selected(&db, None).unwrap().len(),
            0
        );
        accounts::select(&db, "000000 - 00000000").unwrap();
        assert_eq!(
            accounts::get_account_or_selected(&db, None).unwrap().len(),
            1
        );

        begin_undo_step(&db, &format!("unselect")).unwrap();
        accounts::unselect(&db, Some("000000 - 00000000")).unwrap();
        assert_eq!(
            accounts::get_account_or_selected(&db, None).unwrap().len(),
            0
        );

        command_undo(&db).unwrap();
        assert_eq!(
            accounts::get_account_or_selected(&db, None).unwrap().len(),
            1
        );
    }

    #[test]
    fn undo_account_unselect_multiple() {
        let db = open_stingy_testing_database();
        db.insert_test_data();

        assert_eq!(
            accounts::get_account_or_selected(&db, None).unwrap().len(),
            0
        );
        accounts::select(&db, "000000 - 00000000").unwrap();
        accounts::select(&db, "111111 - 11111111").unwrap();
        assert_eq!(
            accounts::get_account_or_selected(&db, None).unwrap().len(),
            2
        );

        begin_undo_step(&db, &format!("unselect")).unwrap();
        accounts::unselect(&db, None).unwrap();
        assert_eq!(
            accounts::get_account_or_selected(&db, None).unwrap().len(),
            0
        );

        command_undo(&db).unwrap();
        assert_eq!(
            accounts::get_account_or_selected(&db, None).unwrap().len(),
            2
        );
    }

    #[test]
    fn undo_test_data() {
        let db = open_stingy_testing_database();
        begin_undo_step(&db, &format!("insert_test_data")).unwrap();
        db.insert_test_data();

        assert_ne!(db.count_transactions().unwrap(), 0);
        command_undo(&db).unwrap();
        assert_eq!(db.count_transactions().unwrap(), 0);
    }

    #[test]
    fn truncate_history() {
        let db = open_stingy_testing_database();

        for step in 0..MAX_UNDO_STEPS + 1 {
            begin_undo_step(&db, &format!("undo_truncate_step_{step}")).unwrap();
            let account = model::Account {
                id: None,
                name: format!("account_{step}"),
                alias: None,
                selected: false,
                bank: None,
            };
            db.insert(account).unwrap();
        }

        for _ in 0..MAX_UNDO_STEPS {
            command_undo(&db).unwrap();
        }
        assert!(command_undo(&db).is_err());
    }
}
