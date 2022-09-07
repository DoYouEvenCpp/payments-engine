use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use std::ops::Deref;

#[derive(PartialEq, Eq, Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct Amount(pub Decimal);

// just to ease usage of Amount acros other components
impl Deref for Amount {
    type Target = Decimal;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<Decimal> for Amount {
    fn from(d: Decimal) -> Self {
        Amount(d)
    }
}
