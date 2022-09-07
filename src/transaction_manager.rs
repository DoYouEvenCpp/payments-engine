use crate::{
    account::Account, amount::Amount, error::Errors, record::OperationType, record::Record,
};
use std::collections::HashMap;

use anyhow::Result;

#[derive(Debug)]
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

    pub fn parse_entry(&mut self, record: Record) -> Result<(), Errors> {
        let account = self
            .accounts
            .entry(record.client)
            .or_insert_with(|| Account::new(record.client));

        //keep track only of transactions that are of type deposit or withdrawal
        //if there's a dispute/resolve/chargeback that is reffering to a non-existing operation
        //then it would get dropped anyway
        match record.r#type {
            OperationType::Deposit => {
                if self
                    .transactions
                    .insert(record.tx, TransactionRecord::new(record.amount))
                    .is_some()
                {
                    return Err(Errors::TransactionIdAlreadyUsed(record.tx));
                }
                if let Some(amount) = record.amount {
                    account.deposit(amount)?;
                }
            }
            OperationType::Withdrawal => {
                if self
                    .transactions
                    .insert(record.tx, TransactionRecord::new(record.amount))
                    .is_some()
                {
                    return Err(Errors::TransactionIdAlreadyUsed(record.tx));
                }
                if let Some(amount) = record.amount {
                    account.withdrawal(amount)?;
                }
            }
            OperationType::Chargeback => {
                if let Some(transaction) = self.transactions.get_mut(&record.tx) {
                    if transaction.under_dispute {
                        if let Some(amount) = transaction.amount {
                            transaction.under_dispute = false;
                            account.chargeback(amount)?;
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
                            account.dispute(amount)?;
                        }
                    }
                }
            }
            OperationType::Resolve => {
                if let Some(transaction) = self.transactions.get_mut(&record.tx) {
                    if transaction.under_dispute {
                        transaction.under_dispute = false;
                        if let Some(amount) = record.amount {
                            account.resolve(amount)?;
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

    //either allow(dead_code) or keep it in here
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
            Record::new(OperationType::Deposit, 1, 1, Some(dec!(12.5).into())),
            Record::new(OperationType::Dispute, 1, 100, None),
        ];

        assert!(records.into_iter().all(|r| manager.parse_entry(r).is_ok()));

        assert_eq!(manager.accounts.get(&1).unwrap().available(), dec!(12.5));
        assert_eq!(manager.accounts.get(&1).unwrap().held(), dec!(0.0));
    }

    #[test]
    fn test_dispute_on_locked_account_shall_be_be_completed() {
        let mut manager = TransactionManager::new();
        let records: Vec<Record> = vec![
            Record::new(OperationType::Deposit, 1, 1, Some(dec!(12).into())),
            Record::new(OperationType::Deposit, 1, 2, Some(dec!(5).into())),
            Record::new(OperationType::Dispute, 1, 1, None),
            Record::new(OperationType::Chargeback, 1, 1, None),
            Record::new(OperationType::Dispute, 1, 2, None),
            Record::new(OperationType::Chargeback, 1, 2, None),
        ];

        assert!(records.into_iter().all(|r| manager.parse_entry(r).is_ok()));

        assert_eq!(manager.accounts.get(&1).unwrap().held(), dec!(0));
        assert_eq!(manager.accounts.get(&1).unwrap().available(), dec!(0));
    }

    #[test]
    fn test_chargeback_for_operation_that_was_not_under_dispute_shall_have_no_effect() {
        let mut manager = TransactionManager::new();
        let records: Vec<Record> = vec![
            Record::new(OperationType::Deposit, 1, 1, Some(dec!(100.0).into())),
            Record::new(OperationType::Deposit, 1, 2, Some(dec!(20.0).into())),
            Record::new(OperationType::Deposit, 1, 3, Some(dec!(15.0).into())),
            Record::new(OperationType::Dispute, 1, 3, None), //blocks 15.0, available 120
            Record::new(OperationType::Chargeback, 1, 2, None), //#2 wasn't under dispute, no effect
        ];

        assert!(records.into_iter().all(|r| manager.parse_entry(r).is_ok()));

        assert_eq!(manager.accounts.get(&1).unwrap().held(), dec!(15.0));
        assert_eq!(manager.accounts.get(&1).unwrap().available(), dec!(120.0));
    }

    #[test]
    fn test_within_multiple_dispute_and_chargebacks_only_first_shall_affect_account_balance() {
        let mut manager = TransactionManager::new();
        let records: Vec<Record> = vec![
            Record::new(OperationType::Deposit, 1, 1, Some(dec!(100.0).into())),
            Record::new(OperationType::Deposit, 1, 2, Some(dec!(20.0).into())),
            Record::new(OperationType::Dispute, 1, 2, None),
            Record::new(OperationType::Chargeback, 1, 2, None),
            Record::new(OperationType::Dispute, 1, 2, None),
            Record::new(OperationType::Chargeback, 1, 2, None),
            Record::new(OperationType::Dispute, 1, 2, None),
            Record::new(OperationType::Chargeback, 1, 2, None),
        ];

        assert!(records.into_iter().all(|r| manager.parse_entry(r).is_ok()));

        assert_eq!(manager.accounts.get(&1).unwrap().held(), dec!(0.0));
        assert!(manager.accounts.get(&1).unwrap().is_locked());
        assert_eq!(manager.accounts.get(&1).unwrap().available(), dec!(100.0));
    }

    #[test]
    fn test_resolve_for_operation_that_is_not_under_dispute_shall_have_no_effect() {
        let mut manager = TransactionManager::new();
        let records: Vec<Record> = vec![
            Record::new(OperationType::Deposit, 1, 1, Some(dec!(1.234).into())),
            Record::new(OperationType::Resolve, 1, 1, None),
        ];

        assert!(records.into_iter().all(|r| manager.parse_entry(r).is_ok()));

        assert_eq!(manager.accounts.get(&1).unwrap().held(), dec!(0.0));
        assert_eq!(manager.accounts.get(&1).unwrap().available(), dec!(1.234));
    }

    #[test]
    fn test_chargeback_for_operation_that_is_not_under_dispute_shall_have_no_effect() {
        let mut manager = TransactionManager::new();
        let records: Vec<Record> = vec![
            Record::new(OperationType::Deposit, 1, 1, Some(dec!(1.234).into())),
            Record::new(OperationType::Chargeback, 1, 1, None),
        ];

        assert!(records.into_iter().all(|r| manager.parse_entry(r).is_ok()));

        assert_eq!(manager.accounts.get(&1).unwrap().held(), dec!(0.0));
        assert!(!manager.accounts.get(&1).unwrap().is_locked());
        assert_eq!(manager.accounts.get(&1).unwrap().available(), dec!(1.234));
    }

    #[test]
    fn test_resolve_for_operation_that_was_already_chargedbacked_shall_have_no_effect() {
        let mut manager = TransactionManager::new();
        let records: Vec<Record> = vec![
            Record::new(OperationType::Deposit, 1, 1, Some(dec!(0.234).into())),
            Record::new(OperationType::Deposit, 1, 2, Some(dec!(1.0).into())),
            Record::new(OperationType::Dispute, 1, 2, None),
            Record::new(OperationType::Chargeback, 1, 2, None),
            Record::new(OperationType::Resolve, 1, 2, None),
        ];

        assert!(records.into_iter().all(|r| manager.parse_entry(r).is_ok()));

        assert_eq!(manager.accounts.get(&1).unwrap().held(), dec!(0.0));
        assert!(manager.accounts.get(&1).unwrap().is_locked());
        assert_eq!(manager.accounts.get(&1).unwrap().available(), dec!(0.234));
    }

    #[test]
    fn test_resolve_on_non_existing_transaction_has_no_effets() {
        let mut manager = TransactionManager::new();
        let records: Vec<Record> = vec![
            Record::new(OperationType::Deposit, 1, 1, Some(dec!(1).into())),
            Record::new(OperationType::Resolve, 1, 2, None),
        ];

        assert!(records.into_iter().all(|r| manager.parse_entry(r).is_ok()));

        assert_eq!(manager.accounts.get(&1).unwrap().available(), dec!(1));
        assert_eq!(manager.accounts.get(&1).unwrap().held(), dec!(0.0));
    }

    #[test]
    fn test_chargeback_on_non_existing_transaction_has_no_effets() {
        let mut manager = TransactionManager::new();
        let records: Vec<Record> = vec![
            Record::new(OperationType::Deposit, 1, 1, Some(dec!(2).into())),
            Record::new(OperationType::Chargeback, 1, 3, None),
        ];

        assert!(records.into_iter().all(|r| manager.parse_entry(r).is_ok()));

        assert_eq!(manager.accounts.get(&1).unwrap().available(), dec!(2));
        assert_eq!(manager.accounts.get(&1).unwrap().held(), dec!(0.0));
    }
}
