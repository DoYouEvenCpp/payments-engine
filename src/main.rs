use anyhow::{anyhow, Result};
use clap::Parser;

mod account;
mod amount;
mod error;
mod record;
mod transaction_manager;

/// Command-line arguments structure.
#[derive(Parser, Debug)]
struct Args {
    /// Path to the CSV input file.
    csv_path: String,
}

fn run() -> Result<()> {
    let args = Args::parse();

    // Build the CSV reader with explicit configuration.
    let mut reader = csv::ReaderBuilder::new()
        .delimiter(b',')
        .has_headers(true)
        .flexible(true)
        .trim(csv::Trim::All)
        .from_path(&args.csv_path)
        .map_err(|e| anyhow!("Failed to open CSV file {}: {:?}", args.csv_path, e))?;

    // Read CSV records; log errors whenver an entry fails to deserialize
    let mut valid_records = Vec::new();
    let mut csv_error_count = 0u64;
    for result in reader.deserialize::<record::Record>() {
        match result {
            Ok(record) => valid_records.push(record),
            Err(e) => {
                eprintln!("CSV parsing error: {:?}", e);
                csv_error_count += 1;
            }
        }
    }
    if csv_error_count > 0 {
        eprintln!(
            "Discarded {} CSV entries from {}",
            csv_error_count, args.csv_path
        );
    }

    // Process transactions.
    let mut transactions_manager = transaction_manager::TransactionManager::new();
    let mut failed_transactions = 0u64;
    for record in &valid_records {
        if let Err(err) = transactions_manager.parse_entry(record) {
            eprintln!("Error processing record (tx id: {}): {:?}", record.tx, err);
            failed_transactions += 1;
        }
    }
    if failed_transactions > 0 {
        eprintln!(
            "Discarded {} transactions - failed to follow required logic.",
            failed_transactions
        );
    }

    let mut writer = csv::Writer::from_writer(std::io::stdout());
    for account in transactions_manager.accounts() {
        if let Err(err) = writer.serialize(account) {
            eprintln!(
                "Error serializing account (client id: {}): {:?}",
                account.get_client_id(),
                err
            );
        }
    }
    writer.flush()?;

    Ok(())
}

fn main() {
    // Use proper error handling; exit with non-zero code on fatal error.
    if let Err(err) = run() {
        eprintln!("Fatal error: {:?}", err);
        std::process::exit(1);
    }
}
