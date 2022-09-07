use anyhow::Result;
use clap::Parser;

mod account;
mod amount;
mod error;
mod record;
mod transaction_manager;

#[derive(Parser, Debug)]
struct Args {
    csv_path: String,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let mut reader = csv::ReaderBuilder::new()
        .delimiter(b',')
        .has_headers(true)
        .flexible(true)
        .trim(csv::Trim::All)
        .from_path(args.csv_path)?;

    let entries = reader
        .deserialize::<record::Record>()
        .filter_map(|r| r.ok());

    let mut transactions_manager = transaction_manager::TransactionManager::new();
    for e in entries {
        if let Err(err) = transactions_manager.parse_entry(&e) {
            eprintln!("Input parsing error: {:?}", err);
        }
    }
    let mut output_writer = csv::Writer::from_writer(std::io::stdout());
    for acc in transactions_manager.accounts() {
        if let Err(err) = output_writer.serialize(acc) {
            eprintln!("Deserialisation error: {:?}", err);
        }
    }

    Ok(())
}
