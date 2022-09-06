use crate::amount::Amount;
use serde::Deserialize;
use std::fmt;
//TODO: fix structs visibility(?)
#[derive(Copy, Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum OperationType {
    Chargeback,
    Dispute,
    Deposit,
    Resolve,
    Withdrawal,
}

impl fmt::Display for OperationType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                OperationType::Chargeback => "Chargeback",
                OperationType::Dispute => "Dispute",
                OperationType::Deposit => "Deposit",
                OperationType::Resolve => "Resolve",
                OperationType::Withdrawal => "Withdrawal",
            }
        )
    }
}

#[derive(Debug, Deserialize)]
pub struct Record {
    pub r#type: OperationType,
    pub client: u16,
    pub tx: u32,
    pub amount: Option<Amount>,
}
