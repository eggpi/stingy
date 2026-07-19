WITH filtered_unique_transactions AS (
    SELECT
        transactions.id AS tr_id,
        IFNULL(accounts.alias, account_name) AS account_name,
        {aggregation_expr} AS aggregation,
        credit_amount,
        debit_amount,
        balance
    FROM transactions
    LEFT JOIN transactions_tags ON transactions_tags.transaction_id = transactions.id
    LEFT JOIN tag_rules ON transactions_tags.tag_rule_id = tag_rules.id
    LEFT JOIN accounts ON transactions.account_name = accounts.name
    {filters}
    GROUP BY transactions.id
), aggregated_by_time AS (
    SELECT
        account_name,
        aggregation,
        SUM(credit_amount) AS sum_credit_amount,
        SUM(debit_amount) AS sum_debit_amount,
        SUM(credit_amount) - SUM(debit_amount),
        balance
    FROM filtered_unique_transactions
    GROUP BY 1, 2
    HAVING tr_id = MAX(tr_id) -- Take the balance from the last transaction in the time period
    ORDER BY 2 DESC
)
SELECT *,
    SUM(sum_credit_amount) OVER (
        ORDER BY 2 DESC ROWS BETWEEN CURRENT ROW AND UNBOUNDED FOLLOWING),
    SUM(sum_debit_amount) OVER (
        ORDER BY 2 DESC ROWS BETWEEN CURRENT ROW AND UNBOUNDED FOLLOWING)
FROM aggregated_by_time;
