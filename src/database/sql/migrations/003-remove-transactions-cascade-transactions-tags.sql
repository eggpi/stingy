ALTER TABLE transactions_tags RENAME TO old_transactions_tags;

CREATE TABLE transactions_tags (
    transaction_id INTEGER,
    tag_rule_id INTEGER,
    FOREIGN KEY(transaction_id) REFERENCES transactions(id) ON DELETE CASCADE,
    FOREIGN KEY(tag_rule_id) REFERENCES tag_rules(id) ON DELETE CASCADE
    UNIQUE(transaction_id, tag_rule_id)
);

INSERT INTO transactions_tags SELECT * FROM old_transactions_tags;
DROP TABLE old_transactions_tags;
