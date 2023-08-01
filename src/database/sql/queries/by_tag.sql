-- https://www.sqlite.org/lang_aggfunc.html explains the difference between
-- TOTAL and SUM.
WITH unique_debits_credits_per_tag AS (
    -- A transaction may be tagged more than once with the same tag by different
    -- rules. To avoid adding up the same transaction twice, we first select the
    -- unique (transaction, debit, credit) tuples...
    SELECT IIF(tag IS NULL, "", tag) AS tag,
           debit_amount,
           credit_amount
    FROM transactions
    LEFT JOIN transactions_tags ON transactions_tags.transaction_id = transactions.id
    LEFT JOIN tag_rules ON transactions_tags.tag_rule_id = tag_rules.id
    {filters} GROUP BY tag, transactions.id
), per_tag_debit_credit AS (
    -- ... and onlt then we aggregate by tag.
    SELECT tag,
           TOTAL(debit_amount) AS tag_debit,
           TOTAL(credit_amount) AS tag_credit
    FROM unique_debits_credits_per_tag
    GROUP BY tag
)
SELECT tag,
       tag_debit,
       IIF(total_debit > 0.0, 100 * tag_debit / total_debit, 0.0),
       tag_credit,
       IIF(total_credit > 0.0, 100 * tag_credit / total_credit, 0.0)
FROM per_tag_debit_credit CROSS JOIN (
    SELECT TOTAL(tag_debit) AS total_debit,
           TOTAL(tag_credit) AS total_credit
    FROM per_tag_debit_credit
) GROUP BY tag ORDER BY tag_debit DESC;
