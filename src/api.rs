use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct CreateJob {
    pub image: Option<String>,
    pub args: Vec<String>,
    pub code: String,
}
