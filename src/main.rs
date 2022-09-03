use anyhow::Result;
use clap::Parser;
use std::collections::HashMap;

use crate::{account::Account, record::OperationType, record::Record};

mod account;
mod record;

#[derive(Parser, Debug)]
struct Args {
    csv_path: String,
}

type Accounts = HashMap<u16, account::Account>;
type Transactions = HashMap<u32, Record>;

fn main() -> Result<()> {
    //TODO: removedata duplication between accounts and transactions
    let mut accounts = Accounts::new();
    let mut transactions = Transactions::new();

    let args = Args::parse();
    // let input = std::fs::File::open(args.csv_path)?;
    let mut reader = csv::ReaderBuilder::new()
        .delimiter(b',')
        .has_headers(true)
        .flexible(true)
        .trim(csv::Trim::All)
        .from_path(args.csv_path)?;

    let entries = reader
        .deserialize::<record::Record>()
        .filter_map(|r| r.ok());

    for e in entries {
        let entry = accounts
            .entry(e.client)
            .or_insert_with(|| Account::new(e.client));
        if e.amount.is_some() {
            transactions
                .entry(e.tx)
                .or_insert_with(|| Record::new(e.r#type, e.client, e.tx, e.amount));
        }
        match e.r#type {
            record::OperationType::Deposit => {
                if let Some(amount) = e.amount {
                    entry.deposit(amount)?;
                }
            }
            record::OperationType::Withdrawal => {
                if let Some(amount) = e.amount {
                    entry.withdrawal(amount)?;
                }
            }
            OperationType::Chargeback => {
                if let Some(t) = transactions.get(&e.tx) {
                    if let Some(amount) = t.amount {
                        entry.charbegack(amount)?;
                    }
                }
            }
            OperationType::Dispute => {
                if let Some(t) = transactions.get(&e.tx) {
                    if let Some(amount) = t.amount {
                        entry.dispute(amount)?;
                    }
                }
            }
            OperationType::Resolve => {
                if let Some(t) = transactions.get(&e.tx) {
                    if let Some(amount) = t.amount {
                        entry.resolve(amount)?;
                    }
                }
            }
        }
    }
    let mut output_writer = csv::Writer::from_writer(std::io::stdout());
    for (_, acc) in accounts {
        output_writer.serialize(acc)?;
    }

    Ok(())
}
