use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct App {
    pub app_id: i64,
    pub project_id: i64,
    pub name: String,
}

#[derive(Deserialize, Serialize, Debug)]
pub struct SaveApp {
    pub name: String,
}

#[derive(Deserialize, Serialize, Debug)]
pub struct AppVersion {
    pub app_version_id: i64,
    pub project_id: i64,
    pub app_id: i64,
    pub is_valid: bool,
    pub validated_at: Option<String>,
    pub user_id: Option<i64>,
    pub created_at: String,
}
