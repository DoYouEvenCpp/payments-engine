use thiserror::Error;

use crate::record::OperationType;

#[derive(Error, Debug)]
pub enum Errors {
    #[error("Account {0} is locked")]
    AccountLocked(u16),
    #[error("Not enough funds available for account {0}")]
    InsuficientFunds(u16),
    #[error("Overflow occured in account {0}")]
    FundsOverflow(u16),
    #[error("Missing amount for the operation")]
    MissingAmount,
    #[error("Negative amount")]
    NegativeAmount,
    #[error("Resolve requested to a non dispute operation")]
    ResolveOnNonDisputeOperation,
    #[error("Transaction ID {0} already taken in operation {1}")]
    TransactionIdAlreadyUsed(u32, OperationType),
}
