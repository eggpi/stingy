SELECT  IFNULL(accounts.alias, account_name),
        transactions.id,
        REPLACE(GROUP_CONCAT(DISTINCT IIF(tag IS NULL, "", tag)), ',', x'0a'),
        {amount_column},
        description,
        posted_date,
        SUM({amount_column}) OVER (
            ORDER BY {amount_column} DESC ROWS BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW
        ),
        100 * SUM({amount_column}) OVER (
            ORDER BY {amount_column} DESC ROWS BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW
        ) / SUM({amount_column}) OVER (
            ORDER BY {amount_column} DESC ROWS BETWEEN UNBOUNDED PRECEDING AND UNBOUNDED FOLLOWING
        )
FROM transactions
LEFT JOIN transactions_tags ON transactions_tags.transaction_id = transactions.id
LEFT JOIN tag_rules ON transactions_tags.tag_rule_id = tag_rules.id
LEFT JOIN accounts ON transactions.account_name = accounts.name
{filters}
GROUP BY transactions.id
ORDER BY transactions.{amount_column} DESC;
