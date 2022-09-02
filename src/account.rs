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

    pub fn deposit(&mut self, amount: f32) {
        if self.locked == AccountState::Unlocked {
            self.available += amount;
        }
    }

    pub fn withdrawal(&mut self, amount: f32) {
        if self.locked == AccountState::Unlocked {
            if self.available >= amount {
                self.available -= amount;
            }
        }
    }
    pub fn dispute(&mut self, amount: f32) {
        self.available -= amount;
        self.held += amount;
    }
    pub fn resolve(&mut self, amount: f32) {
        //find specific transaction, if exists!
        self.available += amount;
        self.held -= amount;
        self.unlock_account();
        // shall it happen?
    }
    pub fn charbegack(&mut self, amount: f32) {
        //find specific transaction, if exists!
        self.held -= amount;
        self.available -= amount;
        self.lock_account();
    }

    pub fn lock_account(&mut self) {
        self.locked = AccountState::Locked;
    }

    pub fn unlock_account(&mut self) {
        self.locked = AccountState::Unlocked;
    }
}
