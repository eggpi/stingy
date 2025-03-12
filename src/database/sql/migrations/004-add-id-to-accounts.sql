CREATE TABLE new_accounts (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT,
    alias TEXT,
    selected INTEGER,
    UNIQUE(name)
);

INSERT INTO new_accounts SELECT NULL, name, alias, selected FROM accounts;
DROP TABLE accounts;
ALTER TABLE new_accounts RENAME TO accounts;

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
