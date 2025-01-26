use anyhow::{anyhow, bail, Result};
use chrono;
use sqlite;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use crate::database::*;

fn sql<T: sqlite::Bindable>(
    conn: &sqlite::Connection,
    sql: &str,
    args: T,
) -> Result<Vec<Vec<sqlite::Value>>> {
    let mut stmt = conn.prepare(sql)?;
    stmt.bind(args)?;
    let mut rows = Vec::new();
    let mut cursor = stmt.iter();
    while let Some(row) = cursor.try_next()? {
        rows.push(row.to_vec());
    }
    Ok(rows)
}

// Run an SQL query with variadic arguments.
macro_rules! sqlv {
    ($conn:expr, $sql:expr, $($args:expr),*) => ({
        let mut args: Vec<sqlite::Value> = Vec::new();
        $(
            args.push($args.into());
        )*
        self::sql($conn, $sql, args.as_slice())
    });
    ($conn:expr, $sql:expr) => ({
        self::sql($conn, $sql, Vec::<sqlite::Value>::new().as_slice())
    });
}

struct SuspendForeignKeys<'a> {
    conn: &'a sqlite::Connection,
}

impl SuspendForeignKeys<'_> {
    fn new(conn: &sqlite::Connection) -> SuspendForeignKeys {
        conn.execute("PRAGMA foreign_keys = OFF;").unwrap();
        SuspendForeignKeys { conn: conn }
    }
}

impl Drop for SuspendForeignKeys<'_> {
    fn drop(&mut self) {
        self.conn.execute("PRAGMA foreign_keys = ON;").unwrap();
    }
}

#[cfg(not(debug_assertions))]
fn stingy_data_dir() -> Result<PathBuf> {
    dirs::data_dir()
        .ok_or(anyhow!("can't determine data dir"))
        .and_then(|d| Ok(d.join("Stingy")))
}

#[cfg(debug_assertions)]
fn stingy_data_dir() -> Result<PathBuf> {
    use std::env;
    Ok(env::temp_dir().join("Stingy"))
}

const MIGRATIONS: &[(&str, &str)] = &[
    ("", ""), // Sentinel
    (
        "001-initial-schema.sql",
        include_str!("./sql/migrations/001-initial-schema.sql"),
    ),
    (
        "002-undo.sql",
        include_str!("./sql/migrations/002-undo.sql"),
    ),
];

fn perform_migrations(conn: &sqlite::Connection, migrations: &[(&str, &str)]) -> Result<bool> {
    let rows = sqlv!(conn, "PRAGMA user_version")?;
    // This always returns a row, user_version is 0 on an empty database.
    let prev_version: i64 = (&rows[0][0]).try_into()?;
    let mut version: usize = prev_version as usize + 1;
    while version < migrations.len() {
        let migration_name = migrations[version].0;
        // We could switch to SAVEPOINT if we need to use nested transactions
        // inside the migration scripts.
        // See https://www.sqlite.org/lang_savepoint.html.
        conn.execute("BEGIN TRANSACTION")
            .map_err(|e| anyhow!("couldn't start transaction: {e}"))?;
        conn.execute(migrations[version].1).or_else(|e| {
            conn.execute("ROLLBACK")
                .map_err(|e| anyhow!("the migration failed, and we failed to rollback: {e}"))?;
            bail!("migration '{migration_name}' failed ({e})")
        })?;
        // I think we can't use parameters in this query because our sqlite library doesn't bind
        // them on PRAGMA? Version is just a trusted number so it should be fine to format! here.
        conn.execute(format!("PRAGMA user_version = {version}"))
            .or_else(|e| {
                conn.execute("ROLLBACK")
                    .map_err(|e| anyhow!("failed to update version, and to rollback: {e}."))?;
                bail!("failed to update version ({e})")
            })?;
        conn.execute("END TRANSACTION")
            .map_err(|e| anyhow!("couldn't commit transaction: {e}"))?;
        version += 1;
    }
    Ok((prev_version as usize) < migrations.len() - 1)
}

fn initialize_sqlite(conn: &sqlite::Connection) -> Result<bool> {
    // Foreign keys are disabled by default, and need to be enabled per connection.
    // https://www.sqlite.org/foreignkeys.html
    conn.execute("PRAGMA foreign_keys = ON;")
        .map_err(|e| anyhow!(e))?;
    perform_migrations(conn, MIGRATIONS).map_err(|e| anyhow!("failed to apply migration: {e}"))
}

pub struct SQLiteStingyDatabase {
    conn: sqlite::Connection,
    path: PathBuf,
}

impl SQLiteStingyDatabase {
    pub fn new() -> Result<Box<dyn StingyDatabase>> {
        let data_dir = stingy_data_dir()?;
        fs::create_dir_all(&data_dir)?;
        let db_file = data_dir.join("stingy.sqlite");

        // By default, sqlite::open also creates the file if it's not there.
        let conn = sqlite::open(format!("file:{}", db_file.display()))?;
        let schema_updated = initialize_sqlite(&conn)?;
        initialize_undo(&conn, schema_updated)?;
        Ok(Box::new(Self {
            conn: conn,
            path: db_file,
        }))
    }

    #[cfg(test)]
    pub fn new_for_testing() -> Box<dyn StingyDatabase> {
        let conn = sqlite::open(":memory:").unwrap();
        initialize_sqlite(&conn).unwrap();
        initialize_undo(&conn, true).unwrap();
        Box::new(Self {
            conn: conn,
            path: PathBuf::from(":memory:"),
        })
    }
}

impl private::Reset for SQLiteStingyDatabase {
    fn reset(&self) -> Result<()> {
        fs::remove_file(&self.path).map_err(|_| anyhow!("failed to remove database file."))
    }
}

impl StingyDatabase for SQLiteStingyDatabase {
    fn get_uri(&self) -> String {
        let mut uri = "file://".to_string();
        if let Some(p) = self.path.to_str() {
            uri.push_str(p);
        }
        uri
    }

    fn count_transactions(&self) -> Result<usize> {
        let rows = sqlv!(&self.conn, "SELECT COUNT(*) FROM transactions")?;
        let count: i64 = (&rows[0][0]).try_into()?;
        Ok(count as usize)
    }

    fn count_matching_transactions(&self, tag_rule_id: &str) -> Result<usize> {
        let rows = sqlv!(
            &self.conn,
            "SELECT COUNT(*) FROM transactions_tags WHERE tag_rule_id = ?",
            tag_rule_id.to_string()
        )?;
        let count: i64 = (&rows[0][0]).try_into()?;
        Ok(count as usize)
    }

    fn insert_test_data(&self) {
        self.conn
            .execute(include_str!("./sql/test/test_data.sql"))
            .unwrap();
    }
}

/* Macro-implement the triggers for UndoOperations for various models.
 * In this postmodern piece of metaprogramming, we write...
 *   ... a macro, which generates, for each model
 *   ... code, which generates, for each SQL statement that writes
 *   ... a TEMP trigger, which generates, at each write
 *   ... a statement that reverses that write, stored in the undo table.
 * See https://www.sqlite.org/undoredo.html for the inspiration.
 */
macro_rules! impl_undo_operations {
    ($conn:ident, $model_type:ty, $table:ident) => {
        let table = stringify!($table);
        $conn.execute(format!(
            "CREATE TEMP TRIGGER {table}_undo_insert
             AFTER INSERT ON {table}
             BEGIN
                INSERT INTO undo_statements VALUES(
                    (SELECT MAX(id) FROM undo_steps),
                    'DELETE FROM {table} WHERE rowid = ' || new.rowid);
             END;",
        ))?;

        let revert_update_statement = format!(
            "'UPDATE {table} SET '|| {}",
            <$model_type>::FIELD_NAMES_AS_ARRAY
                .iter()
                .map(|field_name| format!("'{field_name} = ' || quote(old.{field_name})"))
                .collect::<Vec<String>>()
                .join("|| ', ' ||")
        );
        $conn.execute(format!(
            "CREATE TEMP TRIGGER {table}_undo_update
             AFTER UPDATE ON {table}
             BEGIN
                INSERT INTO undo_statements VALUES(
                    (SELECT MAX(id) FROM undo_steps),
                    {revert_update_statement});
             END;"
        ))?;

        let reinsert_on_delete_statement = format!(
            "'INSERT INTO {table}({}) VALUES('|| {} || ')'",
            <$model_type>::FIELD_NAMES_AS_ARRAY.join(", "),
            <$model_type>::FIELD_NAMES_AS_ARRAY
                .iter()
                .map(|field_name| format!("quote(old.{field_name})"))
                .collect::<Vec<String>>()
                .join("|| ', ' ||")
        );
        $conn.execute(format!(
            "CREATE TEMP TRIGGER {table}_undo_delete
             BEFORE DELETE ON {table}
             BEGIN
                INSERT INTO undo_statements VALUES(
                    (SELECT MAX(id) FROM undo_steps),
                    {reinsert_on_delete_statement});
             END;"
        ))?;
    };
}

fn initialize_undo(conn: &sqlite::Connection, from_scratch: bool) -> Result<()> {
    if from_scratch {
        conn.execute("DELETE FROM undo_steps")?;
    }

    // Clean up undo steps with no statements (created by read-only commands).
    conn.execute(
        "DELETE FROM undo_steps WHERE id NOT IN
             (SELECT undo_step_id FROM undo_statements)",
    )?;

    impl_undo_operations!(conn, model::Account, accounts);
    impl_undo_operations!(conn, model::Transaction, transactions);
    impl_undo_operations!(conn, model::TagRule, tag_rules);
    Ok(())
}

impl UndoOperations for SQLiteStingyDatabase {
    fn begin_undo_step(&self, undo_step_name: &str, max_undo_steps: usize) -> Result<()> {
        sqlv!(
            &self.conn,
            "INSERT INTO undo_steps VALUES(NULL, ?)",
            undo_step_name.to_string()
        )?;

        // Truncate to the maximum number of steps.
        sqlv!(
            &self.conn,
            "DELETE FROM undo_steps WHERE id <= (
                SELECT MAX(id) FROM undo_steps) - ?",
            max_undo_steps as i64
        )
        .map(|_| ())
    }

    fn undo_last_step(&self) -> Result<()> {
        let _sfk = SuspendForeignKeys::new(&self.conn);
        let rows = sqlv!(
            &self.conn,
            "SELECT undo_step_id, statement
             FROM undo_statements WHERE undo_step_id = (
                SELECT MAX(id) FROM undo_steps);"
        )?;
        if rows.is_empty() {
            bail!("there is nothing to undo.");
        }
        for row in &rows {
            let statement: String = row[1].clone().try_into()?;
            self.conn.execute(&statement)?;
        }
        sqlv!(
            &self.conn,
            "DELETE FROM undo_steps WHERE id = ?",
            rows[0][0].clone()
        )
        .map(|_| ())
    }

    fn get_last_undo_step(&self) -> Result<String> {
        let rows = sqlv!(
            &self.conn,
            "SELECT name FROM undo_steps
             ORDER BY id DESC LIMIT 1;"
        )?;
        if rows.is_empty() {
            bail!("there is nothing to undo.");
        }
        rows[0][0]
            .clone()
            .try_into()
            .map_err(|_| anyhow!("failed to fetch undo step name"))
    }
}

/* Macro-implement the ModelOperations trait for the various models.
 *
 * This makes a few assumptions about the models and the database schema:
 *
 * 1. Each model is represented by a table, with columns named after the model
 *    struct's fields.
 * 2. The first field of the model's struct is its primary key.
 * 3. There is a UNIQUE constraint across all of the other fields.
 */
macro_rules! impl_model_operations {
    ($model_type:ty, $table:ident) => {
        impl ModelOperations<$model_type> for SQLiteStingyDatabase {
            fn get_all(&self) -> Result<Vec<$model_type>> {
                let table = stringify!($table);
                let sql = format!(
                    "SELECT {} FROM {table}",
                    <$model_type>::FIELD_NAMES_AS_ARRAY.join(", ")
                );
                sqlv!(&self.conn, &sql)?
                    .into_iter()
                    .map(|row| row.try_into())
                    .collect()
            }

            fn update(&self, model: &$model_type) -> Result<()> {
                let table = stringify!($table);
                let mut values: Vec<sqlite::Value> = model.into();
                let placeholders: Vec<_> = <$model_type>::FIELD_NAMES_AS_ARRAY
                    .iter()
                    .map(|_| "?")
                    .collect();
                assert_eq!(values.len(), <$model_type>::FIELD_NAMES_AS_ARRAY.len());
                values.push(values[0].clone()); // Repeat the ID for the WHERE clause.
                let sql = format!(
                    "UPDATE {table} SET ({}) = ({}) WHERE {} = ?",
                    <$model_type>::FIELD_NAMES_AS_ARRAY.join(", "),
                    placeholders.join(", "),
                    <$model_type>::FIELD_NAMES_AS_ARRAY[0]
                );
                self::sql(&self.conn, &sql, values.as_slice()).map(|_| ())
            }

            fn insert_or_get(&self, model: $model_type) -> Result<NewOrExisting<$model_type>> {
                let values: Vec<sqlite::Value> = (&model).into();
                let table = stringify!($table);
                let placeholders: Vec<&str> = <$model_type>::FIELD_NAMES_AS_ARRAY
                    .iter()
                    .map(|_| "?")
                    .collect();
                assert_eq!(values.len(), <$model_type>::FIELD_NAMES_AS_ARRAY.len());
                let sql = format!(
                    "INSERT INTO {table}({}) VALUES ({}) RETURNING {}",
                    <$model_type>::FIELD_NAMES_AS_ARRAY.join(", "),
                    placeholders.join(", "),
                    <$model_type>::FIELD_NAMES_AS_ARRAY.join(", ")
                );
                match self::sql(&self.conn, &sql, values.as_slice()) {
                    Ok(mut rows) => Ok(NewOrExisting::New(rows.remove(0).try_into()?)),
                    Err(err) if !err.to_string().starts_with("UNIQUE constraint failed") => {
                        Err(err)
                    }
                    Err(_) => {
                        let sql = format!(
                            "SELECT {} FROM {table} WHERE ({}) IS ({})",
                            <$model_type>::FIELD_NAMES_AS_ARRAY.join(", "),
                            <$model_type>::FIELD_NAMES_AS_ARRAY[1..].join(", "),
                            placeholders[1..].join(", ")
                        );
                        let mut rows = self::sql(&self.conn, &sql, &values[1..])?;
                        if rows.len() == 0 {
                            bail!("values are not unique, but can't find the duplicate.");
                        }
                        Ok(NewOrExisting::Existing(rows.remove(0).try_into()?))
                    }
                }
            }

            fn delete(&self, model: $model_type) -> Result<usize> {
                let table = stringify!($table);
                let mut values: Vec<sqlite::Value> = (&model).into();
                sqlv!(
                    &self.conn,
                    &format!(
                        "DELETE FROM {table} WHERE {} = ?",
                        <$model_type>::FIELD_NAMES_AS_ARRAY[0]
                    ),
                    values.remove(0)
                )?;
                Ok(self.conn.change_count())
            }
        }
    };
}

impl TryFrom<sqlite::Value> for model::TransactionType {
    type Error = anyhow::Error;

    fn try_from(value: sqlite::Value) -> Result<Self> {
        Ok(match (&value).try_into()? {
            "Debit" => model::TransactionType::Debit,
            "Credit" => model::TransactionType::Credit,
            "Direct Debit" => model::TransactionType::DirectDebit,
            &_ => bail!("unexpected transaction type, this is a bug"),
        })
    }
}

impl From<&model::TransactionType> for sqlite::Value {
    fn from(model: &model::TransactionType) -> Self {
        match model {
            model::TransactionType::Debit => "Debit".into(),
            model::TransactionType::Credit => "Credit".into(),
            model::TransactionType::DirectDebit => "Direct Debit".into(),
        }
    }
}

fn from_naive_date_to_sqlite_value(naive_date: &chrono::NaiveDate) -> sqlite::Value {
    match naive_date {
        &chrono::NaiveDate::MAX => "Inf".into(),
        &chrono::NaiveDate::MIN => "-Inf".into(),
        _ => format!("{}", naive_date.format("%Y-%m-%d")).into(),
    }
}

fn try_from_sqlite_value_to_naive_date(value: sqlite::Value) -> Result<chrono::NaiveDate> {
    let date_from_sqlite: String = value.try_into()?;
    chrono::NaiveDate::parse_from_str(&date_from_sqlite, "%Y-%m-%d")
        .map_err(|_| anyhow!("couldn't parse transaction date"))
}

fn try_from_sqlite_value_to_naive_date_opt(
    value: sqlite::Value,
) -> Result<Option<chrono::NaiveDate>> {
    if value == sqlite::Value::Null {
        return Ok(None);
    }
    try_from_sqlite_value_to_naive_date(value).map(|nd| Some(nd))
}

// A SQLite bug (yes, really) sometimes causes REAL columns to be
// read as INTEGER: https://sqlite.org/forum/forumpost/e0c7574ab2.
macro_rules! as_float {
    ($v:expr) => {{
        let v = $v;
        if let sqlite::Value::Integer(n) = v {
            sqlite::Value::Float(n as f64)
        } else {
            v
        }
    }};
}

impl TryFrom<Vec<sqlite::Value>> for model::Transaction {
    type Error = anyhow::Error;

    fn try_from(mut values: Vec<sqlite::Value>) -> Result<Self> {
        assert_eq!(values.len(), Self::FIELD_NAMES_AS_ARRAY.len());
        Ok(Self {
            id: (&values.remove(0)).try_into()?,
            account_name: values.remove(0).try_into()?,
            posted_date: try_from_sqlite_value_to_naive_date(values.remove(0))?,
            description: values.remove(0).try_into()?,
            debit_amount: (&as_float!(values.remove(0))).try_into()?,
            credit_amount: (&as_float!(values.remove(0))).try_into()?,
            balance: (&as_float!(values.remove(0))).try_into()?,
            transaction_type: values.remove(0).try_into()?,
            currency: values.remove(0).try_into()?,
        })
    }
}

impl From<&model::Transaction> for Vec<sqlite::Value> {
    fn from(model: &model::Transaction) -> Self {
        // We use a match statement to force a build error if the struct
        // fields change.
        match model {
            model::Transaction {
                id,
                account_name,
                posted_date,
                description,
                debit_amount,
                credit_amount,
                balance,
                transaction_type,
                currency,
            } => vec![
                (*id).map(|v| v.into()).unwrap_or(sqlite::Value::Null),
                account_name.as_str().into(),
                from_naive_date_to_sqlite_value(posted_date),
                description.as_str().into(),
                (*debit_amount).into(),
                (*credit_amount).into(),
                (*balance).into(),
                transaction_type.into(),
                currency.as_str().into(),
            ],
        }
    }
}

impl_model_operations!(model::Transaction, transactions);

impl TryFrom<Vec<sqlite::Value>> for model::Account {
    type Error = anyhow::Error;

    fn try_from(mut values: Vec<sqlite::Value>) -> Result<Self> {
        assert_eq!(values.len(), Self::FIELD_NAMES_AS_ARRAY.len());
        Ok(Self {
            name: values.remove(0).try_into()?,
            alias: Option::<&str>::try_from(&values.remove(0))?.map(|s| s.to_string()),
            selected: (&values.remove(0)).try_into::<i64>()? > 0,
        })
    }
}

impl From<&model::Account> for Vec<sqlite::Value> {
    fn from(model: &model::Account) -> Self {
        // We use a match statement to force a build error if the struct
        // fields change.
        match model {
            model::Account {
                name,
                alias,
                selected,
            } => vec![
                name.as_str().into(),
                alias
                    .as_ref()
                    .map(|a| a.as_str().into())
                    .unwrap_or(sqlite::Value::Null),
                (*selected as i64).into(),
            ],
        }
    }
}

impl_model_operations!(model::Account, accounts);

impl TryFrom<Vec<sqlite::Value>> for model::TagRule {
    type Error = anyhow::Error;

    fn try_from(mut values: Vec<sqlite::Value>) -> Result<Self> {
        assert_eq!(values.len(), Self::FIELD_NAMES_AS_ARRAY.len());
        macro_rules! null_opt {
            ($v:expr) => {{
                let v = $v;
                if v == sqlite::Value::Null {
                    None
                } else {
                    Some(v.try_into()?)
                }
            }};
        }
        Ok(Self {
            id: (&values.remove(0)).try_into()?,
            tag: values.remove(0).try_into()?,
            human_readable: values.remove(0).try_into()?,
            transaction_id: (&values.remove(0)).try_into()?,
            description_contains: Option::<&str>::try_from(&values.remove(0))?
                .map(|s| s.to_string()),
            transaction_type: null_opt!(values.remove(0)),
            amount_min: (&as_float!(values.remove(0))).try_into()?,
            amount_max: (&as_float!(values.remove(0))).try_into()?,
            from_date: try_from_sqlite_value_to_naive_date_opt(values.remove(0))?,
            to_date: try_from_sqlite_value_to_naive_date_opt(values.remove(0))?,
        })
    }
}

impl From<&model::TagRule> for Vec<sqlite::Value> {
    fn from(model: &model::TagRule) -> Self {
        use sqlite::Value::Null;
        // We use a match statement to force a build error if the struct
        // fields change.
        match model {
            model::TagRule {
                id,
                tag,
                human_readable,
                transaction_id,
                description_contains,
                transaction_type,
                amount_min,
                amount_max,
                from_date,
                to_date,
            } => vec![
                id.map(|v| v.into()).unwrap_or(Null),
                tag.as_str().into(),
                human_readable.as_str().into(),
                transaction_id.map(|v| v.into()).unwrap_or(Null),
                description_contains
                    .as_ref()
                    .map(|v| v.as_str().into())
                    .unwrap_or(Null),
                transaction_type.as_ref().map(|v| v.into()).unwrap_or(Null),
                amount_min.map(|v| v.into()).unwrap_or(Null),
                amount_max.map(|v| v.into()).unwrap_or(Null),
                from_date
                    .as_ref()
                    .map(from_naive_date_to_sqlite_value)
                    .unwrap_or(Null),
                to_date
                    .as_ref()
                    .map(from_naive_date_to_sqlite_value)
                    .unwrap_or(Null),
            ],
        }
    }
}

impl_model_operations!(model::TagRule, tag_rules);

fn query_filters_to_sql(filters: QueryFilters) -> (String, Vec<(String, sqlite::Value)>) {
    let mut sql = vec![];
    let mut args: HashMap<String, sqlite::Value> = HashMap::new();

    if filters.accounts.len() > 0 {
        // Create parameters A0, A1, ... for each account, and the SQL to match
        // against each of them.
        let mut accounts_sql = vec!["(".to_string()];
        for (i, account) in filters.accounts.iter().enumerate() {
            let name = format!(":A{}", i);
            args.insert(name.clone(), account.clone().into());
            accounts_sql.push(format!("INSTR(LOWER(account_name), LOWER({}))", name));
            if i < filters.accounts.len() - 1 {
                accounts_sql.push("OR".to_string());
            }
        }
        accounts_sql.push(")".to_string());
        sql.push(accounts_sql.join(" "));
    }

    if filters.tags.len() > 0 {
        // Create parameters T0, T1, ... for each tag.
        let mut tag_parameters = vec![];
        for (i, tag) in filters.tags.iter().enumerate() {
            let name = format!(":T{}", i);
            tag_parameters.push(name.clone());
            args.insert(name, tag.clone().into());
        }
        // Generate SQL to prefix-match the tags to the parameters we
        // created above.
        sql.push(format!(
            "transactions.id IN (
                        SELECT DISTINCT transaction_id
                        FROM transactions_tags
                        WHERE ({}))",
            tag_parameters
                .iter()
                .map(|tp| format!("SUBSTR(tag, 1, LENGTH({tp})) = {tp}"))
                .collect::<Vec<_>>()
                .join(" OR ")
        ));
    }

    if let Some(description_contains) = filters.description_contains {
        sql.push("INSTR(LOWER(description), LOWER(:DESCRIPTION_CONTAINS))".to_string());
        args.insert(
            ":DESCRIPTION_CONTAINS".to_string(),
            description_contains.into(),
        );
    }

    let amount_column =
        r#"IIF(transactions.transaction_type = "Credit", credit_amount, debit_amount)"#;

    if let Some(amount_min) = filters.amount_min {
        sql.push(format!("{amount_column} >= :AMOUNT_MIN"));
        args.insert(":AMOUNT_MIN".to_string(), amount_min.into());
    }

    if let Some(amount_max) = filters.amount_max {
        sql.push(format!("{amount_column} < :AMOUNT_MAX"));
        args.insert(":AMOUNT_MAX".to_string(), amount_max.into());
    }

    if let Some(date_from) = filters.date_from {
        sql.push("posted_date >= :DATE_FROM".to_string());
        args.insert(
            ":DATE_FROM".to_string(),
            from_naive_date_to_sqlite_value(&date_from),
        );
    }

    if let Some(date_to) = filters.date_to {
        sql.push("posted_date <= :DATE_TO".to_string());
        args.insert(
            ":DATE_TO".to_string(),
            from_naive_date_to_sqlite_value(&date_to),
        );
    }

    if filters.transaction_types.len() > 0 {
        let mut tt_parameters = vec![];
        for (i, tt) in filters.transaction_types.iter().enumerate() {
            let name = format!(":TT{}", i);
            tt_parameters.push(name.clone());
            args.insert(name, tt.into());
        }
        sql.push(format!(
            "transactions.transaction_type IN ({})",
            tt_parameters.join(", ")
        ));
    }

    let sql = if sql.len() > 0 {
        format!("WHERE ({})", sql.join(" AND "))
    } else {
        "".to_string()
    };
    (sql, args.drain().collect())
}

macro_rules! impl_query_operations {
    ($row_type:ty, $template:expr, $transaction_types:expr, $((replace $from:expr, $to:expr)),*) => (
        impl QueryOperations<$row_type> for SQLiteStingyDatabase {
            fn query(&self, mut filters: QueryFilters) -> Result<QueryResult<$row_type>> {
                let mut query_sql = $template.to_string();
                let transaction_types: Vec<model::TransactionType> = $transaction_types;
                if (transaction_types.len() > 0) {
                    filters.transaction_types = transaction_types;
                }
                let (filters_sql, args) = query_filters_to_sql(filters);
                let args: Vec<_> = args.iter().map(|(k, v)| (k.as_str(), v)).collect();

                query_sql = query_sql.replace("{filters}", &filters_sql);
                $(
                    query_sql = query_sql.replace($from, $to);
                )*

                let sqlite_rows = sql(&self.conn, &query_sql, args.as_slice())?;
                let mut rows = Vec::new();
                for row in sqlite_rows {
                    rows.push(row.try_into()?);
                }
                Ok(QueryResult { rows })
            }
        }
    );
    ($row_type:ty, $sql:expr) => (impl_query_operations!($row_type, $sql, vec![], (replace "", ""));)
}

impl TryFrom<Vec<sqlite::Value>> for DebitsRow {
    type Error = anyhow::Error;

    fn try_from(mut values: Vec<sqlite::Value>) -> Result<Self> {
        assert_eq!(values.len(), Self::FIELD_NAMES_AS_ARRAY.len());
        Ok(Self {
            account_name: values.remove(0).try_into()?,
            transaction_id: (&values.remove(0)).try_into()?,
            tags: String::try_from(values.remove(0))?
                .split("\n")
                .map(|s| s.to_string())
                .collect(),
            debit_amount: (&as_float!(values.remove(0))).try_into()?,
            description: values.remove(0).try_into()?,
            posted_date: try_from_sqlite_value_to_naive_date(values.remove(0))?,
            debit_cumulative: (&as_float!(values.remove(0))).try_into()?,
            debit_pct_cumulative: (&as_float!(values.remove(0))).try_into()?,
        })
    }
}

impl_query_operations!(
    DebitsRow,
    include_str!("./sql/queries/credits_debits.sql"),
    vec![model::TransactionType::Debit, model::TransactionType::DirectDebit],
    (replace "{amount_column}", "debit_amount"));

impl TryFrom<Vec<sqlite::Value>> for CreditsRow {
    type Error = anyhow::Error;

    fn try_from(mut values: Vec<sqlite::Value>) -> Result<Self> {
        assert_eq!(values.len(), Self::FIELD_NAMES_AS_ARRAY.len());
        Ok(Self {
            account_name: values.remove(0).try_into()?,
            transaction_id: (&values.remove(0)).try_into()?,
            tags: String::try_from(values.remove(0))?
                .split("\n")
                .map(|s| s.to_string())
                .collect(),
            credit_amount: (&as_float!(values.remove(0))).try_into()?,
            description: values.remove(0).try_into()?,
            posted_date: try_from_sqlite_value_to_naive_date(values.remove(0))?,
            credit_cumulative: (&as_float!(values.remove(0))).try_into()?,
            credit_pct_cumulative: (&as_float!(values.remove(0))).try_into()?,
        })
    }
}

impl_query_operations!(
    CreditsRow,
    include_str!("./sql/queries/credits_debits.sql"),
    vec![model::TransactionType::Credit],
    (replace "{amount_column}", "credit_amount"));

impl TryFrom<Vec<sqlite::Value>> for ByMonthRow {
    type Error = anyhow::Error;

    fn try_from(mut values: Vec<sqlite::Value>) -> Result<Self> {
        assert_eq!(values.len(), Self::FIELD_NAMES_AS_ARRAY.len());
        Ok(Self {
            account_name: values.remove(0).try_into()?,
            month: try_from_sqlite_value_to_naive_date(values.remove(0))?,
            credit_amount: (&as_float!(values.remove(0))).try_into()?,
            debit_amount: (&as_float!(values.remove(0))).try_into()?,
            credit_minus_debit: (&as_float!(values.remove(0))).try_into()?,
            balance: (&as_float!(values.remove(0))).try_into()?,
            credit_cumulative: (&as_float!(values.remove(0))).try_into()?,
            debit_cumulative: (&as_float!(values.remove(0))).try_into()?,
        })
    }
}

impl_query_operations!(ByMonthRow, include_str!("./sql/queries/by_month.sql"));

impl TryFrom<Vec<sqlite::Value>> for ByTagRow {
    type Error = anyhow::Error;

    fn try_from(mut values: Vec<sqlite::Value>) -> Result<Self> {
        assert_eq!(values.len(), Self::FIELD_NAMES_AS_ARRAY.len());
        Ok(Self {
            tag: values.remove(0).try_into()?,
            tag_debit: (&as_float!(values.remove(0))).try_into()?,
            tag_debit_pct: (&as_float!(values.remove(0))).try_into()?,
            tag_credit: (&as_float!(values.remove(0))).try_into()?,
            tag_credit_pct: (&as_float!(values.remove(0))).try_into()?,
        })
    }
}

impl_query_operations!(ByTagRow, include_str!("./sql/queries/by_tag.sql"));

#[cfg(test)]
mod sqlite_database_tests {
    use super::*;

    #[test]
    fn migrate_from_empty_database() {
        let conn = sqlite::open(":memory:").unwrap();
        let migrations = vec![
            ("0", ""),
            ("1", "CREATE TABLE test(count INTEGER)"),
            ("2", "INSERT INTO test VALUES(1)"),
            ("3", "UPDATE test SET count = count + 1"),
        ];
        perform_migrations(&conn, &migrations).unwrap();
        let rows = sqlv!(&conn, "SELECT count FROM test").unwrap();
        let count = (&rows[0][0]).try_into::<i64>().unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn migrate_from_existing_schema() {
        let conn = sqlite::open(":memory:").unwrap();
        let migrations = vec![("0", ""), ("1", "CREATE TABLE test(count INTEGER)")];
        perform_migrations(&conn, &migrations).unwrap();
        let migrations = vec![
            ("0", ""),
            ("1", "CREATE TABLE test(count INTEGER)"),
            ("2", "INSERT INTO test VALUES(1)"),
            ("3", "UPDATE test SET count = count + 1"),
        ];
        perform_migrations(&conn, &migrations).unwrap();
        let rows = sqlv!(&conn, "SELECT count FROM test").unwrap();
        let count = (&rows[0][0]).try_into::<i64>().unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn rollback_failed_migration() {
        let conn = sqlite::open(":memory:").unwrap();
        let migrations = vec![
            ("0", ""),
            ("1", "CREATE TABLE test(count INTEGER, UNIQUE(count))"),
            ("2", "INSERT INTO test VALUES(1)"),
            (
                "3",
                "UPDATE test SET count = count + 1; INSERT INTO test VALUES(2)",
            ),
            ("4", "UPDATE test SET count = count + 1"),
        ];
        assert!(perform_migrations(&conn, &migrations).is_err());
        let rows = sqlv!(&conn, "SELECT count FROM test").unwrap();
        let count = (&rows[0][0]).try_into::<i64>().unwrap();
        // Migration "2" was applied, 3 was rolled back, 4 was not attempted.
        assert_eq!(count, 1);
    }

    #[test]
    fn transactions_unique_constraint() {
        // Ensure all non-id rows in the transactions table are part of its
        // UNIQUE constraint.
        let conn = sqlite::open(":memory:").unwrap();
        initialize_sqlite(&conn).unwrap();
        let row: String = sqlv!(
            &conn,
            "SELECT sql FROM sqlite_schema
            WHERE type = 'table' AND name = 'transactions'"
        )
        .unwrap()
        .remove(0)
        .remove(0)
        .try_into()
        .unwrap();
        let start = row.find("UNIQUE(").unwrap() + "UNIQUE(".len();
        let end = row[start..].find(")").unwrap();
        let unique_constraint = row[start..(start + end)].to_string();
        let mut unique_columns: Vec<&str> =
            unique_constraint.split(",").map(|c| c.trim()).collect();
        unique_columns.sort();
        let mut columns = model::Transaction::FIELD_NAMES_AS_ARRAY[1..].to_vec();
        columns.sort();
        assert_eq!(unique_columns, columns);
    }
}
