CREATE TABLE accounts (
    name TEXT PRIMARY KEY,
    alias TEXT,
    selected INTEGER
);

CREATE TRIGGER alias_account_collision_check_insert
BEFORE INSERT ON accounts
BEGIN
    SELECT RAISE(FAIL, "Alias already exists as an account name, or vice-versa")
    FROM accounts
    WHERE NEW.alias = accounts.name OR NEW.name = accounts.alias;
END;

CREATE TRIGGER alias_account_collision_check_update
BEFORE UPDATE ON accounts
BEGIN
    SELECT RAISE(FAIL, "Alias already exists as an account name, or vice-versa")
    FROM accounts
    WHERE NEW.alias = accounts.name OR NEW.name = accounts.alias;
END;

-- We can't just use a UNIQUE constraint because SQLite treats NULL
-- values as different from one another (https://www.sqlite.org/nulls.html).
CREATE TRIGGER alias_unique_insert
BEFORE INSERT ON accounts
WHEN NEW.alias IS NOT NULL
BEGIN
    SELECT RAISE(FAIL, "UNIQUE constraint failed.")
    FROM accounts
    WHERE NEW.alias = accounts.alias AND NEW.name != accounts.name;
END;

CREATE TRIGGER alias_unique_update
BEFORE UPDATE ON accounts
WHEN NEW.alias IS NOT NULL
BEGIN
    SELECT RAISE(FAIL, "UNIQUE constraint failed.")
    FROM accounts
    WHERE NEW.alias = accounts.alias AND NEW.name != accounts.name;
END;

CREATE TABLE transactions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    account_name,
    posted_date TEXT NOT NULL,
    description TEXT NOT NULL,
    debit_amount REAL NOT NULL,
    credit_amount REAL NOT NULL,
    balance REAL NOT NULL,
    transaction_type TEXT NOT NULL,
    currency TEXT NOT NULL,
    FOREIGN KEY(account_name) REFERENCES accounts(name)
    UNIQUE(
        account_name,
        posted_date,
        description,
        debit_amount,
        credit_amount,
        balance,
        transaction_type,
        currency
    ),
    -- FIXME I'm not sure we really want to distinguish Direct Debit from Debit.
    -- The rest of the code (query, tag rules) treats them the same.
    CHECK(transaction_type IN ("Debit", "Credit", "Direct Debit"))
    CHECK(debit_amount >= 0)
    CHECK(credit_amount >= 0)
);

CREATE TABLE tag_rules (
    -- We can't use the ROWID as the primary key here, because of the foreign
    -- key from transactions_tags to this table.
    -- The SQLite documentation says:
    --
    --     The parent key is the column or set of columns in the parent table that
    --     the foreign key constraint refers to. This is normally, but not always,
    --     the primary key of the parent table. The parent key must be a named
    --     column or columns in the parent table, not the rowid.
    --                                     https://www.sqlite.org/foreignkeys.html
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    tag TEXT NOT NULL,
    human_readable TEXT NOT NULL,
    transaction_id INTEGER,
    description_contains TEXT,
    transaction_type TEXT,
    amount_min REAL,
    amount_max REAL,
    from_date TEXT,
    to_date TEXT,
    FOREIGN KEY(transaction_id) REFERENCES transactions(id) ON DELETE CASCADE
    -- We don't differentiate "debit" and "direct debit" in tag rules.
    CHECK(transaction_type IN ("Debit", "Credit"))
);

-- We can't just use a UNIQUE constraint because SQLite treats NULL
-- values as different from one another (https://www.sqlite.org/nulls.html).
CREATE TRIGGER tag_rules_unique_insert
BEFORE INSERT ON tag_rules
BEGIN
    SELECT RAISE(FAIL, "UNIQUE constraint failed.")
    FROM tag_rules
    WHERE (
        NEW.tag,
        NEW.human_readable,
        NEW.transaction_id,
        NEW.description_contains,
        NEW.transaction_type,
        NEW.amount_min,
        NEW.amount_max,
        NEW.from_date,
        NEW.to_date
    ) IS (
        tag,
        human_readable,
        transaction_id,
        description_contains,
        transaction_type,
        amount_min,
        amount_max,
        from_date,
        to_date
    );
END;

CREATE TRIGGER tag_rules_unique_update
BEFORE UPDATE ON tag_rules
BEGIN
    SELECT RAISE(FAIL, "UNIQUE constraint failed.")
    FROM tag_rules
    WHERE (
        NEW.tag,
        NEW.human_readable,
        NEW.transaction_id,
        NEW.description_contains,
        NEW.transaction_type,
        NEW.amount_min,
        NEW.amount_max,
        NEW.from_date,
        NEW.to_date
    ) IS (
        tag,
        human_readable,
        transaction_id,
        description_contains,
        transaction_type,
        amount_min,
        amount_max,
        from_date,
        to_date
    );
END;

CREATE TABLE transactions_tags (
    transaction_id INTEGER,
    tag_rule_id INTEGER,
    FOREIGN KEY(transaction_id) REFERENCES transactions(id),
    FOREIGN KEY(tag_rule_id) REFERENCES tag_rules(id) ON DELETE CASCADE
    UNIQUE(transaction_id, tag_rule_id)
);

CREATE TRIGGER evaluate_tag_rule_on_tag_rule_insert
AFTER INSERT ON tag_rules
BEGIN
    INSERT OR IGNORE INTO transactions_tags
    SELECT transactions.id, NEW.id
    FROM transactions
    WHERE (
        transactions.id = IFNULL(NEW.transaction_id, transactions.id) AND
        INSTR(LOWER(transactions.transaction_type), LOWER(IFNULL(NEW.transaction_type, ""))) AND
        INSTR(LOWER(transactions.description), LOWER(IFNULL(NEW.description_contains, ""))) AND
        MAX(transactions.debit_amount, transactions.credit_amount) >= IFNULL(NEW.amount_min, 0.0) AND
        MAX(transactions.debit_amount, transactions.credit_amount) < IFNULL(NEW.amount_max, 9e999) AND
        transactions.posted_date >= IFNULL(NEW.from_date, "-Inf") AND
        transactions.posted_date <= IFNULL(NEW.to_date, "Inf")
    );
END;

CREATE TRIGGER evaluate_tag_rules_on_transaction_insert
AFTER INSERT ON transactions
BEGIN
    INSERT OR IGNORE INTO transactions_tags
    SELECT NEW.id, tag_rules.id
    FROM tag_rules
    WHERE (
        NEW.id = IFNULL(tag_rules.transaction_id, NEW.id) AND
        INSTR(LOWER(NEW.transaction_type), LOWER(IFNULL(tag_rules.transaction_type, ""))) AND
        INSTR(LOWER(NEW.description), LOWER(IFNULL(tag_rules.description_contains, ""))) AND
        MAX(NEW.debit_amount, NEW.credit_amount) >= IFNULL(tag_rules.amount_min, 0.0) AND
        MAX(NEW.debit_amount, NEW.credit_amount) < IFNULL(tag_rules.amount_max, 9e999) AND
        NEW.posted_date >= IFNULL(tag_rules.from_date, "-Inf") AND
        NEW.posted_date <= IFNULL(tag_rules.to_date, "Inf")
    );
END;

CREATE TRIGGER evaluate_tag_rules_on_transaction_update
AFTER UPDATE ON transactions
BEGIN
    INSERT OR IGNORE INTO transactions_tags
    SELECT NEW.id, tag_rules.id
    FROM tag_rules
    WHERE (
        NEW.id = IFNULL(tag_rules.transaction_id, NEW.id) AND
        INSTR(LOWER(NEW.transaction_type), LOWER(IFNULL(tag_rules.transaction_type, ""))) AND
        INSTR(LOWER(NEW.description), LOWER(IFNULL(tag_rules.description_contains, ""))) AND
        MAX(NEW.debit_amount, NEW.credit_amount) >= IFNULL(tag_rules.amount_min, 0.0) AND
        MAX(NEW.debit_amount, NEW.credit_amount) < IFNULL(tag_rules.amount_max, 9e999) AND
        NEW.posted_date >= IFNULL(tag_rules.from_date, "-Inf") AND
        NEW.posted_date <= IFNULL(tag_rules.to_date, "Inf")
    );
END;
