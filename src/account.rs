use crate::amount::Amount;
use anyhow::Result;
use rust_decimal::Decimal;
use serde::ser::SerializeStruct;
use serde::{Serialize, Serializer};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Errors {
    #[error("Account {0} is locked")]
    AccountLocked(u16),
    #[error("Not enough {0} funds available!")]
    InsuficientFunds(u16),
    #[error("Overflow occured in {0}")]
    FundsOverflow(u16),
}
#[derive(Debug, PartialEq, Serialize)]
enum AccountState {
    #[serde(rename = "true")]
    Locked,
    #[serde(rename = "false")]
    Unlocked,
}

impl Default for AccountState {
    fn default() -> Self {
        AccountState::Unlocked
    }
}

#[derive(Debug)]
pub struct Account {
    client_id: u16,
    //TODO: change decimal to Amount
    available: Decimal,
    held: Decimal,
    locked: AccountState,
}

impl Serialize for Account {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("Account", 5)?;
        state.serialize_field("client", &self.client_id)?;
        state.serialize_field("available", &self.available)?;
        state.serialize_field("held", &self.held)?;
        state.serialize_field("total", &(self.available + self.held))?;
        state.serialize_field("locked", &self.locked)?;
        state.end()
    }
}

impl Account {
    pub fn new(client_id: u16) -> Self {
        Self {
            client_id,
            available: Default::default(),
            held: Default::default(),
            locked: Default::default(),
        }
    }

    pub fn deposit(&mut self, amount: Amount) -> Result<(), Errors> {
        match self.locked {
            AccountState::Locked => Err(Errors::AccountLocked(self.client_id)),
            AccountState::Unlocked => {
                self.available = self
                    .available
                    .checked_add(*amount)
                    .ok_or(Errors::FundsOverflow(self.client_id))?;
                Ok(())
            }
        }
    }

    pub fn withdrawal(&mut self, amount: Amount) -> Result<(), Errors> {
        match self.locked {
            AccountState::Locked => Err(Errors::AccountLocked(self.client_id)),
            AccountState::Unlocked => {
                if self.available >= *amount {
                    self.available = self
                        .available
                        .checked_sub(*amount)
                        .ok_or(Errors::FundsOverflow(self.client_id))?;
                    Ok(())
                } else {
                    Err(Errors::InsuficientFunds(self.client_id))
                }
            }
        }
    }

    pub fn dispute(&mut self, amount: Amount) -> Result<(), Errors> {
        if self.available >= *amount {
            self.available = self
                .available
                .checked_sub(*amount)
                .ok_or(Errors::InsuficientFunds(self.client_id))?;
            self.held = self
                .held
                .checked_add(*amount)
                .ok_or(Errors::FundsOverflow(self.client_id))?;
            Ok(())
        } else {
            Err(Errors::InsuficientFunds(self.client_id))
        }
    }

    pub fn resolve(&mut self, amount: Amount) -> Result<(), Errors> {
        self.available = self
            .available
            .checked_add(*amount)
            .ok_or(Errors::FundsOverflow(self.client_id))?;
        self.held = self
            .held
            .checked_sub(*amount)
            .ok_or(Errors::FundsOverflow(self.client_id))?;
        Ok(())
    }

    pub fn chargeback(&mut self, amount: Amount) -> Result<(), Errors> {
        self.held = self
            .held
            .checked_sub(*amount)
            .ok_or(Errors::FundsOverflow(self.client_id))?;
        self.locked = AccountState::Locked;
        Ok(())
    }

    #[cfg(test)]
    pub fn available(&self) -> Decimal {
        self.available
    }

    #[cfg(test)]
    pub fn held(&self) -> Decimal {
        self.held
    }

    #[cfg(test)]
    pub fn is_locked(&self) -> bool {
        match self.locked {
            AccountState::Locked => true,
            AccountState::Unlocked => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn test_sanity_check_on_new_account() {
        let account = Account::new(1);
        assert_eq!(account.available, dec!(0.0));
        assert_eq!(account.held, dec!(0.0));
        assert_eq!(account.locked, AccountState::Unlocked);
    }

    #[test]
    fn test_deposit_to_acount() {
        let mut account = Account::new(1);
        assert_eq!(account.available, dec!(0.0));
        assert!(account.deposit(Amount(dec!(1.0))).is_ok());
        assert_eq!(account.available, dec!(1.0));
    }

    #[test]
    fn test_withdrawal_from_account_with_sufficient_amount() {
        let mut account = Account::new(1);
        assert!(account.deposit(Amount(dec!(100.0))).is_ok());
        assert!(account.withdrawal(Amount(dec!(99.5))).is_ok());
        assert_eq!(account.available, dec!(0.5));
    }

    #[test]
    fn test_withdrawal_from_account_with_insufficient_amount() {
        let mut account = Account::new(1);
        assert!(account.deposit(Amount(dec!(100.0))).is_ok());
        assert!(matches!(
            account.withdrawal(Amount(dec!(200.0))),
            Err(Errors::InsuficientFunds(1))
        ));
        assert_eq!(account.available, dec!(100.0));
    }

    #[test]
    fn test_withdrawal_from_account_with_zero_funds() {
        let mut account = Account::new(123);
        assert!(matches!(
            account.withdrawal(Amount(dec!(42.0))),
            Err(Errors::InsuficientFunds(123))
        ));
        assert_eq!(account.available, dec!(0.0));
    }

    #[test]
    fn test_dispute_to_account() {
        let mut account = Account::new(1);
        assert!(account.deposit(Amount(dec!(100.0))).is_ok());
        assert!(account.dispute(Amount(dec!(10.0))).is_ok());
        assert_eq!(account.available, dec!(90.0));
        assert_eq!(account.held, dec!(10.0));
    }

    #[test]
    fn test_dispute_to_account_with_not_enough_funds() {
        let mut account = Account::new(1);
        assert!(matches!(
            account.dispute(Amount(dec!(10.0))),
            Err(Errors::InsuficientFunds(1))
        ));
        assert_eq!(account.held, dec!(0.0));
    }

    #[test]
    fn test_chargeback_locks_account_and_reduces_available_funds() {
        let mut account = Account::new(1);
        let held_amount = dec!(10.0);

        assert!(account.deposit(Amount(dec!(100.0))).is_ok());
        assert!(account.dispute(Amount(held_amount)).is_ok());

        assert_eq!(account.held, held_amount);
        assert_eq!(account.available, dec!(100.0) - held_amount);

        assert!(account.chargeback(Amount(held_amount)).is_ok());

        assert_eq!(account.available, dec!(100.0) - held_amount);
        assert_eq!(account.held, dec!(0.0));
        assert_eq!(account.locked, AccountState::Locked);
    }

    #[test]
    fn test_resolve_frees_held_amount() {
        let mut account = Account::new(1);
        assert!(account.deposit(Amount(dec!(10.0))).is_ok());
        assert!(account.dispute(Amount(dec!(5.0))).is_ok());
        assert!(account.resolve(Amount(dec!(5.0))).is_ok());
        assert_eq!(account.available, dec!(10.0));
    }

    #[test]
    fn test_deposit_on_locked_account() {
        let mut account = Account::new(1);
        assert!(account.deposit(Amount(dec!(10.0))).is_ok());

        assert!(account.chargeback(Amount(dec!(5.0))).is_ok());

        assert!(matches!(
            account.deposit(Amount(dec!(5.0))),
            Err(Errors::AccountLocked(1))
        ));
    }
}
