use anyhow::Result;
use serde::ser::SerializeStruct;
use serde::{Serialize, Serializer};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Errors {
    #[error("Account {0} is locked")]
    AccountLocked(u16),
    #[error("Not enough {0} funds available!")]
    InsuficientFunds(u16),
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
    // TODO: wrap types into structures
    // TODO: do i need client_id in Account?
    client_id: u16,
    available: f32,
    held: f32,
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

// TODO: add error handling
// TODO: verify overflows/floating arithmetics
impl Account {
    pub fn new(client_id: u16) -> Self {
        Self {
            client_id,
            available: Default::default(),
            held: Default::default(),
            locked: Default::default(),
        }
    }

    pub fn deposit(&mut self, amount: f32) -> Result<(), Errors> {
        match self.locked {
            AccountState::Locked => Err(Errors::AccountLocked(self.client_id)),
            AccountState::Unlocked => {
                self.available += amount;
                Ok(())
            }
        }
    }

    pub fn withdrawal(&mut self, amount: f32) -> Result<(), Errors> {
        match self.locked {
            AccountState::Locked => Err(Errors::AccountLocked(self.client_id)),
            AccountState::Unlocked => {
                if self.available < amount {
                    Err(Errors::InsuficientFunds(self.client_id))
                } else {
                    self.available -= amount;
                    Ok(())
                }
            }
        }
    }

    pub fn dispute(&mut self, amount: f32) -> Result<(), Errors> {
        if self.available - amount >= 0.0 {
            self.available -= amount;
            self.held += amount;
        }
        Ok(())
    }

    pub fn resolve(&mut self, amount: f32) -> Result<(), Errors> {
        self.available += amount;
        self.held -= amount;
        Ok(())
    }

    pub fn charbegack(&mut self, amount: f32) -> Result<(), Errors> {
        self.held -= amount;
        self.locked = AccountState::Locked;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanity_check_on_new_account() {
        let account = Account::new(1);
        assert_eq!(account.available, 0.0);
        assert_eq!(account.held, 0.0);
        assert_eq!(account.locked, AccountState::Unlocked);
    }

    #[test]
    fn test_deposit_to_acount() {
        let mut account = Account::new(1);
        assert_eq!(account.available, 0.0);
        assert!(account.deposit(1.0).is_ok());
        assert_eq!(account.available, 1.0);
    }

    #[test]
    fn test_withdrawal_from_account_with_sufficient_amounts() {
        let mut account = Account::new(1);
        assert!(account.deposit(100.0).is_ok());
        assert!(account.withdrawal(99.5).is_ok());
        assert_eq!(account.available, 0.5);
    }

    #[test]
    fn test_withdrawal_from_account_with_insufficient_amounts() {
        let mut account = Account::new(1);
        assert!(account.deposit(100.0).is_ok());
        assert!(matches!(
            account.withdrawal(200.0),
            Err(Errors::InsuficientFunds(1))
        ));
        assert_eq!(account.available, 100.0);
    }

    #[test]
    fn test_withdrawal_from_account_with_zero_funds() {
        let mut account = Account::new(123);
        assert!(matches!(
            account.withdrawal(42.0),
            Err(Errors::InsuficientFunds(123))
        ));
        assert_eq!(account.available, 0.0);
    }

    #[test]
    fn test_dispute_to_account() {
        let mut account = Account::new(1);
        assert!(account.deposit(100.0).is_ok());
        assert!(account.dispute(10.0).is_ok());
        assert_eq!(account.available, 90.0);
        assert_eq!(account.held, 10.0);
    }

    #[test]
    fn test_dispute_to_account_with_not_enough_funds() {
        let mut account = Account::new(1);
        assert!(account.dispute(10.0).is_ok());
        assert_eq!(account.held, 0.0);
    }

    #[test]
    fn test_chargeback_locks_account_and_reduces_available_funds() {
        let mut account = Account::new(1);
        let held_amount = 10.0f32;

        assert!(account.deposit(100.0).is_ok());
        assert!(account.dispute(held_amount).is_ok());

        assert_eq!(account.held, held_amount);
        assert_eq!(account.available, 100.0 - held_amount);

        assert!(account.charbegack(held_amount).is_ok());

        assert_eq!(account.available, 100.0 - held_amount);
        assert_eq!(account.held, 0.0);
        assert_eq!(account.locked, AccountState::Locked);
    }

    #[test]
    fn test_resolve_frees_held_amount() {
        let mut account = Account::new(1);
        assert!(account.deposit(10.0).is_ok());
        assert!(account.dispute(5.0).is_ok());
        assert!(account.resolve(5.0).is_ok());
        assert_eq!(account.available, 10.0);
    }

    #[test]
    fn test_deposit_on_locked_account() {
        let mut account = Account::new(1);
        assert!(account.deposit(10.0).is_ok());

        assert!(account.charbegack(5.0).is_ok());

        assert!(matches!(
            account.deposit(5.0),
            Err(Errors::AccountLocked(1))
        ));
    }
}
