use crate::database::model;
use crate::output::Output;
use anyhow::{anyhow, bail, Result};
use chrono;
use chrono::Datelike;
use chrono::NaiveDate;
use clap::{error::ErrorKind, CommandFactory, ValueEnum};
use clap::{Parser, Subcommand};
use regex::Regex;
use std::env;
use std::ffi::OsStr;
use std::fmt::{Display, Error, Formatter};
use std::fs;
use std::io;
#[cfg(not(test))]
use std::io::Read;
use std::io::Write;
use std::path::Path;
use std::process::ExitCode;
use std::str::FromStr;

mod commands;
mod database;
mod fallible_print;
mod output;

const TIP: &str = "💡";
const OK: &str = "✅";
const ERR: &str = "❗";
const WARN: &str = "⚠️ ";
#[cfg(debug_assertions)]
const DEBUG: &str = "🛠️ ";

#[derive(Debug, Parser)]
#[command(infer_subcommands = true)]
struct Stingy {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// List accounts, create aliases for them, and select one as the default.
    Accounts {
        #[command(subcommand)]
        accounts: AccountOperation,
    },

    /// Manage rules used for automatically tagging transactions.
    Tags {
        #[command(subcommand)]
        tags: TagOperation,
    },

    /// View transaction data, aggregated and filtered in different ways.
    Query {
        /// The name of the query to run.
        #[command(subcommand)]
        query: PreparedQuery,

        /// Only consider transactions in this time period. Examples: 'january', '2021/01-2022-01', ':-march'.
        #[arg(short, long, global = true)]
        period: Option<String>,

        /// Only consider transactions with these tags.
        #[arg(short, long, use_value_delimiter = true, global = true)]
        tags: Vec<String>,

        /// Exclude transactions with these tags.
        #[arg(short, long, use_value_delimiter = true, global = true)]
        not_tags: Vec<String>,

        /// Only consider transactions whose description (partially) matches this value.
        #[arg(short, long, global = true)]
        description_contains: Option<String>,

        /// Only consider transactions whose amount is in this range. Examples: '10-1000', '50-:'.
        #[arg(long, global = true)]
        amount_range: Option<String>,

        /// Only consider transactions for this account.
        #[arg(long, global = true)]
        account: Option<String>,
    },

    /// Import transactions from a bank.
    Import {
        #[command(subcommand)]
        import: ImportOperations,
    },

    /// Undo the last invocation that wrote to the database.
    Undo {},

    /// List configuration paths and other information.
    Info {},

    /// Remove all data.
    Reset {},
}

#[derive(Debug, Subcommand)]
enum AccountOperation {
    /// List imported accounts.
    List,
    /// Select an account as the default in all queries.
    Select { account: String },
    /// Unselect a default account, or all if none are passed.
    Unselect { account: Option<String> },
    /// Create an alias for an account name. The alias can be used in any command that accepts an
    /// account name.
    Alias {
        /// The account name.
        #[arg(long)]
        account: String,

        /// The alias for that account.
        #[arg(long)]
        alias: String,
    },
    /// Delete an alias for an account.
    #[command(alias = "remove-alias")]
    DeleteAlias { alias: String },
}

#[derive(Debug, Subcommand)]
enum TagOperation {
    /// List the tag rules.
    ListRules {
        /// The tag associated with this rule.
        #[arg(short, long)]
        tag: Option<String>,
    },
    /// Add a tag rule for automatically tagging transactions.
    AddRule {
        /// The tag associated with this rule.
        #[arg(short, long)]
        tag: String,

        /// Match only the transaction with this ID. This is usually not needed.
        #[arg(long)]
        transaction_id: Option<usize>,

        /// Match only transactions whose description contains this text
        #[arg(short, long)]
        description_contains: Option<String>,

        /// Match only transactions of this type.
        #[arg(long)]
        transaction_type: Option<TransactionType>,

        /// Match only transactions whose amount is in this range. Examples: '10-1000', '50-:'.
        #[arg(long)]
        amount_range: Option<String>,

        /// Match only transactions in this period. Examples: 'january', '2021/01-2022/01',
        /// ':-march'.
        #[arg(short, long)]
        period: Option<String>,
    },
    /// Delete a tag rule, removing its tag from all transactions.
    #[command(alias = "remove-rule")]
    DeleteRule { id: String },
}

#[allow(non_camel_case_types)]
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
pub enum TransactionType {
    debit,
    credit,
}

impl Display for TransactionType {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        write!(f, "{:?}", self)
    }
}

#[derive(Debug, Subcommand)]
pub enum PreparedQuery {
    /// A detailed view of debit transactions.
    Debits {
        /// Also display the transaction IDs in the results.
        #[arg(long, global = true)]
        show_transaction_id: bool,
    },
    /// A detailed view of credit transactions.
    Credits {
        /// Also display the transaction IDs in the results.
        #[arg(long, global = true)]
        show_transaction_id: bool,
    },
    /// A summary of expenses, grouped by month.
    ByMonth {
        /// Show the results as a table instead of the default chart.
        #[arg(long, global = true)]
        table: bool,
    },
    /// A summary of expenses, grouped by tag.
    ByTag {
        /// Only consider transactions of this type.
        #[arg(long)]
        transaction_type: Option<TransactionType>,

        /// Show the results as a table instead of the default chart.
        #[arg(long, global = true)]
        table: bool,
    },
}

#[derive(Debug, Subcommand)]
enum ImportOperations {
    /// Import from AIB (aib.ie)
    AIB {
        /// The csv file(s) to use.
        #[arg(long, num_args = 1.., required = true)]
        csv: Vec<String>,
    },

    /// Import from Revolut (revolut.com)
    Revolut {
        /// The csv file(s) to use.
        #[arg(long, num_args = 1.., required = true)]
        csv: Vec<String>,

        /// Import into this account, mandatory for Revolut.
        #[arg(long)]
        account: String,

        /// Import this product, mandatory for Revolut.
        #[arg(long)]
        product: String,
    },
}

fn main() -> ExitCode {
    match stingy_main() {
        Err(e) => {
            if let Some(io_error) = e.downcast_ref::<io::Error>() {
                if io_error.kind() == io::ErrorKind::BrokenPipe {
                    return ExitCode::SUCCESS;
                }
            } else if let Some(clap_error) = e.downcast_ref::<clap::error::Error>() {
                clap_error.exit();
            }
            let _ = eprintln!("Error: {e}");
            return ExitCode::FAILURE;
        }
        Ok(_) => {
            return ExitCode::SUCCESS;
        }
    }
}

fn stingy_main() -> Result<()> {
    #[cfg(debug_assertions)]
    println!("{DEBUG} This is a debug build!")?;

    let db = database::open_stingy_database()?;
    let has_transactions = db.count_transactions()? > 0;

    // https://stackoverflow.com/a/36848555
    let binary_name = env::args()
        .nth(0)
        .as_ref()
        .map(Path::new)
        .and_then(Path::file_name)
        .and_then(OsStr::to_str)
        .map(String::from)
        .unwrap_or(String::from("stingy"));

    let cli = Stingy::parse();
    let mut cmd = Stingy::command();
    let can_run_on_empty_database = match &cli.command {
        Some(Commands::Import { .. }) | Some(Commands::Reset {}) | Some(Commands::Info {}) => true,
        _ => false,
    };
    // Don't generate an undo step for the undo command itself.
    match &cli.command {
        Some(Commands::Undo { .. }) => {}
        _ => {
            let mut invocation = vec![binary_name.clone()];
            invocation.extend(env::args().skip(1));
            commands::undo::begin_undo_step(&db, &invocation.join(" "))?;
        }
    }
    match &cli.command {
        _ if !has_transactions && !can_run_on_empty_database => {
            println!(
                "{TIP} Empty database detected. Run '{} help import' to learn how to populate it.",
                binary_name
            )
        }
        // On empty invocation, default to ByMonth for the current year.
        None => {
            let accounts = commands::accounts::get_account_or_selected(&db, None)?;
            let accounts_names: Vec<&str> = accounts
                .iter()
                .map(|account| account.name.as_str())
                .collect();
            let (january, today) =
                parse_period(Some("jan-:")).map_err(|e| cmd.error(ErrorKind::InvalidValue, e))?;
            commands::query::command_query(
                &db,
                &mut io::stdout(),
                &PreparedQuery::ByMonth { table: false },
                &Vec::new(), // tags
                &Vec::new(), // not_tags
                None,        // description_contains
                None,        // amount_min
                None,        // amount_max
                january,     // from
                today,       // to
                accounts_names,
            )
            .map(|_| ())
        }
        Some(Commands::Import { import }) => {
            let (format, paths) = match &import {
                ImportOperations::AIB { csv } => (commands::import::ImportFormat::AIB, csv),
                ImportOperations::Revolut {
                    csv,
                    account,
                    product,
                } => (
                    commands::import::ImportFormat::Revolut {
                        account: account,
                        product: product,
                    },
                    csv,
                ),
            };
            let mut readers = Vec::new();
            for path in paths {
                let reader = fs::File::open(fs::canonicalize(path)?)?;
                readers.push((path.as_str(), reader));
            }
            match commands::import::import(&db, &mut readers, format) {
                Ok(commands::import::ImportResult {
                    accounts,
                    imported,
                    before,
                    after,
                }) => {
                    println!(
                        "{OK} {} new transactions in {} account(s) imported from {} file(s).",
                        imported,
                        accounts.len(),
                        readers.len()
                    )?;
                    let selected_accounts = commands::accounts::get_account_or_selected(&db, None)?;
                    if selected_accounts.is_empty() && accounts.len() > 0 {
                        println!(
                            "{TIP} No account is currently selected as the default.  Use '{binary_name} help accounts' view account options."
                        )?;
                    }
                    if let (Some(before), Some(after)) = (before, after) {
                        let gap_days = (after - before).num_days().abs();
                        if gap_days > 3 {
                            println!("{WARN} There is a gap of {gap_days} days between the old and the newly imported data ({before} to {after}).")
                        } else {
                            Ok(())
                        }
                    } else {
                        println!(
                            "{TIP} Try '{} help query' to learn different ways of querying transactions.",
                            binary_name
                        )
                    }
                }
                Err(err) => {
                    println!("{ERR} Failed to import: {err}")?;
                    println!("{TIP} Check the documentation a step-by-step guide on importing.")
                }
            }
        }
        Some(Commands::Accounts {
            accounts: AccountOperation::List,
        }) => {
            let result = commands::accounts::list(&db)?;
            let mut to = output::table::TableOutput::new(io::stdout(), None);
            to.render_table(&result.columns, &result.rows).map(|_| ())
        }
        Some(Commands::Accounts {
            accounts: AccountOperation::Select { account },
        }) => match commands::accounts::select(&db, account) {
            Ok(_) => println!("Account '{account}' selected."),
            Err(err) => {
                println!("{ERR} Failed to select account: {err}")?;
                println!("{TIP} Use '{binary_name} accounts list' to view available options.")
            }
        },
        Some(Commands::Accounts {
            accounts: AccountOperation::Unselect { account },
        }) => {
            commands::accounts::unselect(&db, account.as_deref())?;
            println!("Done.")
        }
        Some(Commands::Accounts {
            accounts: AccountOperation::Alias { account, alias },
        }) => {
            match commands::accounts::alias(&db, account, alias) {
                Ok(account) => println!("Set '{alias}' to refer to '{name}'.", name = account.name),
                Err(err) => {
                    println!("{ERR} Failed to set alias: {err}")?;
                    println!("{TIP} Use '{binary_name} accounts list' to view accounts and their aliases.")
                }
            }
        }
        Some(Commands::Accounts {
            accounts: AccountOperation::DeleteAlias { alias },
        }) => {
            match commands::accounts::delete_alias(&db, alias) {
                Ok(_) => println!("Deleted alias '{alias}'"),
                Err(err) => {
                    println!("{ERR} Failed to delete alias: {err}")?;
                    println!("{TIP} Use '{binary_name} accounts list' to view accounts and their aliases.")
                }
            }
        }
        Some(Commands::Tags {
            tags: TagOperation::ListRules { tag },
        }) => {
            let result = commands::tags::list_tag_rules(&db, tag.as_deref())?;
            let mut to = output::table::TableOutput::new(io::stdout(), None);
            to.render_table(&result.columns, &result.rows).map(|_| ())
        }
        Some(Commands::Tags {
            tags:
                TagOperation::AddRule {
                    tag,
                    transaction_id,
                    description_contains,
                    transaction_type,
                    amount_range,
                    period,
                },
        }) => {
            let parameters = (
                transaction_id,
                description_contains.as_deref(),
                amount_range.as_deref(),
                period.as_deref(),
                parse_amount_range(amount_range.as_deref()),
                parse_period(period.as_deref()),
            );
            let transaction_type = transaction_type.as_ref().map(|tt| {
                if *tt == crate::TransactionType::credit {
                    model::TransactionType::Credit
                } else {
                    model::TransactionType::Debit
                }
            });
            match parameters {
                (None, None, None, None, _, _) => {
                    bail!(cmd.error(
                        ErrorKind::MissingRequiredArgument,
                        "At least one parameter must be passed.\n\n{TIP} Use {binary_name} help tags add-rule to view available options."
                    ));
                }
                (_, _, Some(_), _, Err(e), _) => {
                    bail!(cmd.error(
                        ErrorKind::InvalidValue,
                        format!("Invalid format for --amount-range: {}", e)
                    ));
                }
                (_, _, _, Some(_), _, Err(e)) => {
                    bail!(cmd.error(
                        ErrorKind::InvalidValue,
                        format!("Invalid format for --period: {}", e)
                    ));
                }
                (_, _, _, _, Ok((amount_min, amount_max)), Ok((from, to))) => {
                    let result = commands::tags::add_tag_rule(
                        &db,
                        tag,
                        *transaction_id,
                        description_contains.as_deref(),
                        transaction_type.clone(),
                        amount_min,
                        amount_max,
                        from,
                        to,
                    )?;
                    match result {
                        commands::tags::AddTagRuleResult::Added {
                            tag_rule_id,
                            tagged_transactions,
                        } => {
                            println!("{OK} Added tag rule {tag_rule_id}, tagging {tagged_transactions} transaction(s) with tag '{tag}'.")
                        }
                        commands::tags::AddTagRuleResult::NotUnique { tag_rule_id } => {
                            println!(
                                "Tag rule {tag_rule_id} already matches these parameters, ignoring.\n\n{TIP} Use {binary_name} tags list to view tag rules."
                            )
                        }
                    }
                }
                (_, _, _, _, _, _) => {
                    unreachable!("This shouldn't happen.");
                }
            }?;
            if transaction_id.is_some() {
                println!("{TIP} Tag rules using transaction IDs override all other tag rules.")
            } else {
                Ok(())
            }
        }
        Some(Commands::Tags {
            tags: TagOperation::DeleteRule { id },
        }) => {
            let id_i64 = id
                .parse::<i64>()
                .map_err(|_| anyhow!("invalid rule ID (must be a number)"))?;
            let tag_rules: Vec<model::TagRule> = db.get_all()?;
            let tag_rule = tag_rules.iter().filter(|tr| tr.id == Some(id_i64)).next();
            match tag_rule {
                None => {
                    bail!(cmd.error(
                        ErrorKind::InvalidValue,
                        format!("Rule {id} not found.\n\n{TIP} Use {binary_name} tags list-rules to see existing rules.")
                    ));
                }
                Some(tr) => {
                    let prompt = format!("{WARN} Delete rule {id} ({0})", tr.human_readable);
                    with_confirmation(&prompt, || {
                        let deleted = commands::tags::delete_tag_rule(&db, id)?;
                        if deleted != 1 {
                            unreachable!("This shouldn't happen.");
                        }
                        println!("{OK} Tag rule {id} deleted.")
                    })
                }
            }
        }
        Some(Commands::Query {
            query,
            period,
            tags,
            not_tags,
            description_contains,
            amount_range,
            account,
        }) => {
            let (from, to) = parse_period(period.as_deref())
                .map_err(|e| cmd.error(ErrorKind::InvalidValue, e))?;
            let (amount_min, amount_max) = {
                parse_amount_range(amount_range.as_deref()).or_else(|e| {
                    bail!(cmd.error(
                        ErrorKind::InvalidValue,
                        format!("Invalid format for --amount-range: {}", e)
                    ));
                })?
            };
            let accounts = commands::accounts::get_account_or_selected(&db, account.as_deref())?;
            let account_names: Vec<&str> = accounts
                .iter()
                .map(|account| account.name.as_str())
                .collect();
            commands::query::command_query(
                &db,
                &mut io::stdout(),
                query,
                tags,
                not_tags,
                description_contains.as_deref(),
                amount_min,
                amount_max,
                from,
                to,
                account_names,
            )?;
            match query {
                PreparedQuery::ByMonth { table: false }
                | PreparedQuery::ByTag { table: false, .. } => {
                    println!("{TIP} Add --table to this command to view details in table format.")
                }
                _ => Ok(()),
            }
        }
        Some(Commands::Undo {}) => commands::undo::command_undo(&db),
        Some(Commands::Info {}) => {
            let info = commands::info::command_info(&db)?;
            println!("Database URI: {}", info.database_uri)?;
            println!("Git SHA: {}", info.git_sha)
        }
        Some(Commands::Reset {}) => {
            let prompt =
                format!("{WARN} This will delete the Stingy database, ALL DATA WILL BE LOST!");
            with_confirmation(&prompt, || commands::reset::command_reset(db))
        }
    }
}

fn parse_period(period_option: Option<&str>) -> Result<(Option<NaiveDate>, Option<NaiveDate>)> {
    if let Some(period) = period_option {
        if let Ok((m, y)) = parse_month(period) {
            Ok((
                Some(first_day_of_month(y, m)?),
                Some(last_day_of_month(y, m)?),
            ))
        } else {
            parse_date_range(period)
        }
    } else {
        Ok((None, None))
    }
}

fn parse_amount_range(amount_range: Option<&str>) -> Result<(Option<f64>, Option<f64>)> {
    if let Some(range_string) = amount_range {
        parse_range(range_string, |(i, part)| {
            if part == ":" {
                if i == 0 {
                    Ok(Some(f64::MIN))
                } else {
                    Ok(Some(f64::MAX))
                }
            } else {
                match part.parse() {
                    Ok(value) => Ok(Some(value)),
                    Err(_) => Err(anyhow!("failed to parse range value")),
                }
            }
        })
    } else {
        Ok((None, None))
    }
}

fn parse_date_range(date_range: &str) -> Result<(Option<NaiveDate>, Option<NaiveDate>)> {
    parse_range(date_range, |(i, part)| {
        if part == ":" {
            if i == 0 {
                Ok(Some(NaiveDate::MIN))
            } else {
                Ok(Some(NaiveDate::MAX))
            }
        } else {
            NaiveDate::parse_from_str(part, "%Y/%m/%d")
                .map(|naive_date| Some(naive_date))
                .or_else(|_| {
                    let (m, y) = parse_month(part)?;
                    let result = if i == 0 {
                        first_day_of_month(y, m)?
                    } else {
                        last_day_of_month(y, m)?
                    };
                    Ok(Some(result))
                })
        }
    })
}

fn parse_range<T, F>(range_string: &str, parse_part: F) -> Result<(Option<T>, Option<T>)>
where
    T: Copy + Clone + PartialOrd,
    F: Fn((usize, &str)) -> Result<Option<T>>,
{
    let lo_and_hi: Vec<_> = range_string
        .splitn(2, "-")
        .enumerate()
        .map(parse_part)
        .collect();
    if lo_and_hi.len() != 2 {
        bail!("could not split provided range string (must contain one '-')");
    }
    match (&lo_and_hi[0], &lo_and_hi[1]) {
        (Err(_), _) | (_, Err(_)) => {
            bail!("invalid value(s) in range.\n\n{TIP} Tip: use ':' to mean infinity, as in ':-max' or 'min-:'.");
        }
        (Ok(Some(lo)), Ok(Some(hi))) if lo > hi => {
            bail!("range values are in the wrong order!");
        }
        (Ok(optional_lo), Ok(optional_hi)) => Ok((*optional_lo, *optional_hi)),
    }
}

#[cfg(not(test))]
fn now() -> chrono::DateTime<chrono::Local> {
    chrono::Local::now()
}

#[cfg(test)]
fn now() -> chrono::DateTime<chrono::Local> {
    return NaiveDate::from_ymd_opt(2021, 03, 01)
        .unwrap()
        .and_hms_opt(0, 0, 0)
        .unwrap()
        .and_local_timezone(chrono::Local)
        .unwrap();
}

fn parse_month(month: &str) -> Result<(u32, i32)> {
    let now = now();
    let month_re = Regex::new(r"^((?P<year>\d{4})[-/])?(?P<month>(\d{2}|[A-Za-z]+))$")?;
    let capture = month_re
        .captures(&month)
        .ok_or(anyhow!("invalid month format: try YYYY/MM."))?;

    let year = match capture.name("year") {
        Some(ys) => ys
            .as_str()
            .parse::<i32>()
            .map_err(|_| anyhow!("invalid year '{}'", ys.as_str())),
        None => Ok(now.year()),
    }?;
    let month = capture["month"].parse::<u32>().or_else(|_| {
        // Textual month, no year.
        match chrono::Month::from_str(&capture["month"]) {
            Ok(m) => Ok(m.number_from_month()),
            Err(_) => Err(anyhow!("invalid month '{}'", &capture["month"])),
        }
    })?;
    if month == 0 || month > 12 {
        Err(anyhow!("month {} is not in range 1-12!", month))
    } else {
        Ok((month, year))
    }
}

fn first_day_of_month(year: i32, month: u32) -> Result<NaiveDate> {
    NaiveDate::from_ymd_opt(year, month, 1)
        .ok_or_else(|| anyhow!("failed to compute the first day of the month. This is a bug."))
}

fn last_day_of_month(year: i32, month: u32) -> Result<NaiveDate> {
    let err = "failed to compute the last day of the month. This is a bug.";
    NaiveDate::from_ymd_opt(year, month + 1, 1)
        .unwrap_or(NaiveDate::from_ymd_opt(year + 1, 1, 1).ok_or_else(|| anyhow!(err))?)
        .pred_opt()
        .ok_or_else(|| anyhow!(err))
}

#[cfg(not(test))]
fn confirm(prompt: &str) -> Result<bool> {
    print!("{prompt} Proceed? [y/N] ")?;
    io::stdout().flush()?;
    let mut yn = [0; 1];
    io::stdin().read(&mut yn)?;
    let yn = yn[0] as char;
    Ok(yn == 'y' || yn == 'Y')
}

#[cfg(not(test))]
fn with_confirmation<F>(prompt: &str, action: F) -> Result<()>
where
    F: FnOnce() -> Result<()>,
{
    if confirm(prompt)? {
        action()?;
        println!("Done.")?;
    } else {
        println!("Canceled.")?;
    }
    Ok(())
}

#[cfg(test)]
fn with_confirmation<F>(prompt: &str, action: F) -> Result<()>
where
    F: FnOnce() -> Result<()>,
{
    let _ = prompt;
    action()
}

#[cfg(test)]
mod parse_period_tests {
    use super::*;

    #[test]
    fn mock_now() {
        let mock_now = now();
        assert_eq!(mock_now.year(), 2021);
    }

    #[test]
    fn parse_period_none() {
        assert_eq!(parse_period(None).unwrap(), (None, None));
    }

    #[test]
    fn parse_period_with_dates() {
        assert_eq!(
            parse_period(Some("2023/02/01-2023/03/04")).unwrap(),
            (
                Some(NaiveDate::from_ymd_opt(2023, 02, 01).unwrap()),
                Some(NaiveDate::from_ymd_opt(2023, 03, 04).unwrap())
            )
        );
        assert_eq!(
            parse_period(Some("2023/02/01-2023/02/01")).unwrap(),
            (
                Some(NaiveDate::from_ymd_opt(2023, 02, 01).unwrap()),
                Some(NaiveDate::from_ymd_opt(2023, 02, 01).unwrap())
            )
        );
    }

    #[test]
    fn parse_period_with_months() {
        assert_eq!(
            parse_period(Some("february-march")).unwrap(),
            (
                Some(NaiveDate::from_ymd_opt(2021, 02, 01).unwrap()),
                Some(NaiveDate::from_ymd_opt(2021, 03, 31).unwrap())
            )
        );
        assert_eq!(
            parse_period(Some("march-march")).unwrap(),
            (
                Some(NaiveDate::from_ymd_opt(2021, 03, 01).unwrap()),
                Some(NaiveDate::from_ymd_opt(2021, 03, 31).unwrap())
            )
        );
    }

    #[test]
    fn parse_period_with_mixed_date_and_month() {
        assert_eq!(
            parse_period(Some("2021/02/15-march")).unwrap(),
            (
                Some(NaiveDate::from_ymd_opt(2021, 02, 15).unwrap()),
                Some(NaiveDate::from_ymd_opt(2021, 03, 31).unwrap())
            )
        );
    }

    #[test]
    fn parse_period_with_year_month() {
        assert_eq!(
            parse_period(Some("2023/02-2023/03")).unwrap(),
            (
                Some(NaiveDate::from_ymd_opt(2023, 02, 1).unwrap()),
                Some(NaiveDate::from_ymd_opt(2023, 03, 31).unwrap())
            )
        );
    }

    #[test]
    fn parse_period_single_arguments() {
        assert_eq!(
            parse_period(Some("2023/02")).unwrap(),
            (
                Some(NaiveDate::from_ymd_opt(2023, 02, 1).unwrap()),
                Some(NaiveDate::from_ymd_opt(2023, 02, 28).unwrap())
            )
        );
        assert_eq!(
            parse_period(Some("02")).unwrap(),
            (
                Some(NaiveDate::from_ymd_opt(2021, 02, 1).unwrap()),
                Some(NaiveDate::from_ymd_opt(2021, 02, 28).unwrap())
            )
        );
        assert_eq!(
            parse_period(Some("february")).unwrap(),
            (
                Some(NaiveDate::from_ymd_opt(2021, 02, 1).unwrap()),
                Some(NaiveDate::from_ymd_opt(2021, 02, 28).unwrap())
            )
        );
    }

    #[test]
    fn parse_period_wrong_order() {
        assert!(parse_period(Some("01/2023-01/2022")).is_err());
    }

    #[test]
    fn parse_period_infinity() {
        assert_eq!(
            parse_period(Some(":-:")).unwrap(),
            (Some(NaiveDate::MIN), Some(NaiveDate::MAX))
        );

        assert_eq!(
            parse_period(Some(":-march")).unwrap(),
            (
                Some(NaiveDate::MIN),
                Some(NaiveDate::from_ymd_opt(2021, 03, 31).unwrap())
            )
        );

        assert_eq!(
            parse_period(Some("march-:")).unwrap(),
            (
                Some(NaiveDate::from_ymd_opt(2021, 03, 01).unwrap()),
                Some(NaiveDate::MAX)
            )
        );
    }
}

#[cfg(test)]
mod parse_amount_range_tests {
    use super::*;

    #[test]
    fn parse_amount_range_none() {
        assert_eq!(parse_amount_range(None).unwrap(), (None, None));
    }

    #[test]
    fn parse_amount_range_with_ints() {
        assert_eq!(
            parse_amount_range(Some("50-100")).unwrap(),
            (Some(50.into()), Some(100.into()))
        );
        assert_eq!(
            parse_amount_range(Some("50-50")).unwrap(),
            (Some(50.into()), Some(50.into()))
        );
    }

    #[test]
    fn parse_amount_range_with_floats() {
        assert_eq!(
            parse_amount_range(Some("50.5-100.5")).unwrap(),
            (Some(50.5), Some(100.5))
        );
        assert_eq!(
            parse_amount_range(Some("50.5-50.5")).unwrap(),
            (Some(50.5), Some(50.5))
        );
    }

    #[test]
    fn parse_amount_range_with_mixed_int_and_float() {
        assert_eq!(
            parse_amount_range(Some("50-65.5")).unwrap(),
            (Some(50.into()), Some(65.5))
        );
    }

    #[test]
    fn parse_amount_range_single_arguments() {
        assert!(parse_amount_range(Some("50")).is_err());
    }

    #[test]
    fn parse_amount_range_wrong_order() {
        assert!(parse_amount_range(Some("100-50")).is_err());
    }

    #[test]
    fn parse_amount_range_infinity() {
        assert_eq!(
            parse_amount_range(Some(":-:")).unwrap(),
            (Some(f64::MIN), Some(f64::MAX))
        );

        assert_eq!(
            parse_amount_range(Some(":-50")).unwrap(),
            (Some(f64::MIN), Some(50.into()))
        );

        assert_eq!(
            parse_amount_range(Some("50-:")).unwrap(),
            (Some(50.into()), Some(f64::MAX))
        );
    }
}
