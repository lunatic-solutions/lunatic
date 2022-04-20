use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Request {
    Spawn,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Response {
    Spawned,
    Linked,
}
