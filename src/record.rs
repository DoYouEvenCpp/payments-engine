use crate::amount::Amount;
use serde::Deserialize;

#[derive(Copy, Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum OperationType {
    Chargeback,
    Dispute,
    Deposit,
    Resolve,
    Withdrawal,
}

#[derive(Debug, Deserialize)]
pub struct Record {
    pub r#type: OperationType,
    pub client: u16,
    pub tx: u32,
    pub amount: Option<Amount>,
}
