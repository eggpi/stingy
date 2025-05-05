-- When a transaction is tagged with (at least one) tag rule based on
-- transaction_id, it should suppress all other (non transaction_id) tag rules,
-- so we delete their rows from transactions_tags.
DELETE FROM transactions_tags
WHERE (transaction_id, tag_rule_id) IN (
    SELECT transactions_tags.transaction_id, transactions_tags.tag_rule_id
    FROM transactions_tags
    JOIN tag_rules ON tag_rule_id = tag_rules.id
    WHERE tag_rules.transaction_id IS NULL AND
    transactions_tags.transaction_id IN (
        SELECT DISTINCT transactions_tags.transaction_id
        FROM transactions_tags
        JOIN tag_rules ON tag_rule_id = tag_rules.id
        WHERE tag_rules.transaction_id IS NOT NULL
    )
);

CREATE TRIGGER delete_lower_priority_tags_when_transaction_id_tag_rule_is_added
AFTER INSERT ON tag_rules
BEGIN
    DELETE FROM transactions_tags
    WHERE (
        NEW.transaction_id IS NOT NULL AND
        transactions_tags.transaction_id = NEW.transaction_id AND
        transactions_tags.tag_rule_id NOT IN (
            SELECT id FROM tag_rules WHERE transaction_id IS NOT NULL
        )
    );
END;

CREATE TRIGGER delete_lower_priority_tags_when_transaction_id_tag_rule_is_deleted
AFTER DELETE ON tag_rules
BEGIN
    -- Evaluate all tag rules on the transaction that used to be tagged.
    INSERT OR IGNORE INTO transactions_tags
    SELECT OLD.transaction_id, tag_rules.id
    FROM transactions JOIN tag_rules
    WHERE (
        transactions.id = OLD.transaction_id AND
        transactions.id = IFNULL(tag_rules.transaction_id, transactions.id) AND
        INSTR(LOWER(transactions.transaction_type), LOWER(IFNULL(tag_rules.transaction_type, ""))) AND
        INSTR(LOWER(transactions.description), LOWER(IFNULL(tag_rules.description_contains, ""))) AND
        MAX(transactions.debit_amount, transactions.credit_amount) >= IFNULL(tag_rules.amount_min, 0.0) AND
        MAX(transactions.debit_amount, transactions.credit_amount) < IFNULL(tag_rules.amount_max, 9e999) AND
        transactions.posted_date >= IFNULL(tag_rules.from_date, "-Inf") AND
        transactions.posted_date <= IFNULL(tag_rules.to_date, "Inf")
    );
    -- If there is a tag by transaction ID, remove all tags not set by that
    -- attribute.
    DELETE FROM transactions_tags
    WHERE (transaction_id, tag_rule_id) IN (
        SELECT transactions_tags.transaction_id, transactions_tags.tag_rule_id
        FROM transactions_tags
        JOIN tag_rules ON tag_rule_id = tag_rules.id
        WHERE tag_rules.transaction_id IS NULL AND
        transactions_tags.transaction_id IN (
            SELECT DISTINCT transactions_tags.transaction_id
            FROM transactions_tags
            JOIN tag_rules ON tag_rule_id = tag_rules.id
            WHERE tag_rules.transaction_id IS NOT NULL
        )
    );
END;
