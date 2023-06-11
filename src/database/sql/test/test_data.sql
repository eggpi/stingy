INSERT INTO accounts VALUES ("000000 - 00000000", NULL, false);
INSERT INTO transactions VALUES
    (NULL, "000000 - 00000000", "2021-02-25", "INCOMING TRANSFER", 0.0, 1000.00, 10000.00, "Credit", "EUR"),
    (NULL, "000000 - 00000000", "2021-02-25", "COFFEE", 3.74, 0.0, 9996.16, "Debit", "EUR"),
    (NULL, "000000 - 00000000", "2021-02-26", "FOOD ORDER 1", 10.00, 0.0, 9986.16, "Debit", "EUR"),
    (NULL, "000000 - 00000000", "2021-02-26", "FOOD ORDER 2", 22.50, 0.0, 9963.50, "Debit", "EUR"),
    (NULL, "000000 - 00000000", "2021-02-26", "GROCERIES", 35.98, 0.0, 9927.52, "Debit", "EUR"),
    (NULL, "000000 - 00000000", "2021-03-01", "GROCERIES", 15.99, 0.0, 9911.53, "Debit", "EUR"),
    (NULL, "000000 - 00000000", "2021-03-01", "PUB", 16.0, 0.0, 9895.53, "Debit", "EUR"),
    (NULL, "000000 - 00000000", "2021-03-01", "COFFEE", 2.99, 0.0, 9885.54, "Debit", "EUR"),
    (NULL, "000000 - 00000000", "2021-03-02", "SUBSCRIPTION", 7.63, 0.0, 9877.91, "Direct Debit", "EUR"),
    (NULL, "000000 - 00000000", "2021-03-03", "FOOD ORDER 3", 25.15, 0.0, 9852.76, "Debit", "EUR");

INSERT INTO accounts VALUES ("111111 - 11111111", NULL, false);
INSERT INTO transactions VALUES
    (NULL, "111111 - 11111111", "2021-03-01", "INSURANCE REPAYMENT", 0.0, 100.00, 100.00, "Credit", "EUR");
