use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct App {
    pub app_id: i64,
    pub project_id: i64,
    pub name: String,
}
