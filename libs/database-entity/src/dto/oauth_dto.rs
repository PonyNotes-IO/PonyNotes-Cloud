use serde::{Deserialize, Serialize};

/// 抖音OAuth请求结构
#[derive(Debug, Serialize, Deserialize)]
pub struct DouyinOAuthRequest {
    pub code: String,
    pub state: String,
}

/// 抖音OAuth响应结构
#[derive(Debug, Serialize, Deserialize)]
pub struct DouyinOAuthResponse {
    pub auth_url: String,
    pub state: String,
    pub expires_in: i64,
}

/// 通用OAuth用户信息
#[derive(Debug, Serialize, Deserialize)]
pub struct OAuthUserInfo {
    pub provider: String,
    pub provider_id: String,
    pub email: Option<String>,
    pub name: String,
    pub avatar_url: Option<String>,
    pub raw_data: serde_json::Value,
}
