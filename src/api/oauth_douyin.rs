use crate::api::util::get_user_uuid_from_headers;
use actix_web::{web, HttpRequest, HttpResponse, Result};
use app_error::AppError;
use database_entity::dto::oauth_dto::{DouyinOAuthRequest, DouyinOAuthResponse};
use reqwest;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 抖音OAuth登录相关的API端点

/// 抖音用户信息结构
#[derive(Debug, Serialize, Deserialize)]
pub struct DouyinUserInfo {
    pub open_id: String,
    pub union_id: Option<String>,
    pub nickname: String,
    pub avatar: Option<String>,
    pub gender: Option<i32>,
    pub city: Option<String>,
    pub province: Option<String>,
    pub country: Option<String>,
}

/// 抖音访问令牌响应
#[derive(Debug, Serialize, Deserialize)]
pub struct DouyinTokenResponse {
    pub access_token: String,
    pub expires_in: i64,
    pub refresh_token: String,
    pub open_id: String,
    pub scope: String,
}

/// 生成抖音登录二维码
/// 
/// POST /api/auth/douyin/qrcode
pub async fn generate_douyin_qrcode(
    _req: HttpRequest,
) -> Result<HttpResponse, AppError> {
    // 生成随机状态码
    let state = generate_random_state();
    
    // 构建抖音授权URL
    let client_key = std::env::var("DOUYIN_CLIENT_KEY")
        .unwrap_or_else(|_| "aws8ujfhmwybxv72".to_string());
    let redirect_uri = std::env::var("DOUYIN_REDIRECT_URI")
        .unwrap_or_else(|_| "https://your-app.com/auth/douyin/callback".to_string());
    
    let auth_url = format!(
        "https://open.douyin.com/platform/oauth/connect/?client_key={}&response_type=code&scope=user_info&redirect_uri={}&state={}",
        client_key,
        urlencoding::encode(&redirect_uri),
        state
    );
    
    let response = DouyinOAuthResponse {
        auth_url,
        state,
        expires_in: 300, // 5分钟过期
    };
    
    Ok(HttpResponse::Ok().json(response))
}

/// 检查抖音登录状态
/// 
/// GET /api/auth/douyin/status?state={state}
pub async fn check_douyin_login_status(
    _req: HttpRequest,
    query: web::Query<HashMap<String, String>>,
) -> Result<HttpResponse, AppError> {
    let state = query.get("state").ok_or_else(|| {
        AppError::InvalidRequest("Missing state parameter".to_string())
    })?;
    
    // 这里应该检查数据库中的登录状态
    // 暂时返回等待状态
    let response = serde_json::json!({
        "status": "waiting",
        "message": "等待用户扫码授权",
        "state": state
    });
    
    Ok(HttpResponse::Ok().json(response))
}

/// 处理抖音OAuth回调
/// 
/// POST /api/auth/douyin/callback
pub async fn handle_douyin_callback(
    _req: HttpRequest,
    payload: web::Json<DouyinOAuthRequest>,
) -> Result<HttpResponse, AppError> {
    let code = &payload.code;
    let state = &payload.state;
    
    // 验证state参数（实际实现中应该验证state的有效性）
    
    // 使用授权码获取访问令牌
    let token_response = exchange_code_for_token(code).await?;
    
    // 使用访问令牌获取用户信息
    let user_info = get_douyin_user_info(&token_response.access_token, &token_response.open_id).await?;
    
    // 这里应该创建或更新用户账户
    // 暂时返回模拟响应
    let response = serde_json::json!({
        "success": true,
        "user": {
            "open_id": user_info.open_id,
            "nickname": user_info.nickname,
            "avatar": user_info.avatar,
        },
        "access_token": token_response.access_token,
        "expires_in": token_response.expires_in
    });
    
    Ok(HttpResponse::Ok().json(response))
}

/// 获取抖音访问令牌
/// 
/// POST /api/auth/douyin/token
pub async fn get_douyin_access_token(
    _req: HttpRequest,
    payload: web::Json<serde_json::Value>,
) -> Result<HttpResponse, AppError> {
    let code = payload.get("code")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::InvalidRequest("Missing code parameter".to_string()))?;
    
    let token_response = exchange_code_for_token(code).await?;
    
    Ok(HttpResponse::Ok().json(token_response))
}

/// 获取抖音用户信息
/// 
/// POST /api/auth/douyin/userinfo
pub async fn get_douyin_user_info_api(
    _req: HttpRequest,
    payload: web::Json<serde_json::Value>,
) -> Result<HttpResponse, AppError> {
    let access_token = payload.get("access_token")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::InvalidRequest("Missing access_token parameter".to_string()))?;
    
    let open_id = payload.get("open_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::InvalidRequest("Missing open_id parameter".to_string()))?;
    
    let user_info = get_douyin_user_info(access_token, open_id).await?;
    
    Ok(HttpResponse::Ok().json(user_info))
}

/// 使用授权码交换访问令牌
async fn exchange_code_for_token(code: &str) -> Result<DouyinTokenResponse, AppError> {
    let client_key = std::env::var("DOUYIN_CLIENT_KEY")
        .unwrap_or_else(|_| "aws8ujfhmwybxv72".to_string());
    let client_secret = std::env::var("DOUYIN_CLIENT_SECRET")
        .unwrap_or_else(|_| "5a4aea1685c0ba05b7d22b6c2372cc47".to_string());
    
    let client = reqwest::Client::new();
    let mut params = HashMap::new();
    params.insert("client_key", client_key);
    params.insert("client_secret", client_secret);
    params.insert("code", code.to_string());
    params.insert("grant_type", "authorization_code".to_string());
    
    let response = client
        .post("https://open.douyin.com/oauth/access_token/")
        .json(&params)
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("Failed to request Douyin token: {}", e)))?;
    
    let token_data: serde_json::Value = response
        .json()
        .await
        .map_err(|e| AppError::Internal(format!("Failed to parse Douyin token response: {}", e)))?;
    
    // 检查是否有错误
    if let Some(error) = token_data.get("error") {
        return Err(AppError::Internal(format!("Douyin API error: {}", error)));
    }
    
    // 解析响应
    let access_token = token_data.get("access_token")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::Internal("Missing access_token in response".to_string()))?;
    
    let expires_in = token_data.get("expires_in")
        .and_then(|v| v.as_i64())
        .unwrap_or(7200);
    
    let refresh_token = token_data.get("refresh_token")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    
    let open_id = token_data.get("open_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::Internal("Missing open_id in response".to_string()))?;
    
    let scope = token_data.get("scope")
        .and_then(|v| v.as_str())
        .unwrap_or("user_info")
        .to_string();
    
    Ok(DouyinTokenResponse {
        access_token: access_token.to_string(),
        expires_in,
        refresh_token,
        open_id: open_id.to_string(),
        scope,
    })
}

/// 获取抖音用户信息
async fn get_douyin_user_info(access_token: &str, open_id: &str) -> Result<DouyinUserInfo, AppError> {
    let client = reqwest::Client::new();
    let mut params = HashMap::new();
    params.insert("access_token", access_token.to_string());
    params.insert("open_id", open_id.to_string());
    
    let response = client
        .post("https://open.douyin.com/oauth/userinfo/")
        .json(&params)
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("Failed to request Douyin user info: {}", e)))?;
    
    let user_data: serde_json::Value = response
        .json()
        .await
        .map_err(|e| AppError::Internal(format!("Failed to parse Douyin user info response: {}", e)))?;
    
    // 检查是否有错误
    if let Some(error) = user_data.get("error") {
        return Err(AppError::Internal(format!("Douyin API error: {}", error)));
    }
    
    // 解析用户信息
    let nickname = user_data.get("nickname")
        .and_then(|v| v.as_str())
        .unwrap_or("抖音用户")
        .to_string();
    
    let avatar = user_data.get("avatar")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    
    let gender = user_data.get("gender")
        .and_then(|v| v.as_i64())
        .map(|g| g as i32);
    
    let city = user_data.get("city")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    
    let province = user_data.get("province")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    
    let country = user_data.get("country")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    
    let union_id = user_data.get("union_id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    
    Ok(DouyinUserInfo {
        open_id: open_id.to_string(),
        union_id,
        nickname,
        avatar,
        gender,
        city,
        province,
        country,
    })
}

/// 生成随机状态码
fn generate_random_state() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    (0..16)
        .map(|_| rng.gen_range(0..16))
        .map(|n| format!("{:x}", n))
        .collect()
}

/// 配置抖音OAuth路由
pub fn config(cfg: &mut web::ServiceConfig) {
    cfg
        .route("/auth/douyin/qrcode", web::post().to(generate_douyin_qrcode))
        .route("/auth/douyin/status", web::get().to(check_douyin_login_status))
        .route("/auth/douyin/callback", web::post().to(handle_douyin_callback))
        .route("/auth/douyin/token", web::post().to(get_douyin_access_token))
        .route("/auth/douyin/userinfo", web::post().to(get_douyin_user_info_api));
}
