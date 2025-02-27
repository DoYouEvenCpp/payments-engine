use crate::{
    account::Account, amount::Amount, error::Errors, record::OperationType, record::Record,
};
use std::collections::HashMap;

use anyhow::Result;

#[derive(Debug)]
struct TransactionRecord {
    operation_type: OperationType,
    amount: Option<Amount>,
    under_dispute: bool,
    already_disputed: bool,
}

impl TransactionRecord {
    fn new(operation_type: OperationType, amount: Option<Amount>) -> Self {
        Self {
            operation_type,
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

    fn get_account(&mut self, cliend_id: u16) -> &mut Account {
        self.accounts
            .entry(cliend_id)
            .or_insert_with(|| Account::new(cliend_id))
    }

    pub fn parse_entry(&mut self, record: &Record) -> Result<(), Errors> {
        //keep track only of transactions that are of type deposit or withdrawal
        //if there's a dispute/resolve/chargeback that reffers to a non-existing operation
        //then it gets dropped anyway
        match record.r#type {
            OperationType::Deposit => {
                if self.transactions.contains_key(&record.tx) {
                    return Err(Errors::TransactionIdAlreadyUsed(record.tx, record.r#type));
                }
                self.transactions.insert(
                    record.tx,
                    TransactionRecord::new(record.r#type, record.amount),
                );
                if let Some(amount) = record.amount {
                    self.get_account(record.client).deposit(amount)?;
                }
            }
            OperationType::Withdrawal => {
                if self.transactions.contains_key(&record.tx) {
                    return Err(Errors::TransactionIdAlreadyUsed(record.tx, record.r#type));
                }
                self.transactions.insert(
                    record.tx,
                    TransactionRecord::new(record.r#type, record.amount),
                );
                if let Some(amount) = record.amount {
                    self.get_account(record.client).withdrawal(amount)?;
                }
            }
            OperationType::Chargeback => {
                if let Some(transaction) = self.transactions.get_mut(&record.tx) {
                    if transaction.under_dispute {
                        if let Some(amount) = transaction.amount {
                            transaction.under_dispute = false;
                            if transaction.operation_type == OperationType::Deposit {
                                self.get_account(record.client).chargeback(amount)?;
                            } else if transaction.operation_type == OperationType::Withdrawal {
                                self.get_account(record.client)
                                    .chargeback_withdrawal(amount)?;
                            }
                        }
                    }
                }
            }
            OperationType::Dispute => {
                if let Some(transaction) = self.transactions.get_mut(&record.tx) {
                    if !transaction.already_disputed {
                        transaction.under_dispute = true;
                        transaction.already_disputed = true;
                        if transaction.operation_type == OperationType::Deposit {
                            if let Some(amount) = transaction.amount {
                                self.get_account(record.client).dispute(amount)?;
                            }
                        }
                    }
                }
            }
            OperationType::Resolve => {
                if let Some(transaction) = self.transactions.get_mut(&record.tx) {
                    if transaction.under_dispute {
                        transaction.under_dispute = false;
                        if let Some(amount) = record.amount {
                            self.get_account(record.client).resolve(amount)?;
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
        let records = vec![
            Record::new(OperationType::Deposit, 1, 1, Some(dec!(12.5).into())),
            Record::new(OperationType::Dispute, 1, 100, None),
        ];

        assert!(records.iter().all(|r| manager.parse_entry(&r).is_ok()));

        assert_eq!(manager.accounts.get(&1).unwrap().available(), dec!(12.5));
        assert_eq!(manager.accounts.get(&1).unwrap().held(), dec!(0.0));
    }

    #[test]
    fn test_dispute_on_locked_account_shall_be_completed() {
        let mut manager = TransactionManager::new();
        let records = vec![
            Record::new(OperationType::Deposit, 1, 1, Some(dec!(12).into())),
            Record::new(OperationType::Deposit, 1, 2, Some(dec!(5).into())),
            Record::new(OperationType::Dispute, 1, 1, None),
            Record::new(OperationType::Chargeback, 1, 1, None),
            Record::new(OperationType::Dispute, 1, 2, None),
            Record::new(OperationType::Chargeback, 1, 2, None),
        ];

        assert!(records.into_iter().all(|r| manager.parse_entry(&r).is_ok()));

        assert_eq!(manager.accounts.get(&1).unwrap().held(), dec!(0));
        assert_eq!(manager.accounts.get(&1).unwrap().available(), dec!(0));
    }

    #[test]
    fn test_chargeback_for_operation_that_was_not_under_dispute_shall_have_no_effect() {
        let mut manager = TransactionManager::new();
        let records = vec![
            Record::new(OperationType::Deposit, 1, 1, Some(dec!(100.0).into())),
            Record::new(OperationType::Deposit, 1, 2, Some(dec!(20.0).into())),
            Record::new(OperationType::Deposit, 1, 3, Some(dec!(15.0).into())),
            Record::new(OperationType::Dispute, 1, 3, None), //blocks 15.0, available 120
            Record::new(OperationType::Chargeback, 1, 2, None), //#2 wasn't under dispute, no effect
        ];

        assert!(records.into_iter().all(|r| manager.parse_entry(&r).is_ok()));

        assert_eq!(manager.accounts.get(&1).unwrap().held(), dec!(15.0));
        assert_eq!(manager.accounts.get(&1).unwrap().available(), dec!(120.0));
    }

    #[test]
    fn test_chargeback_for_operation_that_is_not_under_dispute_shall_have_no_effect() {
        let mut manager = TransactionManager::new();
        let records = vec![
            Record::new(OperationType::Deposit, 1, 1, Some(dec!(1.234).into())),
            Record::new(OperationType::Chargeback, 1, 1, None),
        ];

        assert!(records.into_iter().all(|r| manager.parse_entry(&r).is_ok()));

        assert_eq!(manager.accounts.get(&1).unwrap().held(), dec!(0.0));
        assert!(!manager.accounts.get(&1).unwrap().is_locked());
        assert_eq!(manager.accounts.get(&1).unwrap().available(), dec!(1.234));
    }

    #[test]
    fn test_disputed_transaction_can_be_chargedback_only_once() {
        let mut manager = TransactionManager::new();
        let records = vec![
            Record::new(OperationType::Deposit, 1, 1, Some(dec!(100.0).into())),
            Record::new(OperationType::Deposit, 1, 2, Some(dec!(20.0).into())),
            Record::new(OperationType::Dispute, 1, 2, None),
            Record::new(OperationType::Chargeback, 1, 2, None),
            Record::new(OperationType::Dispute, 1, 2, None),
            Record::new(OperationType::Chargeback, 1, 2, None),
            Record::new(OperationType::Dispute, 1, 2, None),
            Record::new(OperationType::Chargeback, 1, 2, None),
        ];

        assert!(records.into_iter().all(|r| manager.parse_entry(&r).is_ok()));

        assert_eq!(manager.accounts.get(&1).unwrap().held(), dec!(0.0));
        assert!(manager.accounts.get(&1).unwrap().is_locked());
        assert_eq!(manager.accounts.get(&1).unwrap().available(), dec!(100.0));
    }

    #[test]
    fn test_resolve_for_operation_that_is_not_under_dispute_shall_have_no_effect() {
        let mut manager = TransactionManager::new();
        let records = vec![
            Record::new(OperationType::Deposit, 1, 1, Some(dec!(1.234).into())),
            Record::new(OperationType::Resolve, 1, 1, None),
        ];

        assert!(records.into_iter().all(|r| manager.parse_entry(&r).is_ok()));

        assert_eq!(manager.accounts.get(&1).unwrap().held(), dec!(0.0));
        assert_eq!(manager.accounts.get(&1).unwrap().available(), dec!(1.234));
    }

    #[test]
    fn test_resolve_for_operation_that_was_already_chargedbacked_shall_have_no_effect() {
        let mut manager = TransactionManager::new();
        let records = vec![
            Record::new(OperationType::Deposit, 1, 1, Some(dec!(0.234).into())),
            Record::new(OperationType::Deposit, 1, 2, Some(dec!(1.0).into())),
            Record::new(OperationType::Dispute, 1, 2, None),
            Record::new(OperationType::Chargeback, 1, 2, None),
            Record::new(OperationType::Resolve, 1, 2, None),
        ];

        assert!(records.into_iter().all(|r| manager.parse_entry(&r).is_ok()));

        assert_eq!(manager.accounts.get(&1).unwrap().held(), dec!(0.0));
        assert!(manager.accounts.get(&1).unwrap().is_locked());
        assert_eq!(manager.accounts.get(&1).unwrap().available(), dec!(0.234));
    }

    #[test]
    fn test_resolve_on_non_existing_transaction_has_no_effets() {
        let mut manager = TransactionManager::new();
        let records = vec![
            Record::new(OperationType::Deposit, 1, 1, Some(dec!(1).into())),
            Record::new(OperationType::Resolve, 1, 2, None),
        ];

        assert!(records.into_iter().all(|r| manager.parse_entry(&r).is_ok()));

        assert_eq!(manager.accounts.get(&1).unwrap().available(), dec!(1));
        assert_eq!(manager.accounts.get(&1).unwrap().held(), dec!(0.0));
    }

    #[test]
    fn test_chargeback_on_non_existing_transaction_has_no_effets() {
        let mut manager = TransactionManager::new();
        let records = vec![
            Record::new(OperationType::Deposit, 1, 1, Some(dec!(2).into())),
            Record::new(OperationType::Chargeback, 1, 3, None),
        ];

        assert!(records.into_iter().all(|r| manager.parse_entry(&r).is_ok()));

        assert_eq!(manager.accounts.get(&1).unwrap().available(), dec!(2));
        assert_eq!(manager.accounts.get(&1).unwrap().held(), dec!(0.0));
    }

    #[test]
    fn test_transaction_with_the_same_id_shall_be_rejected_and_error_shall_be_reported() {
        let mut manager = TransactionManager::new();
        let records = vec![
            Record::new(OperationType::Deposit, 1, 1, Some(dec!(2).into())),
            Record::new(OperationType::Deposit, 1, 1, Some(dec!(1).into())),
        ];

        assert!(manager.parse_entry(&records[0]).is_ok());
        assert!(matches!(
            manager.parse_entry(&records[1]),
            Err(Errors::TransactionIdAlreadyUsed(1, OperationType::Deposit))
        ));

        assert_eq!(manager.transactions.len(), 1);
        let _expected_transaction =
            TransactionRecord::new(OperationType::Deposit, Some(dec!(2).into()));
        assert!(matches!(
            manager.transactions.get(&1).unwrap(),
            _expected_transaction
        ));
    }

    #[test]
    fn test_for_unique_clients_repeated_transaction_id_shall_not_create_new_account() {
        let mut manager = TransactionManager::new();
        let records = vec![
            Record::new(OperationType::Deposit, 1, 1, Some(dec!(2).into())),
            Record::new(OperationType::Deposit, 2, 1, Some(dec!(1).into())),
        ];

        assert!(manager.parse_entry(&records[0]).is_ok());
        assert!(matches!(
            manager.parse_entry(&records[1]),
            Err(Errors::TransactionIdAlreadyUsed(1, OperationType::Deposit))
        ));

        assert_eq!(manager.transactions.len(), 1);
        let _expected_transaction =
            TransactionRecord::new(OperationType::Deposit, Some(dec!(2).into()));
        assert!(matches!(
            manager.transactions.get(&1).unwrap(),
            _expected_transaction
        ));
        let mut accounts_iter = manager.accounts().peekable();
        assert!(accounts_iter.peek().is_some());
        accounts_iter.next();
        assert!(accounts_iter.peek().is_none());
    }

    #[test]
    fn test_chargback_on_withdrawal() {
        let mut manager = TransactionManager::new();
        let records = vec![
            Record::new(OperationType::Deposit, 1, 1, Some(dec!(10).into())),
            Record::new(OperationType::Withdrawal, 1, 2, Some(dec!(3).into())),
            Record::new(OperationType::Dispute, 1, 2, None),
            Record::new(OperationType::Chargeback, 1, 2, None),
        ];
        assert!(records.iter().all(|r| manager.parse_entry(r).is_ok()));
        assert_eq!(manager.accounts.get(&1).unwrap().available(), dec!(10));
    }
}
