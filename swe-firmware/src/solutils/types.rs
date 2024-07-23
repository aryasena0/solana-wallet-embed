extern crate alloc;
use alloc::string::String;
use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct GetBalanceResponse {
    #[serde(skip)]
    pub jsonrpc: &'static str,
    pub result: GetBalanceResult,
    pub id: i64,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct GetBalanceResult {
    pub context: Context,
    pub value: i64,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct Context {
    #[serde(skip)]
    pub api_version: String,
    pub slot: i64,
}
