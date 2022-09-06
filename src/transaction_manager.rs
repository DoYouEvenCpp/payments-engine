use crate::{account::Account, amount::Amount, record::OperationType, record::Record};
use std::collections::HashMap;

use anyhow::Result;

struct TransactionRecord {
    amount: Option<Amount>,
    under_dispute: bool,
    already_disputed: bool,
}

impl TransactionRecord {
    fn new(amount: Option<Amount>) -> Self {
        Self {
            amount,
            under_dispute: false,
            already_disputed: false,
        }
    }
}

type Accounts = HashMap<u16, Account>;
type Transactions = HashMap<u32, TransactionRecord>;

pub struct TransactionManager {
    accounts: Accounts,
    transactions: Transactions,
}

impl TransactionManager {
    pub fn new() -> Self {
        Self {
            accounts: Accounts::new(),
            transactions: Transactions::new(),
        }
    }

    pub fn parse_entry(&mut self, record: Record) -> Result<()> {
        let entry = self
            .accounts
            .entry(record.client)
            .or_insert_with(|| Account::new(record.client));
        if record.amount.is_some() {
            self.transactions
                .entry(record.tx)
                .or_insert_with(|| TransactionRecord::new(record.amount));
        }
        match record.r#type {
            OperationType::Deposit => {
                if let Some(amount) = record.amount {
                    entry.deposit(amount)?;
                }
            }
            OperationType::Withdrawal => {
                if let Some(amount) = record.amount {
                    entry.withdrawal(amount)?;
                }
            }
            OperationType::Chargeback => {
                if let Some(transaction) = self.transactions.get_mut(&record.tx) {
                    if transaction.under_dispute {
                        if let Some(amount) = transaction.amount {
                            transaction.under_dispute = false;
                            entry.chargeback(amount)?;
                        }
                    }
                }
            }
            OperationType::Dispute => {
                if let Some(transaction) = self.transactions.get_mut(&record.tx) {
                    if !transaction.already_disputed {
                        transaction.under_dispute = true;
                        transaction.already_disputed = true;
                        if let Some(amount) = transaction.amount {
                            entry.dispute(amount)?;
                        }
                    }
                }
            }
            OperationType::Resolve => {
                if let Some(transaction) = self.transactions.get_mut(&record.tx) {
                    if transaction.under_dispute {
                        transaction.under_dispute = false;
                        if let Some(amount) = record.amount {
                            entry.resolve(amount)?;
                        }
                    }
                }
            }
        }
        Ok(())
    }
    pub fn accounts(&self) -> impl Iterator<Item = &Account> {
        self.accounts.values()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::amount::Amount;
    use rust_decimal_macros::dec;

    impl Record {
        pub fn new(r#type: OperationType, client: u16, tx: u32, amount: Option<Amount>) -> Self {
            Self {
                r#type,
                client,
                tx,
                amount,
            }
        }
    }

    #[test]
    fn test_dispute_on_non_existing_transaction_has_no_effets() {
        let mut manager = TransactionManager::new();
        let records: Vec<Record> = vec![
            Record::new(OperationType::Deposit, 1, 1, Some(Amount(dec!(12.5)))),
            Record::new(OperationType::Dispute, 1, 100, None),
        ];

        for r in records {
            assert!(manager.parse_entry(r).is_ok());
        }

        assert_eq!(manager.accounts.len(), 1);
        assert!(manager.accounts.get(&1).is_some());
        assert_eq!(manager.accounts.get(&1).unwrap().available(), dec!(12.5));
        assert_eq!(manager.accounts.get(&1).unwrap().held(), dec!(0.0));
    }

    #[test]
    fn test_account_chargeback_for_operation_that_was_not_under_dispute_shall_have_no_effect() {
        let mut manager = TransactionManager::new();
        let records: Vec<Record> = vec![
            Record::new(OperationType::Deposit, 1, 1, Some(Amount(dec!(100.0)))),
            Record::new(OperationType::Deposit, 1, 2, Some(Amount(dec!(20.0)))),
            Record::new(OperationType::Deposit, 1, 3, Some(Amount(dec!(15.0)))),
            Record::new(OperationType::Dispute, 1, 3, None), //blocks 15.0, available 120
            Record::new(OperationType::Chargeback, 1, 2, None), //#2 wasn't under dispute, no effect
        ];

        for r in records {
            assert!(manager.parse_entry(r).is_ok());
        }

        assert_eq!(manager.accounts.len(), 1);
        assert!(manager.accounts.get(&1).is_some());
        assert_eq!(manager.accounts.get(&1).unwrap().held(), dec!(15.0));
        assert_eq!(manager.accounts.get(&1).unwrap().available(), dec!(120.0));
    }

    #[test]
    fn test_within_multiple_dispute_and_chargebacks_only_first_shall_affect_account_balance() {
        let mut manager = TransactionManager::new();
        let records: Vec<Record> = vec![
            Record::new(OperationType::Deposit, 1, 1, Some(Amount(dec!(100.0)))),
            Record::new(OperationType::Deposit, 1, 2, Some(Amount(dec!(20.0)))),
            Record::new(OperationType::Dispute, 1, 2, None),
            Record::new(OperationType::Chargeback, 1, 2, None),
            Record::new(OperationType::Dispute, 1, 2, None),
            Record::new(OperationType::Chargeback, 1, 2, None),
            Record::new(OperationType::Dispute, 1, 2, None),
            Record::new(OperationType::Chargeback, 1, 2, None),
        ];

        for r in records {
            assert!(manager.parse_entry(r).is_ok());
        }

        assert_eq!(manager.accounts.len(), 1);
        assert!(manager.accounts.get(&1).is_some());
        assert_eq!(manager.accounts.get(&1).unwrap().held(), dec!(0.0));
        assert!(manager.accounts.get(&1).unwrap().is_locked());
        assert_eq!(manager.accounts.get(&1).unwrap().available(), dec!(100.0));
    }

    #[test]
    fn test_resolve_for_operation_that_is_not_under_dispute_shall_have_no_effect() {
        let mut manager = TransactionManager::new();
        let records: Vec<Record> = vec![
            Record::new(OperationType::Deposit, 1, 1, Some(Amount(dec!(1.234)))),
            Record::new(OperationType::Resolve, 1, 1, None),
        ];

        for r in records {
            assert!(manager.parse_entry(r).is_ok());
        }

        assert_eq!(manager.accounts.len(), 1);
        assert!(manager.accounts.get(&1).is_some());
        assert_eq!(manager.accounts.get(&1).unwrap().held(), dec!(0.0));
        assert!(!manager.accounts.get(&1).unwrap().is_locked());
        assert_eq!(manager.accounts.get(&1).unwrap().available(), dec!(1.234));
    }

    #[test]
    fn test_resolve_for_operation_that_was_alredy_chargedbacked_shall_have_no_effect() {
        let mut manager = TransactionManager::new();
        let records: Vec<Record> = vec![
            Record::new(OperationType::Deposit, 1, 1, Some(Amount(dec!(0.234)))),
            Record::new(OperationType::Deposit, 1, 2, Some(Amount(dec!(1.0)))),
            Record::new(OperationType::Dispute, 1, 2, None),
            Record::new(OperationType::Chargeback, 1, 2, None),
            Record::new(OperationType::Resolve, 1, 2, None),
        ];

        for r in records {
            assert!(manager.parse_entry(r).is_ok());
        }

        assert_eq!(manager.accounts.len(), 1);
        assert!(manager.accounts.get(&1).is_some());
        assert_eq!(manager.accounts.get(&1).unwrap().held(), dec!(0.0));
        assert!(manager.accounts.get(&1).unwrap().is_locked());
        assert_eq!(manager.accounts.get(&1).unwrap().available(), dec!(0.234));
    }
}
