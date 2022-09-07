use crate::amount::Amount;
use crate::error::Errors;
use anyhow::Result;
use rust_decimal::Decimal;
use serde::ser::SerializeStruct;
use serde::{Serialize, Serializer};

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
        state.serialize_field("available", &format!("{:.04}", self.available.round_dp(4)))?;
        state.serialize_field("held", &format!("{:.04}", self.held.round_dp(4)))?;
        state.serialize_field(
            "total",
            &format!("{:.04}", (self.available + self.held).round_dp(4)),
        )?;
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
        assert!(account.deposit(dec!(1.0).into()).is_ok());
        assert_eq!(account.available, dec!(1.0));
    }

    #[test]
    fn test_withdrawal_from_account_with_sufficient_amount() {
        let mut account = Account::new(1);
        assert!(account.deposit(dec!(100.0).into()).is_ok());
        assert!(account.withdrawal(dec!(99.5).into()).is_ok());
        assert_eq!(account.available, dec!(0.5));
    }

    #[test]
    fn test_withdrawal_from_account_with_insufficient_amount() {
        let mut account = Account::new(1);
        assert!(account.deposit(dec!(100.0).into()).is_ok());
        assert!(matches!(
            account.withdrawal(dec!(200.0).into()),
            Err(Errors::InsuficientFunds(1))
        ));
        assert_eq!(account.available, dec!(100.0));
    }

    #[test]
    fn test_withdrawal_from_account_with_zero_funds() {
        let mut account = Account::new(123);
        assert!(matches!(
            account.withdrawal(dec!(42.0).into()),
            Err(Errors::InsuficientFunds(123))
        ));
        assert_eq!(account.available, dec!(0.0));
    }

    #[test]
    fn test_dispute_to_account() {
        let mut account = Account::new(1);
        assert!(account.deposit(dec!(100.0).into()).is_ok());
        assert!(account.dispute(dec!(10.0).into()).is_ok());
        assert_eq!(account.available, dec!(90.0));
        assert_eq!(account.held, dec!(10.0));
    }

    #[test]
    fn test_dispute_to_account_with_not_enough_funds() {
        let mut account = Account::new(1);
        assert!(matches!(
            account.dispute(dec!(10.0).into()),
            Err(Errors::InsuficientFunds(1))
        ));
        assert_eq!(account.held, dec!(0.0));
    }

    #[test]
    fn test_chargeback_locks_account_and_reduces_available_funds() {
        let mut account = Account::new(1);
        let held_amount = dec!(10.0);

        assert!(account.deposit(dec!(100.0).into()).is_ok());
        assert!(account.dispute(held_amount.into()).is_ok());

        assert_eq!(account.held, held_amount);
        assert_eq!(account.available, dec!(100.0) - held_amount);

        assert!(account.chargeback(held_amount.into()).is_ok());

        assert_eq!(account.available, dec!(100.0) - held_amount);
        assert_eq!(account.held, dec!(0.0));
        assert_eq!(account.locked, AccountState::Locked);
    }

    #[test]
    fn test_resolve_frees_held_amount() {
        let mut account = Account::new(1);
        assert!(account.deposit(dec!(10.0).into()).is_ok());
        assert!(account.dispute(dec!(5.0).into()).is_ok());
        assert!(account.resolve(dec!(5.0).into()).is_ok());
        assert_eq!(account.available, dec!(10.0));
    }

    #[test]
    fn test_deposit_on_locked_account() {
        let mut account = Account::new(1);
        account.available = dec!(10.0);
        account.locked = AccountState::Locked;

        assert!(matches!(
            account.deposit(dec!(5.0).into()),
            Err(Errors::AccountLocked(1))
        ));
    }

    #[test]
    fn test_deposit_fails_due_overflow() {
        let mut account = Account::new(1);
        account.held = Decimal::MAX;
        account.available = Decimal::MAX;

        assert!(matches!(
            account.deposit(Decimal::MAX.into()),
            Err(Errors::FundsOverflow(1))
        ));
    }

    #[test]
    fn test_withdrawal_fails_due_overflow() {
        let mut account = Account::new(1);
        account.held = Decimal::MAX;
        account.available = Decimal::MAX;

        assert!(matches!(
            account.withdrawal(Decimal::MIN.into()),
            Err(Errors::FundsOverflow(1))
        ));
    }

    #[test]
    fn test_dispute_fails_due_overflow() {
        let mut account = Account::new(1);

        account.held = Decimal::MAX;
        account.available = Decimal::MAX;

        assert!(matches!(
            account.dispute(Decimal::MAX.into()),
            Err(Errors::FundsOverflow(1))
        ));
    }

    #[test]
    fn test_chargeback_fails_due_overflow() {
        let mut account = Account::new(1);

        account.held = Decimal::MIN;
        account.available = Decimal::MIN;

        assert!(matches!(
            account.chargeback(Decimal::MAX.into()),
            Err(Errors::FundsOverflow(1))
        ));
    }

    #[test]
    fn test_resolve_fails_due_overflow() {
        let mut account = Account::new(1);
        account.held = Decimal::MAX;
        account.available = Decimal::MAX;
        assert!(matches!(
            account.resolve(Decimal::MIN.into()),
            Err(Errors::FundsOverflow(1))
        ));
    }
}
