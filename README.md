# Stingy

## What is this?

Stingy is a simple command-line tool for managing your expenses.

https://github.com/eggpi/stingy/assets/489134/017219fc-b559-4849-9e26-5c4339a85861

(The demo above uses fake data. Thankfully, I'm not in that much debt.)

It allows you to:

* Import transactions from your regular bank.
  * Currently only [AIB](http://aib.ie) and [Revolut](http://revolut.com) are
    supported.
* Create rules for automatically tagging transactions.
* Query transactions in various ways, slicing by period, tag, description and more.

## Installing

There are currently no binary releases of Stingy, but it can be installed with
standard Rust tooling.

Make sure you have Cargo installed (likely using [rustup](https://rustup.rs/))
and then:

```
cargo install --locked --git https://github.com/eggpi/stingy.git
```

## Usage

### Importing transactions

First, get a CSV export of your transactions from your bank. Currently only
[AIB](http://aib.ie) and [Revolut](http://revolut.com) are supported.

Then, import the CSV with:

```
stingy import --aib-csv <path-to-aib-csv>
```

Use `--revolut-csv` if importing from Revolut instead.

### Querying transactions

There are four built-in queries:

* By Month
* By Tag
* Debits
* Credits

The first two give you aggregate information about the transactions, grouped by
month an tag, respectively, while the other two give detailed information
about debits and credits, respectively.

Additionally, the transactions that are aggregated or summarized can be filtered
with a variety of options. Use `stingy help query <query-name>` to explore those
options.

Here are a few useful examples:

 How do I... ? |  Command(s)
:--------------|:------------|
View my total credits, debits and balance month by month                              | `stingy query by-month`
View my total credits, debits and balance this year                                   | `stingy query by-month --period 2023/01-:`
View the distribution of transactions per month, for a set of tags                    | `stingy query by-month --tags <tag1>,<tag2>`
View the distribution of transactions across all tags in May this year                | `stingy query by-tag --period May`
View the distribution of transactions by tag, for transactions over a certain amount  | `stingy query by-tag --amount-range <min>-:`
View my debits for the month, sorted by amount                                        | `stingy query debits --period May`
Search my debits by description (e.g. how much did I pay at that restaurant?)         | `stingy query debits --description-contains <description>`
List all debits with a given tag                                                      | `stingy query debits --tags <tag1>,<tag2>`
List all debits, except ones with a given tag                                         | `stingy query debits --not-tags <tag1>,<tag2>`
View all credits for the year (and sum total), for a given account                    | `stingy query credits --period 2022/01-2022/12 --account <account>`

All filtering options generally work across all queries, so try them out!

### Tagging transactions

To tag transactions, you need to create a _tag rule_ that will be evaluated on every
transaction, tagging it if there is a match.

The attributes of a tag rule are similar to the query filters
[explained above](#querying-transactions). Use `stingy help tags add-rule` to
explore those attributes.

Here are a few useful examples:

 How do I... ? |  Command(s)
:--------------|:------------|
Create a tag for my electricity bills, whose description is "ELECTRICITY COMPANY"  | `stingy tags add-rule --description-contains ELECTRIC --tag "electricity bill"`
Create a tag for my debits over a certain period (e.g., during a trip)             | `stingy tags add-rule --period 2022/09/12-2022/09/19 --tag "travel/athens"`
View the tags I've created so far                                                  | `stingy tags list`
Delete a tag                                                                       | `stingy tags list` to find its ID, then `stingy tags delete-rule <ID>`
Tag one specific transaction                                                       | `stingy query debits --show-transaction-id` to find its ID, then `stingy tags add-rule --tag <tag> --transaction-id <ID>`

### Managing accounts

Accounts are automatically created when you [import a CSV file](#importing-transactions).

Queries by default return transactions across all accounts, but you can select
an account (see `stingy accounts help select`) to restrict further queries to it.

You can also create aliases for accounts (see `stingy accounts help alias`) as a
convenience. The alias can be used anywhere an account name is accepted.

### Advanced tips

* Most command line options can be shortened for convenience: try `stingy q by-m`!

* Textual options are matched to the data in a partial, case-insensitive way.

  You can use this to create hierarchies of tags (e.g. "travel/prague",
  "travel/dublin") and then do hierarchical queries (e.g.
  `stingy query by-month --tag travel/` to see total travel transactions).

* You can undo previous commands with `stingy undo` if they modified the
  database.

  The undo history persists across invocations, but is deleted when the database
  changes in a version update.

## Development

### Building and running tests

This is a simple Rust project managed by Cargo. Use `cargo build` to build it
(or `cargo run` to run directly), and `cargo test` to run the tests.

### Git hooks

The git-hooks/ directory contains hooks to ensure tests are green and formatting
is good before commits and pushes. Install them with:

```
cp git-hooks/* .git/hooks/
```

### Releasing

1. Use `cargo update` to update Cargo.lock.
1. Bump the version in Cargo.toml.
1. Commit and push.
