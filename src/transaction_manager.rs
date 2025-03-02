use crate::{
    account::Account, amount::Amount, error::Errors, record::OperationType, record::Record,
};
use anyhow::Result;
use std::collections::HashMap;

/// Holds details for a single transaction.
///
/// This record tracks the type of operation (deposit, withdrawal, etc.),
/// the associated amount (if any), and flags to correctly handle dispute-related logic.
#[derive(Debug)]
struct TransactionRecord {
    /// The type of the operation.
    operation_type: OperationType,
    /// The amount involved in the transaction (if applicable).
    amount: Option<Amount>,
    /// Flag indicating if the transaction is currently under dispute.
    under_dispute: bool,
    /// Flag indicating if the transaction has already been disputed.
    already_disputed: bool,
}

impl TransactionRecord {
    /// Constructs a new `TransactionRecord`.
    ///
    /// # Arguments
    ///
    /// * `operation_type` - The type of operation for the transaction.
    /// * `amount` - Optional amount.
    ///
    /// # Returns
    ///
    /// A new `TransactionRecord` with dispute flags set to false.
    fn new(operation_type: OperationType, amount: Option<Amount>) -> Self {
        Self {
            operation_type,
            amount,
            under_dispute: false,
            already_disputed: false,
        }
    }
}

/// Type alias for mapping client IDs to their respective accounts.
type Accounts = HashMap<u16, Account>;
/// Type alias for mapping transaction IDs to their corresponding records.
type Transactions = HashMap<u32, TransactionRecord>;

/// Provides business logic for this toy payments-engine. Controls overal flow over different types of transactions.
///
/// Internally contains only two collections: accounts and transactions.
/// Provides implementation to properly handle different type of operations.
#[derive(Debug)]
pub struct TransactionManager {
    accounts: Accounts,
    transactions: Transactions,
}

impl TransactionManager {
    /// Creates a new `TransactionManager` instance with empty account and transaction records.
    pub fn new() -> Self {
        Self {
            accounts: Accounts::new(),
            transactions: Transactions::new(),
        }
    }

    /// Helper method to get a handler to an account (creating one if needed at first).
    ///
    /// # Arguments
    ///
    /// * `cliend_id` - The unique identifier of the client.
    ///
    /// # Returns
    ///
    /// A mutable reference to the client's `Account`.
    fn get_account(&mut self, cliend_id: u16) -> &mut Account {
        self.accounts
            .entry(cliend_id)
            .or_insert_with(|| Account::new(cliend_id))
    }

    /// Parses a transaction record and updates the internal state accordingly - the very core of the TransactionManager.
    ///
    /// The function handles different operation types:
    ///
    /// - **Deposit/Withdrawal:** Validates the record, ensures the transaction ID is unique,
    ///   inserts the record, and updates the account balance.
    /// - **Dispute:** Flags a transaction as disputed and adjusts account funds if applicable.
    /// - **Resolve:** Reverses a dispute, moving held funds back to available funds.
    /// - **Chargeback:** Finalizes a dispute by removing held funds and locking the account.
    ///
    /// There is a few assumptions:
    /// - transaction id must be unique, otherwise an error is reported
    /// - if the transaction provided to Deposit or Withdrawal operation is negative, then an error is reported
    /// - if there is no amount provided to Deposit or Withdrawal operation, then an error is reported
    /// - Resolve operation must be referenced to a dispute call, otherwise an error is reported
    /// - Chargeback operations run only over already disputed amounts (eg. there must be a precedeing Dispute operation)
    /// - Chargeback operation logic differs for referenced Deposit or Withdrawal operations
    /// - A single operation could be disputed only once, next are silently ignored
    ///
    ///
    /// # Arguments
    ///
    /// * `record` - A reference to the incoming transaction `Record`.
    ///
    /// # Errors
    ///
    /// See assumptions.
    ///
    pub fn parse_entry(&mut self, record: &Record) -> Result<(), Errors> {
        // Keep track only of transactions that are deposits or withdrawals.
        // Dispute/resolve/chargeback entries for non-existing operations are dropped.
        match record.r#type {
            OperationType::Deposit => {
                if self.transactions.contains_key(&record.tx) {
                    return Err(Errors::TransactionIdAlreadyUsed(record.tx, record.r#type));
                }
                match record.amount {
                    Some(amount) => {
                        if amount.is_sign_negative() {
                            return Err(Errors::NegativeAmount);
                        }
                        self.transactions.insert(
                            record.tx,
                            TransactionRecord::new(record.r#type, record.amount),
                        );
                        self.get_account(record.client).deposit(amount)?;
                    }
                    None => return Err(Errors::MissingAmount),
                }
            }
            OperationType::Withdrawal => {
                if self.transactions.contains_key(&record.tx) {
                    return Err(Errors::TransactionIdAlreadyUsed(record.tx, record.r#type));
                }
                match record.amount {
                    Some(amount) => {
                        if amount.is_sign_negative() {
                            return Err(Errors::NegativeAmount);
                        }
                        self.transactions.insert(
                            record.tx,
                            TransactionRecord::new(record.r#type, record.amount),
                        );
                        self.get_account(record.client).withdrawal(amount)?;
                    }
                    None => return Err(Errors::MissingAmount),
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
                    if !transaction.under_dispute {
                        return Err(Errors::ResolveOnNonDisputeOperation);
                    }
                    transaction.under_dispute = false;
                    if let Some(amount) = transaction.amount {
                        self.get_account(record.client).resolve(amount)?;
                    }
                }
            }
        }
        Ok(())
    }

    /// Convience method, that returns iterator over accounts.
    ///
    pub fn accounts(&self) -> impl Iterator<Item = &Account> {
        self.accounts.values()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::amount::Amount;
    use rust_decimal_macros::dec;

    /// Helper implementation for tests to create new transaction records.
    impl Record {
        /// Creates a new `Record` instance.
        ///
        /// # Arguments
        ///
        /// * `r#type` - The type of operation.
        /// * `client` - The client identifier.
        /// * `tx` - The transaction identifier.
        /// * `amount` - The amount involved (if any).
        pub fn new(r#type: OperationType, client: u16, tx: u32, amount: Option<Amount>) -> Self {
            Self {
                r#type,
                client,
                tx,
                amount,
            }
        }
    }

    // The following tests cover various scenarios including disputes, chargebacks,
    // resolving disputes, duplicate transaction IDs, and handling negative amounts.

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
            Record::new(OperationType::Dispute, 1, 3, None), // Blocks 15.0, available becomes 120
            Record::new(OperationType::Chargeback, 1, 2, None), // Transaction #2 wasn't under dispute
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
    fn test_resolve_on_non_dispute_transaction_shall_be_ignroed() {
        let mut manager = TransactionManager::new();
        let records = vec![
            Record::new(OperationType::Deposit, 1, 1, Some(dec!(1).into())),
            Record::new(OperationType::Withdrawal, 1, 2, Some(dec!(0.5).into())),
            Record::new(OperationType::Resolve, 1, 2, None),
        ];
        assert!(manager.parse_entry(&records[0]).is_ok());
        assert!(manager.parse_entry(&records[1]).is_ok());
        assert!(matches!(
            manager.parse_entry(&records[2]),
            Err(Errors::ResolveOnNonDisputeOperation)
        ));
    }

    #[test]
    fn test_resolve_for_operation_is_not_under_dispute_shall_have_no_effect() {
        let mut manager = TransactionManager::new();
        let records = vec![
            Record::new(OperationType::Deposit, 1, 1, Some(dec!(0.234).into())),
            Record::new(OperationType::Deposit, 1, 2, Some(dec!(1.0).into())),
            Record::new(OperationType::Dispute, 1, 2, None),
            Record::new(OperationType::Chargeback, 1, 2, None),
            Record::new(OperationType::Resolve, 1, 2, None),
        ];

        assert!(manager.parse_entry(&records[0]).is_ok());
        assert!(manager.parse_entry(&records[1]).is_ok());
        assert!(manager.parse_entry(&records[2]).is_ok());
        assert!(manager.parse_entry(&records[3]).is_ok());

        assert_eq!(manager.accounts.get(&1).unwrap().held(), dec!(0.0));
        assert!(manager.accounts.get(&1).unwrap().is_locked());
        assert_eq!(manager.accounts.get(&1).unwrap().available(), dec!(0.234));

        assert!(manager.parse_entry(&records[4]).is_err());

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

    #[test]
    fn test_operation_with_negative_amount_shall_be_discared() {
        let mut manager = TransactionManager::new();
        let records = vec![
            Record::new(OperationType::Deposit, 1, 1, Some(dec!(-1).into())),
            Record::new(OperationType::Withdrawal, 1, 2, Some(dec!(-1).into())),
        ];
        assert!(records.iter().all(|r| manager.parse_entry(r).is_err()));
        assert!(manager.accounts.is_empty());
        assert!(manager.transactions.is_empty());
    }

    #[test]
    fn test_invalid_deposit_or_withdrawal_operation_shall_not_affect_internal_state_of_manager() {
        let mut manager = TransactionManager::new();
        let records = vec![
            Record::new(OperationType::Deposit, 1, 1, Some(dec!(-150).into())),
            Record::new(OperationType::Withdrawal, 1, 2, Some(dec!(-42).into())),
            Record::new(OperationType::Deposit, 1, 3, None),
            Record::new(OperationType::Withdrawal, 1, 4, None),
        ];

        assert!(manager.accounts.is_empty());
        assert!(manager.transactions.is_empty());

        assert!(records.iter().all(|r| manager.parse_entry(r).is_err()));

        assert!(manager.accounts.is_empty());
        assert!(manager.transactions.is_empty());
    }

    #[test]
    fn test_with_multiple_disputes_resolve_shall_release_held_amount_only_from_referenced_operation(
    ) {
        let mut manager = TransactionManager::new();
        let records = vec![
            Record::new(OperationType::Deposit, 1, 1, Some(dec!(10).into())),
            Record::new(OperationType::Deposit, 1, 2, Some(dec!(20).into())),
            Record::new(OperationType::Deposit, 1, 3, Some(dec!(30).into())),
            Record::new(OperationType::Deposit, 1, 4, Some(dec!(40).into())),
            Record::new(OperationType::Dispute, 1, 1, None),
            Record::new(OperationType::Dispute, 1, 2, None),
            Record::new(OperationType::Resolve, 1, 2, None),
            Record::new(OperationType::Dispute, 1, 3, None),
        ];

        records.iter().for_each(|r| {
            let _ = manager.parse_entry(r);
        });

        assert_eq!(manager.accounts.get(&1).unwrap().available(), dec!(60));
        assert_eq!(manager.accounts.get(&1).unwrap().held(), dec!(40));
    }

    #[test]
    fn test_with_multiple_clients() {
        let mut manager = TransactionManager::new();
        let records = vec![
            Record::new(OperationType::Deposit, 1, 1, Some(dec!(10).into())),
            Record::new(OperationType::Deposit, 2, 2, Some(dec!(20).into())),
            Record::new(OperationType::Deposit, 3, 3, Some(dec!(30).into())),
            Record::new(OperationType::Deposit, 2, 4, Some(dec!(40).into())),
            Record::new(OperationType::Deposit, 3, 5, Some(dec!(50).into())),
            Record::new(OperationType::Deposit, 1, 6, Some(dec!(60).into())),
            Record::new(OperationType::Deposit, 2, 7, Some(dec!(70).into())),
            Record::new(OperationType::Deposit, 3, 8, Some(dec!(80).into())),
            Record::new(OperationType::Dispute, 2, 4, None),
            Record::new(OperationType::Withdrawal, 1, 9, Some(dec!(1).into())),
            Record::new(OperationType::Dispute, 3, 8, None),
            Record::new(OperationType::Chargeback, 3, 8, None),
        ];

        records.iter().for_each(|r| {
            assert!(manager.parse_entry(r).is_ok());
        });

        assert_eq!(manager.accounts.len(), 3);

        let account_1 = manager.accounts.get(&1).unwrap();
        let account_2 = manager.accounts.get(&2).unwrap();
        let account_3 = manager.accounts.get(&3).unwrap();

        assert_eq!(account_1.is_locked(), false);
        assert_eq!(account_1.available(), dec!(69));
        assert_eq!(account_1.held(), dec!(0));

        assert_eq!(account_2.is_locked(), false);
        assert_eq!(account_2.available(), dec!(90));
        assert_eq!(account_2.held(), dec!(40));

        assert_eq!(account_3.is_locked(), true);
        assert_eq!(account_3.available(), dec!(80));
        assert_eq!(account_3.held(), dec!(0));
    }
}
