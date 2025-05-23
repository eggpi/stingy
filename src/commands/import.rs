use crate::database::{model, NewOrExisting, StingyDatabase};
use anyhow::{anyhow, bail, Result};
use chrono::NaiveDate;
use std::collections::HashMap;
use std::io::Read;

struct Importer<'a> {
    db: &'a Box<dyn StingyDatabase>,
    accounts: HashMap<String, ()>,
    bank: String,
}

impl Importer<'_> {
    fn new<'a>(db: &'a Box<dyn StingyDatabase>, format: &ImportFormat) -> Result<Importer<'a>> {
        Ok(Importer {
            db: db,
            accounts: HashMap::new(),
            bank: match format {
                ImportFormat::AIB => "AIB".to_string(),
                ImportFormat::Revolut { .. } => "Revolut".to_string(),
            },
        })
    }

    fn insert(&mut self, transaction: model::Transaction) -> Result<()> {
        let account = model::Account {
            id: None,
            name: transaction.account_name.to_string(),
            alias: None,
            selected: false,
            bank: Some(self.bank.clone()),
        };
        match self.db.insert(account.clone())? {
            NewOrExisting::New(_) => {}
            NewOrExisting::Existing => self.ensure_bank_is_set(account)?,
        }
        self.accounts
            .insert(transaction.account_name.to_string(), ());
        self.db.insert(transaction).map(|_| ())
    }

    fn ensure_bank_is_set(&self, account: model::Account) -> Result<()> {
        let all_accounts: Vec<model::Account> = self.db.get_all()?;
        for mut existing in all_accounts {
            if existing.name == account.name && existing.bank.is_none() {
                existing.bank = account.bank;
                self.db.update(&existing)?;
                break;
            }
        }
        Ok(())
    }
}

pub enum ImportFormat<'a> {
    AIB,
    Revolut { account: &'a str, product: &'a str },
}

pub struct ImportResult {
    pub accounts: Vec<String>,
    pub imported: usize,
    pub before: Option<NaiveDate>,
    pub after: Option<NaiveDate>,
}

pub fn import<T>(
    db: &Box<dyn StingyDatabase>,
    paths_and_readers: &mut [(&str, T)],
    format: ImportFormat,
) -> Result<ImportResult>
where
    T: Read,
{
    let mut importer = Importer::new(db, &format)?;
    let before: Vec<model::Transaction> = db.get_all()?;
    let latest_before = before
        .iter()
        .max_by_key(|t: &&model::Transaction| t.posted_date);
    match format {
        ImportFormat::AIB => import_aib_csv(&mut importer, paths_and_readers)?,
        ImportFormat::Revolut { account, product } => {
            import_revolut_csv(&mut importer, paths_and_readers, &account, &product)?
        }
    }

    let after: Vec<model::Transaction> = db.get_all()?;
    let first_after = after
        .iter()
        .filter(|t: &&model::Transaction| {
            let latest_before = latest_before
                .and_then(|t| Some(t.posted_date))
                .unwrap_or(NaiveDate::MIN);
            t.posted_date > latest_before
        })
        .min_by_key(|t| t.posted_date);

    let mut accounts = vec![];
    accounts.extend(importer.accounts.into_keys());
    Ok(ImportResult {
        accounts: accounts,
        imported: after.len() - before.len(),
        before: latest_before.and_then(|t| Some(t.posted_date)),
        after: first_after.and_then(|t| Some(t.posted_date)),
    })
}

/* Revolut statements have a broken text encoding: Unicode, encoded as UTF-8, then _incorrectly_
 * decoded as Latin-1, then encoded as UTF-8.
 *
 * To fix it, we need to reverse the process: read the file and decode it as UTF-8 (Rust does
 * this for us), then encode the broken Unicode as Latin-1, then decode it as UTF-8.
 *
 * To encode to Latin-1 (or, reverse the incorrect decoding) we just convert each code point as
 * its unsigned byte value. This should work as long as the code points are actually representable
 * in the u8 range.
 *
 * This has the same effect of encoding_rs's encode_latin1_lossy function [1], but we implement it
 * ourselves to avoid taking on another dependency (and live dangerously exposed to bugs).
 *
 * 1- https://docs.rs/encoding_rs/latest/encoding_rs/mem/fn.encode_latin1_lossy.html
 */
fn fix_revolut_encoding(s: &str) -> String {
    let code_points = s.chars();
    let mut code_points_as_bytes = vec![];
    for code_point in code_points {
        let well_formed = '\u{0000}' < code_point && code_point <= '\u{00FF}';
        if !well_formed {
            return s.to_string();
        }
        code_points_as_bytes.push(code_point as u8);
    }
    String::from_utf8(code_points_as_bytes).unwrap_or(s.to_string())
}

fn import_revolut_csv<T>(
    importer: &mut Importer,
    paths_and_readers: &mut [(&str, T)],
    account: &str,
    product: &str,
) -> Result<()>
where
    T: Read,
{
    for (path, reader) in paths_and_readers {
        let mut reader = csv::ReaderBuilder::new()
            .delimiter(b',')
            .quote(b'"')
            .from_reader(reader);

        let header: Vec<String> = reader
            .headers()?
            .iter()
            .map(|h| h.trim().to_string())
            .collect();

        let mut line = 2; // line 1 is the header.
        for result in reader.records() {
            let record = result.map_err(|e| anyhow!("{}: {}", path, e))?;
            let mut transaction = model::Transaction::default();
            let as_kv: HashMap<String, String> = header
                .iter()
                .zip(record.iter())
                .map(|(h, r)| (h.clone(), r.to_string()))
                .collect();

            if let Some(st) = as_kv.get("State") {
                if st == "REVERTED" || st == "PENDING" {
                    line += 1;
                    continue;
                }
            }

            let pr = as_kv
                .get("Product")
                .ok_or(anyhow!("{path}:{line} has no 'Product' field"))?;
            if pr != product {
                line += 1;
                continue;
            }

            transaction.account_name = account.to_string();

            if let Some(ptd) = as_kv.get("Completed Date") {
                let date = ptd
                    .splitn(2, ' ')
                    .next()
                    .ok_or(anyhow!("{path}:{line} failed to parse 'Completed Date'"))?;
                transaction.posted_date =
                    NaiveDate::parse_from_str(date, "%Y-%m-%d").map_err(|_| {
                        anyhow!(
                            "{path}:{line} failed to parse 'Completed Date' (expected YYYY-MM-DD)"
                        )
                    })?;
            } else {
                bail!("{path}:{line} has no 'Completed Date' field!");
            }

            transaction.description =
                fix_revolut_encoding(&as_kv.get("Description").unwrap_or(&"".to_string()));

            if let Some(a) = as_kv.get("Amount") {
                let amount = if a == "" {
                    0.0
                } else {
                    a.parse()
                        .map_err(|_| anyhow!("{path}:{line} failed to parse 'Amount'"))?
                };
                if amount >= 0.0 {
                    transaction.transaction_type = model::TransactionType::Credit;
                    transaction.credit_amount = amount;
                } else {
                    transaction.transaction_type = model::TransactionType::Debit;
                    transaction.debit_amount = -amount;
                }
            } else {
                bail!("{path}:{line} has no 'Amount' field!");
            }

            if let Some(f) = as_kv.get("Fee") {
                let fee = if f == "" {
                    0.0
                } else {
                    f.parse()
                        .map_err(|_| anyhow!("{path}:{line} failed to parse 'Fee'"))?
                };
                if fee < 0.0 {
                    bail!("{path}:{line} has negative 'Fee' field!");
                }
                if transaction.transaction_type == model::TransactionType::Credit {
                    transaction.credit_amount -= fee;
                } else {
                    transaction.debit_amount += fee;
                }
            } else {
                bail!("{path}:{line} has no 'Fee' field!");
            }

            if let Some(ba) = as_kv.get("Balance") {
                if ba == "" {
                    // This seems to happen when Revolut moves your account to a different region.
                    continue;
                }
                transaction.balance = ba
                    .parse()
                    .map_err(|_| anyhow!("{path}:{line} failed to parse 'Balance'"))?;
            } else {
                bail!("{path}:{line} has no 'Balance' field!");
            }

            transaction.currency = as_kv
                .get("Currency")
                .ok_or(anyhow!("{path}:{line} has no 'Currency' field"))?
                .to_string();

            importer
                .insert(transaction)
                .map_err(|err| anyhow!("{path}:{line} failed insertion: {}", err))?;
            line += 1;
        }
    }
    Ok(())
}

/* There are two different CSV formats you can get from AIB's website:
 *
 * A. By clicking EXPORT on the account view page.
 * B. By clicking HISTORICAL on the account view page, then EXPORT.
 *
 * We support B, but not A, because I've found that A sometimes lacks some
 * of the transactions that happened on the first day of the export.
 *
 * In more detail, A differs from B in that:
 *  - The date format is DD/MM/YY.
 *  - The Balance column is only populated for the last transaction of the day.
 *  - The transaction dates are chronologically descending, not ascending.
 *  - There is a single Description column.
 *  - The last few columns are missing (e.g. Local Currency).
 *  - There is no "Direct Debit", only "Debit".
 *  - Some transactions have no debit or credit amounts (e.g. Interest rate)
 *  - Some transactions are split across lines.
 *  - There are no quotes around values.
 */

fn import_aib_csv<T>(importer: &mut Importer, paths_and_readers: &mut [(&str, T)]) -> Result<()>
where
    T: Read,
{
    for (path, reader) in paths_and_readers {
        let mut reader = csv::ReaderBuilder::new()
            .delimiter(b',')
            .quote(b'"')
            .from_reader(reader);

        let header: Vec<String> = reader
            .headers()?
            .iter()
            .map(|h| h.trim().to_string())
            .collect();

        // Fail if any file is in format A above.
        if !header.contains(&"Local Currency".to_string()) {
            bail!("{path} is not in the right format!");
        }

        let mut line = 2; // line 1 is the header.
        for result in reader.records() {
            let record = result.map_err(|e| anyhow!("{}: {}", path, e))?;
            let mut transaction = model::Transaction::default();
            let as_kv: HashMap<String, String> = header
                .iter()
                .zip(record.iter())
                .map(|(h, r)| (h.clone(), r.to_string()))
                .collect();

            transaction.account_name = as_kv
                .get("Posted Account")
                .ok_or(anyhow!("{path}:{line} has no 'Posted Account' field"))?
                .to_string();

            if let Some(ptd) = as_kv.get("Posted Transactions Date") {
                transaction.posted_date =
                    NaiveDate::parse_from_str(ptd, "%d/%m/%Y").map_err(|_| {
                        anyhow!(
                            "{path}:{line} failed to parse transaction date (expected DD/MM/YYYY)"
                        )
                    })?;
            } else {
                bail!("{path}:{line} has no 'Posted Transactions Date' field!");
            }

            {
                // Join the description fields present.
                let description_fields = vec![
                    // Used for recent transactions
                    "Description",
                    // Used for historical transactions
                    "Description1",
                    "Description2",
                    "Description3",
                ];

                let mut description = Vec::new();
                for df in description_fields {
                    if let Some(d) = as_kv.get(df) {
                        let trimmed = d.trim().to_string();
                        if trimmed != "" {
                            description.push(trimmed);
                        }
                    }
                }
                transaction.description = description.join(" / ");
            }

            if let Some(da) = as_kv.get("Debit Amount") {
                transaction.debit_amount = if da == "" {
                    0.0
                } else {
                    da.replacen(",", "", 1)
                        .parse()
                        .map_err(|_| anyhow!("{path}:{line} failed to parse debit amount"))?
                }
            } else {
                bail!("{path}:{line} has no 'Debit Amount' field!");
            }

            if let Some(cr) = as_kv.get("Credit Amount") {
                transaction.credit_amount = if cr == "" {
                    0.0
                } else {
                    cr.replacen(",", "", 1)
                        .parse()
                        .map_err(|_| anyhow!("{path}:{line} failed to parse credit amount"))?
                };
            } else {
                bail!("{path}:{line} has no 'Credit Amount' field!");
            }

            if let Some(ba) = as_kv.get("Balance") {
                transaction.balance = ba
                    .replacen(",", "", 1)
                    .parse()
                    .map_err(|_| anyhow!("{path}:{line} failed to parse balance"))?;
            } else {
                bail!("{path}:{line} has no 'Balance' field!");
            }

            if let Some(tt) = as_kv.get("Transaction Type") {
                transaction.transaction_type = match tt.as_str() {
                    "Topup" => model::TransactionType::Debit,
                    "ATM" => model::TransactionType::Debit,
                    "Debit" => model::TransactionType::Debit,
                    "Direct Debit" => model::TransactionType::DirectDebit,
                    "Credit" => model::TransactionType::Credit,
                    _ => bail!("{path}:{line} has unknown 'Transaction Type': {}", *tt),
                }
            } else {
                bail!("{path}:{line} has no 'Transaction Type' field!");
            }

            transaction.currency = as_kv
                .get("Posted Currency")
                .ok_or(anyhow!("{path}:{line} has no 'Currency' field"))?
                .to_string();

            importer
                .insert(transaction)
                .map_err(|err| anyhow!("{path}:{line} failed insertion: {}", err))?;
            line += 1;
        }
    }
    Ok(())
}

#[cfg(test)]
mod importer_tests {
    use super::*;
    use crate::database::open_stingy_testing_database;

    #[test]
    fn account_clashes_with_alias() {
        let db = open_stingy_testing_database();
        db.insert_test_data();
        crate::commands::accounts::alias(&db, "000000 - 00000000", "0").unwrap();

        let mut importer = Importer::new(&db, &ImportFormat::AIB).unwrap();
        let transaction = model::Transaction {
            id: None,
            account_name: "0".to_string(),
            posted_date: NaiveDate::from_ymd_opt(2021, 03, 01).unwrap(),
            description: "".to_string(),
            debit_amount: 0.0,
            credit_amount: 0.0,
            balance: 100.0,
            transaction_type: model::TransactionType::Debit,
            currency: "EUR".to_string(),
        };
        assert!(importer.insert(transaction).is_err());
    }
}

#[cfg(test)]
mod aib_import_tests {
    use super::*;
    use crate::database::open_stingy_testing_database;

    const CSV_HEADER: &str = concat!(
        r#"Posted Account, Posted Transactions Date, Description1, "#,
        r#"Description2, Description3, Debit Amount, Credit Amount,"#,
        r#"Balance,Posted Currency,Transaction Type,Local Currency Amount,"#,
        r#"Local Currency"#
    );

    const CREDIT_TRANSACTION: &str = concat!(
        r#""455556 - 05229944","26/02/2021","Transaction Description 1","#,
        r#""Transaction Description 2","Transaction Description 3",,"1,000.00","#,
        r#""3000.00",EUR,"Credit"," 1,000.00",EUR"#
    );

    const DEBIT_TRANSACTION: &str = concat!(
        r#""455556 - 05229944","25/02/2021","Transaction Description 1","#,
        r#""Transaction Description 2","Transaction Description 3","1,000.00",,"#,
        r#""3000.00",EUR,"Debit"," 1,000.00",EUR"#
    );

    #[test]
    fn import_one_row() {
        let csv = format!("{CSV_HEADER}\n{CREDIT_TRANSACTION}");
        let db = open_stingy_testing_database();
        let r = import(&db, &mut [("csv", csv.as_bytes())], ImportFormat::AIB).unwrap();
        assert_eq!(r.imported, 1);
        let transactions: Vec<model::Transaction> = db.get_all().unwrap();
        assert_eq!(transactions.len(), 1);
        assert_eq!(transactions[0], model::Transaction {
            id: Some(1),
            account_name: "455556 - 05229944".to_string(),
            posted_date: NaiveDate::from_ymd_opt(2021, 02, 26).unwrap(),
            // This also tests that we merge descriptions.
            description: "Transaction Description 1 / Transaction Description 2 / Transaction Description 3".to_string(),
            debit_amount: 0.0,
            credit_amount: 1000.0,
            balance: 3000.0,
            transaction_type: model::TransactionType::Credit,
            currency: "EUR".to_string(),
        });
    }

    #[test]
    fn import_accounts() {
        let db = open_stingy_testing_database();
        let csv = format!("{CSV_HEADER}\n{CREDIT_TRANSACTION}");
        let r = import(&db, &mut [("csv", csv.as_bytes())], ImportFormat::AIB).unwrap();
        assert_eq!(r.accounts, vec!["455556 - 05229944"]);

        // Also return the account when it already existed.
        let csv = format!("{CSV_HEADER}\n{DEBIT_TRANSACTION}");
        let r = import(&db, &mut [("csv", csv.as_bytes())], ImportFormat::AIB).unwrap();
        assert_eq!(r.accounts, vec!["455556 - 05229944"]);
    }

    #[test]
    fn import_multiple_accounts() {
        let db = open_stingy_testing_database();
        let csv = format!("{CSV_HEADER}\n{CREDIT_TRANSACTION}");
        import(&db, &mut [("csv", csv.as_bytes())], ImportFormat::AIB).unwrap();

        let csv = format!("{CSV_HEADER}\n{CREDIT_TRANSACTION}").replace("455556", "566667");
        let r = import(&db, &mut [("csv", csv.as_bytes())], ImportFormat::AIB).unwrap();
        assert_eq!(r.accounts, vec!["566667 - 05229944"]);
    }

    #[test]
    fn import_multiple_rows() {
        let csv = format!("{CSV_HEADER}\n{CREDIT_TRANSACTION}\n{DEBIT_TRANSACTION}");
        let db = open_stingy_testing_database();
        let r = import(&db, &mut [("csv", csv.as_bytes())], ImportFormat::AIB).unwrap();
        assert_eq!(r.imported, 2);
        let mut transactions: Vec<model::Transaction> = db.get_all().unwrap();
        transactions.sort_by_key(|t| format!("{:?}", t.transaction_type));
        assert_eq!(transactions.len(), 2);
        assert_eq!(
            transactions[0].transaction_type,
            model::TransactionType::Credit
        );
        assert_eq!(
            transactions[1].transaction_type,
            model::TransactionType::Debit
        );
    }

    #[test]
    fn import_multiple_files() {
        let credit_csv = format!("{CSV_HEADER}\n{CREDIT_TRANSACTION}");
        let debit_csv = format!("{CSV_HEADER}\n{DEBIT_TRANSACTION}");
        let db = open_stingy_testing_database();
        let r = import(
            &db,
            &mut [
                ("credit", credit_csv.as_bytes()),
                ("debit", debit_csv.as_bytes()),
            ],
            ImportFormat::AIB,
        )
        .unwrap();
        assert_eq!(r.imported, 2);
        let mut transactions: Vec<model::Transaction> = db.get_all().unwrap();
        transactions.sort_by_key(|t| format!("{:?}", t.transaction_type));
        assert_eq!(transactions.len(), 2);
        assert_eq!(
            transactions[0].transaction_type,
            model::TransactionType::Credit
        );
        assert_eq!(
            transactions[1].transaction_type,
            model::TransactionType::Debit
        );
    }

    #[test]
    fn import_count_duplicated_rows() {
        let csv = format!("{CSV_HEADER}\n{DEBIT_TRANSACTION}\n{DEBIT_TRANSACTION}");
        let db = open_stingy_testing_database();
        let r = import(
            &db,
            &mut [
                ("first_csv", csv.as_bytes()),
                ("second_csv", csv.as_bytes()),
            ],
            ImportFormat::AIB,
        )
        .unwrap();
        assert_eq!(r.imported, 1);
        let transactions: Vec<model::Transaction> = db.get_all().unwrap();
        assert_eq!(transactions.len(), 1);
    }

    #[test]
    fn topup_becomes_debit() {
        let csv = format!(
            "{CSV_HEADER}\n{}",
            DEBIT_TRANSACTION.replace("Debit", "Topup")
        );
        let db = open_stingy_testing_database();
        import(&db, &mut [("csv", csv.as_bytes())], ImportFormat::AIB).unwrap();
        let transactions: Vec<model::Transaction> = db.get_all().unwrap();
        assert_eq!(transactions.len(), 1);
        assert_eq!(
            transactions[0].transaction_type,
            model::TransactionType::Debit
        );
    }

    #[test]
    fn atm_becomes_debit() {
        let csv = format!(
            "{CSV_HEADER}\n{}",
            DEBIT_TRANSACTION.replace("Debit", "ATM")
        );
        let db = open_stingy_testing_database();
        import(&db, &mut [("csv", csv.as_bytes())], ImportFormat::AIB).unwrap();
        let transactions: Vec<model::Transaction> = db.get_all().unwrap();
        assert_eq!(transactions.len(), 1);
        assert_eq!(
            transactions[0].transaction_type,
            model::TransactionType::Debit
        );
    }

    #[test]
    fn direct_debit_is_preserved() {
        let csv = format!(
            "{CSV_HEADER}\n{}",
            DEBIT_TRANSACTION.replace("Debit", "Direct Debit")
        );
        let db = open_stingy_testing_database();
        import(&db, &mut [("csv", csv.as_bytes())], ImportFormat::AIB).unwrap();
        let transactions: Vec<model::Transaction> = db.get_all().unwrap();
        assert_eq!(transactions.len(), 1);
        assert_eq!(
            transactions[0].transaction_type,
            model::TransactionType::DirectDebit
        );
    }

    #[test]
    fn before_and_after_dates() {
        let csv = format!("{CSV_HEADER}\n{DEBIT_TRANSACTION}\n{CREDIT_TRANSACTION}");
        let db = open_stingy_testing_database();

        let r = import(&db, &mut [("csv", csv.as_bytes())], ImportFormat::AIB).unwrap();
        // The database was empty, so before should be empty.
        assert!(r.before.is_none());
        // After should be the date of the earliset imported transaction.
        assert_eq!(
            r.after.unwrap(),
            NaiveDate::from_ymd_opt(2021, 02, 25).unwrap()
        );

        // Insert a repeated row.
        let csv = format!("{CSV_HEADER}\n{DEBIT_TRANSACTION}");
        let r = import(&db, &mut [("csv", csv.as_bytes())], ImportFormat::AIB).unwrap();
        // Now before is the date of the last transaction inserted above...
        assert_eq!(
            r.before.unwrap(),
            NaiveDate::from_ymd_opt(2021, 02, 26).unwrap()
        );
        // ... but after is None, because no rows were added.
        assert!(r.after.is_none());

        // Insert a later transaction, now both before and after should change.
        let csv = format!(
            "{CSV_HEADER}\n{}",
            DEBIT_TRANSACTION.replace("25/02/2021", "27/02/2021")
        );
        let r = import(&db, &mut [("csv", csv.as_bytes())], ImportFormat::AIB).unwrap();
        assert_eq!(
            r.before.unwrap(),
            NaiveDate::from_ymd_opt(2021, 02, 26).unwrap()
        );
        assert_eq!(
            r.after.unwrap(),
            NaiveDate::from_ymd_opt(2021, 02, 27).unwrap()
        );
    }

    #[test]
    fn wrong_format_error() {
        // Fail on type A exports, see the big comment above.
        let db = open_stingy_testing_database();
        let csv = concat!(
            r#"Posted Account, Posted Transactions Date, Description, "#,
            r#"Debit Amount, Credit Amount,Balance,Transaction Type"#
        );
        assert!(import(&db, &mut [("csv", csv.as_bytes())], ImportFormat::AIB).is_err());
    }

    #[test]
    fn populate_account_bank() {
        let csv = format!("{CSV_HEADER}\n{DEBIT_TRANSACTION}");
        let db = open_stingy_testing_database();
        import(&db, &mut [("csv", csv.as_bytes())], ImportFormat::AIB).unwrap();
        let accounts: Vec<model::Account> = db.get_all().unwrap();
        assert_eq!(accounts.len(), 1);
        assert_eq!(accounts.get(0).unwrap().bank, Some("AIB".to_string()));
    }

    #[test]
    fn populate_account_bank_existing_account() {
        let csv = format!(
            "{CSV_HEADER}\n{}",
            DEBIT_TRANSACTION.replace("455556 - 05229944", "333333 - 33333333")
        );
        let db = open_stingy_testing_database();
        db.insert_test_data();
        import(&db, &mut [("csv", csv.as_bytes())], ImportFormat::AIB).unwrap();
        let accounts: Vec<model::Account> = db.get_all().unwrap();
        let account: &model::Account = accounts
            .iter()
            .filter(|a: &&model::Account| a.name == "333333 - 33333333")
            .collect::<Vec<&model::Account>>()
            .get(0)
            .unwrap();
        assert_eq!(account.bank, Some("AIB".to_string()));
    }
}

#[cfg(test)]
mod revolut_import_tests {
    use super::*;
    use crate::database::open_stingy_testing_database;

    const CSV_HEADER: &str = concat!(
        "Type,Product,Started Date,Completed Date,Description,",
        "Amount,Fee,Currency,State,Balance"
    );

    const INCOMING_TRANSFER: &str = concat!(
        "TRANSFER,Current,2021-02-26 9:18:01,2021-02-26 9:18:01,",
        "From Someone,32.64,0,EUR,COMPLETED,100.00"
    );

    const OUTGOING_TRANSFER: &str = concat!(
        "TRANSFER,Current,2021-02-26 9:18:01,2021-02-26 9:18:01,",
        "To Someone,-64.32,0,EUR,COMPLETED,100.00"
    );

    const CARD_PAYMENT: &str = concat!(
        "CARD_PAYMENT,Current,2021-03-01 13:18:44,2021-03-01 8:23:15,",
        "Coffee,-2,0,EUR,COMPLETED,100.00"
    );

    const ATM_WITHDRAWAL: &str = concat!(
        "ATM,Current,2021-03-01 12:18:24,2021-03-01 8:12:27,",
        "Cash at ATM,-64.00,0,EUR,COMPLETED,100.00"
    );

    const TOPUP: &str = concat!(
        "TOPUP,Current,2021-03-01 12:18:24,2021-03-01 8:12:27,",
        "Cash at ATM,64.00,0,EUR,COMPLETED,100.00"
    );

    const REVERTED: &str = concat!(
        "CARD_PAYMENT,Current,2021-03-01 12:18:24,2021-03-01 8:12:27,",
        "Shady,64.00,0,EUR,REVERTED,100.00"
    );

    const PENDING: &str = concat!(
        "CARD_PAYMENT,Current,2021-03-01 12:18:24,2021-03-01 8:12:27,",
        "Shady,64.00,0,EUR,PENDING,100.00"
    );

    const CREDIT_WITH_FEE: &str = concat!(
        "INTEREST,Current,2021-03-01 12:18:24,2021-03-01 8:12:27,",
        "Shady,3.22,1.07,EUR,COMPLETED,100.00"
    );

    #[test]
    fn import_one_row() {
        let csv = format!("{CSV_HEADER}\n{CARD_PAYMENT}");
        let db = open_stingy_testing_database();
        let r = import(
            &db,
            &mut [("csv", csv.as_bytes())],
            ImportFormat::Revolut {
                account: "0",
                product: "Current",
            },
        )
        .unwrap();
        assert_eq!(r.imported, 1);
        let transactions: Vec<model::Transaction> = db.get_all().unwrap();
        assert_eq!(transactions.len(), 1);
        assert_eq!(
            transactions[0],
            model::Transaction {
                id: Some(1),
                account_name: "0".to_string(),
                posted_date: NaiveDate::from_ymd_opt(2021, 03, 01).unwrap(),
                description: "Coffee".to_string(),
                debit_amount: 2.0,
                credit_amount: 0.0,
                balance: 100.0,
                transaction_type: model::TransactionType::Debit,
                currency: "EUR".to_string(),
            }
        );
    }

    #[test]
    fn import_multiple_rows() {
        let csv = format!("{CSV_HEADER}\n{INCOMING_TRANSFER}\n{OUTGOING_TRANSFER}");
        let db = open_stingy_testing_database();
        let r = import(
            &db,
            &mut [("csv", csv.as_bytes())],
            ImportFormat::Revolut {
                account: "0",
                product: "Current",
            },
        )
        .unwrap();
        assert_eq!(r.imported, 2);
        let mut transactions: Vec<model::Transaction> = db.get_all().unwrap();
        transactions.sort_by_key(|t| format!("{:?}", t.transaction_type));
        assert_eq!(transactions.len(), 2);
        assert_eq!(
            transactions[0].transaction_type,
            model::TransactionType::Credit
        );
        assert_eq!(transactions[0].credit_amount, 32.64);
        assert_eq!(
            transactions[1].transaction_type,
            model::TransactionType::Debit
        );
        assert_eq!(transactions[1].debit_amount, 64.32);
    }

    #[test]
    fn atm_becomes_debit() {
        let csv = format!("{CSV_HEADER}\n{}", ATM_WITHDRAWAL);
        let db = open_stingy_testing_database();
        import(
            &db,
            &mut [("csv", csv.as_bytes())],
            ImportFormat::Revolut {
                account: "0",
                product: "Current",
            },
        )
        .unwrap();
        let transactions: Vec<model::Transaction> = db.get_all().unwrap();
        assert_eq!(transactions.len(), 1);
        assert_eq!(
            transactions[0].transaction_type,
            model::TransactionType::Debit
        );
    }

    #[test]
    fn topup_becomes_credit() {
        let csv = format!("{CSV_HEADER}\n{}", TOPUP);
        let db = open_stingy_testing_database();
        import(
            &db,
            &mut [("csv", csv.as_bytes())],
            ImportFormat::Revolut {
                account: "0",
                product: "Current",
            },
        )
        .unwrap();
        let transactions: Vec<model::Transaction> = db.get_all().unwrap();
        assert_eq!(transactions.len(), 1);
        assert_eq!(
            transactions[0].transaction_type,
            model::TransactionType::Credit
        );
    }

    #[test]
    fn ignore_reverted() {
        let csv = format!("{CSV_HEADER}\n{}", REVERTED);
        let db = open_stingy_testing_database();
        import(
            &db,
            &mut [("csv", csv.as_bytes())],
            ImportFormat::Revolut {
                account: "0",
                product: "Current",
            },
        )
        .unwrap();
        let transactions: Vec<model::Transaction> = db.get_all().unwrap();
        assert_eq!(transactions.len(), 0);
    }

    #[test]
    fn ignore_pending() {
        let csv = format!("{CSV_HEADER}\n{}", PENDING);
        let db = open_stingy_testing_database();
        import(
            &db,
            &mut [("csv", csv.as_bytes())],
            ImportFormat::Revolut {
                account: "0",
                product: "Current",
            },
        )
        .unwrap();
        let transactions: Vec<model::Transaction> = db.get_all().unwrap();
        assert_eq!(transactions.len(), 0);
    }

    #[test]
    fn credit_with_fee() {
        let csv = format!("{CSV_HEADER}\n{}", CREDIT_WITH_FEE);
        let db = open_stingy_testing_database();
        import(
            &db,
            &mut [("csv", csv.as_bytes())],
            ImportFormat::Revolut {
                account: "0",
                product: "Current",
            },
        )
        .unwrap();
        let transactions: Vec<model::Transaction> = db.get_all().unwrap();
        assert_eq!(transactions.len(), 1);
        assert!((transactions[0].credit_amount - 2.15).abs() < 0.0000001);
    }

    #[test]
    fn ignore_product_mismatch() {
        // Same as credit_with_fee, but we pass a different product so the data should be ignored.
        let csv = format!("{CSV_HEADER}\n{}", CREDIT_WITH_FEE);
        let db = open_stingy_testing_database();
        import(
            &db,
            &mut [("csv", csv.as_bytes())],
            ImportFormat::Revolut {
                account: "0",
                product: "Deposit",
            },
        )
        .unwrap();
        let transactions: Vec<model::Transaction> = db.get_all().unwrap();
        assert_eq!(transactions.len(), 0);
    }

    #[test]
    fn fix_description_encoding() {
        let csv = format!(
            "{CSV_HEADER}\n{}\n{}\n{}\n{}",
            CARD_PAYMENT.replace("Coffee", "LUCKYâS"),
            CARD_PAYMENT.replace("Coffee", "CafÃ©"),
            CARD_PAYMENT.replace("Coffee", "CaffÃ¨"),
            CARD_PAYMENT.replace("Coffee", "FranÃ§ois")
        );
        let db = open_stingy_testing_database();
        let r = import(
            &db,
            &mut [("csv", csv.as_bytes())],
            ImportFormat::Revolut {
                account: "0",
                product: "Current",
            },
        )
        .unwrap();
        assert_eq!(r.imported, 4);
        let transactions: Vec<model::Transaction> = db.get_all().unwrap();
        assert_eq!(transactions[0].description, "LUCKY’S");
        assert_eq!(transactions[1].description, "Café");
        assert_eq!(transactions[2].description, "Caffè");
        assert_eq!(transactions[3].description, "François");
    }

    #[test]
    fn populate_account_bank_new_account() {
        let csv = format!("{CSV_HEADER}\n{OUTGOING_TRANSFER}");
        let db = open_stingy_testing_database();
        import(
            &db,
            &mut [("csv", csv.as_bytes())],
            ImportFormat::Revolut {
                account: "0",
                product: "Current",
            },
        )
        .unwrap();
        let accounts: Vec<model::Account> = db.get_all().unwrap();
        assert_eq!(accounts.len(), 1);
        assert_eq!(accounts.get(0).unwrap().bank, Some("Revolut".to_string()));
    }

    #[test]
    fn populate_account_bank_existing_account() {
        let csv = format!("{CSV_HEADER}\n{OUTGOING_TRANSFER}");
        let db = open_stingy_testing_database();
        db.insert_test_data();
        import(
            &db,
            &mut [("csv", csv.as_bytes())],
            ImportFormat::Revolut {
                account: "333333 - 33333333",
                product: "Current",
            },
        )
        .unwrap();
        let accounts: Vec<model::Account> = db.get_all().unwrap();
        let account: &model::Account = accounts
            .iter()
            .filter(|a: &&model::Account| a.name == "333333 - 33333333")
            .collect::<Vec<&model::Account>>()
            .get(0)
            .unwrap();
        assert_eq!(account.bank, Some("Revolut".to_string()));
    }
}
