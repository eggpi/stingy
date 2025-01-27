use crate::database::{model, StingyDatabase};
use anyhow::{anyhow, Result};

pub struct ListAccountsResult {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

pub fn list(db: &Box<dyn StingyDatabase>) -> Result<ListAccountsResult> {
    let accounts: Vec<model::Account> = db.get_all()?;
    let columns = vec![
        "Name".to_string(),
        "Alias".to_string(),
        "Selected".to_string(),
    ];
    let rows: Vec<Vec<_>> = accounts
        .iter()
        .map(|a| {
            vec![
                a.name.clone(),
                a.alias.clone().unwrap_or("".to_string()),
                if a.selected { crate::OK } else { "" }.to_string(),
            ]
        })
        .collect();
    Ok(ListAccountsResult { columns, rows })
}

pub fn get_account_or_selected(
    db: &Box<dyn StingyDatabase>,
    account_name: Option<&str>,
) -> Result<Vec<model::Account>> {
    let accounts: Vec<model::Account> = db.get_all()?;
    let mut selected = vec![];
    for account in &accounts {
        if account.selected {
            selected.push(account.clone());
        }
        if Some(account.name.as_str()) == account_name
            || (account.alias.is_some() && account.alias.as_deref() == account_name)
        {
            return Ok(vec![account.clone()]);
        }
    }
    if account_name.is_none() {
        Ok(selected)
    } else {
        Err(anyhow!("account or alias not found."))
    }
}

pub fn select(db: &Box<dyn StingyDatabase>, account_name: &str) -> Result<Vec<model::Account>> {
    let mut accounts = get_account_or_selected(db, Some(account_name))?;
    for account in &mut accounts {
        account.selected = true;
        db.update(account)?;
    }
    Ok(accounts)
}

pub fn unselect(db: &Box<dyn StingyDatabase>, account_name: Option<&str>) -> Result<()> {
    let mut accounts = get_account_or_selected(db, account_name)?;
    for account in &mut accounts {
        account.selected = false;
        db.update(account)?;
    }
    Ok(())
}

pub fn alias(
    db: &Box<dyn StingyDatabase>,
    account_name: &str,
    alias: &str,
) -> Result<model::Account> {
    let mut accounts = get_account_or_selected(db, Some(account_name))?;
    let account = &mut accounts[0];
    account.alias = Some(alias.to_string());
    db.update(account)?;
    Ok(account.clone())
}

pub fn delete_alias(db: &Box<dyn StingyDatabase>, alias: &str) -> Result<()> {
    let mut accounts = get_account_or_selected(db, Some(alias))?;
    let account = &mut accounts[0];
    account.alias = None;
    db.update(account)?;
    Ok(())
}

#[cfg(test)]
mod accounts_tests {
    use super::*;
    use crate::database::open_stingy_testing_database;

    #[test]
    fn alias_simple() {
        let db = open_stingy_testing_database();
        db.insert_test_data();

        let account = alias(&db, "000000 - 00000000", "0").unwrap();
        assert_eq!(account.name, "000000 - 00000000");
        assert_eq!(account.alias, Some("0".to_string()));

        let accounts: Vec<model::Account> = db.get_all().unwrap();
        let in_list = accounts
            .iter()
            .filter(|a| a.alias == Some("0".to_string()))
            .map(|a| a.clone())
            .next()
            .unwrap();
        assert_eq!(in_list.name, "000000 - 00000000");
        assert_eq!(in_list.alias, Some("0".to_string()));
    }

    #[test]
    fn alias_collision_with_alias() {
        let db = open_stingy_testing_database();
        db.insert_test_data();
        alias(&db, "000000 - 00000000", "0").unwrap();
        assert!(alias(&db, "111111 - 11111111", "0").is_err());
    }

    #[test]
    fn alias_collision_with_account() {
        let db = open_stingy_testing_database();
        db.insert_test_data();
        assert!(alias(&db, "111111 - 11111111", "000000 - 00000000").is_err());
    }

    #[test]
    fn alias_bad_account() {
        let db = open_stingy_testing_database();
        db.insert_test_data();
        assert!(alias(&db, "bad", "0").is_err());
    }

    #[test]
    fn delete_alias_simple() {
        let db = open_stingy_testing_database();
        db.insert_test_data();

        alias(&db, "000000 - 00000000", "0").unwrap();
        delete_alias(&db, "0").unwrap();
        let list: Vec<model::Account> = db.get_all().unwrap();
        let in_list = list
            .iter()
            .filter(|a| a.alias == Some("0".to_string()))
            .map(|a| a.clone())
            .next();
        assert!(in_list.is_none());
    }

    #[test]
    fn resolve_alias() {
        let db = open_stingy_testing_database();
        db.insert_test_data();

        alias(&db, "000000 - 00000000", "0").unwrap();
        let accounts = get_account_or_selected(&db, Some("0")).unwrap();
        let account = accounts.get(0).unwrap();
        assert_eq!(account.name, "000000 - 00000000");
        assert_eq!(account.alias, Some("0".to_string()));
    }

    #[test]
    fn resolve_account_name() {
        let db = open_stingy_testing_database();
        db.insert_test_data();

        let accounts = get_account_or_selected(&db, Some("000000 - 00000000")).unwrap();
        let account = accounts.get(0).unwrap();
        assert_eq!(account.name, "000000 - 00000000");
    }

    #[test]
    fn resolve_bad() {
        let db = open_stingy_testing_database();
        db.insert_test_data();
        assert!(get_account_or_selected(&db, Some("bad")).is_err());
    }

    #[test]
    fn select_account() {
        let db = open_stingy_testing_database();
        db.insert_test_data();
        select(&db, "111111 - 11111111").unwrap();

        let accounts = get_account_or_selected(&db, None).unwrap();
        let account = accounts.get(0).unwrap();
        assert_eq!(account.name, "111111 - 11111111");
    }

    #[test]
    fn select_multiple_accounts() {
        let db = open_stingy_testing_database();
        db.insert_test_data();
        select(&db, "111111 - 11111111").unwrap();
        select(&db, "000000 - 00000000").unwrap();

        let accounts = get_account_or_selected(&db, None).unwrap();
        let mut account_names: Vec<&str> = accounts.iter().map(|ac| ac.name.as_str()).collect();
        account_names.sort();
        assert_eq!(account_names, ["000000 - 00000000", "111111 - 11111111"]);
    }

    #[test]
    fn select_alias() {
        let db = open_stingy_testing_database();
        db.insert_test_data();
        alias(&db, "111111 - 11111111", "1").unwrap();
        select(&db, "1").unwrap();

        let accounts = get_account_or_selected(&db, None).unwrap();
        let account = accounts.get(0).unwrap();
        assert_eq!(account.name, "111111 - 11111111");
    }

    #[test]
    fn unselect_one() {
        let db = open_stingy_testing_database();
        db.insert_test_data();
        select(&db, "111111 - 11111111").unwrap();
        unselect(&db, Some("111111 - 11111111")).unwrap();
        assert_eq!(get_account_or_selected(&db, None).unwrap().len(), 0);
    }

    #[test]
    fn unselect_all() {
        let db = open_stingy_testing_database();
        db.insert_test_data();
        select(&db, "000000 - 00000000").unwrap();
        select(&db, "111111 - 11111111").unwrap();
        unselect(&db, None).unwrap();
        assert_eq!(get_account_or_selected(&db, None).unwrap().len(), 0);
    }

    #[test]
    fn unselect_invalid() {
        let db = open_stingy_testing_database();
        db.insert_test_data();
        select(&db, "000000 - 00000000").unwrap();
        select(&db, "111111 - 11111111").unwrap();
        assert!(unselect(&db, Some("22222 - 22222222")).is_err());
        assert_eq!(get_account_or_selected(&db, None).unwrap().len(), 2);
    }

    #[test]
    fn list_with_alias_and_selection() {
        let db = open_stingy_testing_database();
        db.insert_test_data();
        alias(&db, "000000 - 00000000", "0").unwrap();
        select(&db, "111111 - 11111111").unwrap();

        let mut result = list(&db).unwrap();
        assert_eq!(result.columns, vec!["Name", "Alias", "Selected",]);

        result
            .rows
            .sort_by(|row1, row2| row1[0].partial_cmp(&row2[0]).unwrap());
        assert_eq!(result.rows[0][0], "000000 - 00000000");
        assert_eq!(result.rows[0][1], "0");
        assert_eq!(result.rows[0][2], "");

        assert_eq!(result.rows[1][0], "111111 - 11111111");
        assert_eq!(result.rows[1][1], "");
        assert_eq!(result.rows[1][2], crate::OK);
    }
}
