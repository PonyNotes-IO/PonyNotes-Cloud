use actix_web::{web, HttpRequest, Result, Scope};
use actix_web::web::{Data, Json};
use serde::{Deserialize, Serialize};
use tracing::info;


use crate::biz::user::phone_auth::{phone_login, validate_phone_number};
use crate::state::AppState;
use shared_entity::response::{AppResponse, AppResponseError, JsonAppResponse};
use app_error::ErrorCode;
use gotrue_entity::dto::{GotrueTokenResponse, User};

#[derive(Deserialize, Debug)]
pub struct SendSmsCodeRequest {
    pub phone: String,
    pub purpose: Option<String>, // "login", "register", "reset_password"
}

#[derive(Serialize, Debug)]
pub struct SendSmsCodeResponse {
    pub request_id: String,
    pub message: String,
}

#[derive(Deserialize, Debug)]
pub struct VerifySmsCodeRequest {
    pub phone: String,
    pub code: String,
    pub purpose: Option<String>,
}

#[derive(Serialize, Debug)]
pub struct VerifySmsCodeResponse {
    pub success: bool,
    pub message: String,
}

#[derive(Deserialize, Debug)]
pub struct PhoneLoginRequest {
    pub phone: String,
    pub code: String,
}

#[derive(Serialize, Debug)]
pub struct PhoneLoginResponse {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: i64,
    pub expires_at: i64,
    pub refresh_token: String,
    pub user: PhoneLoginUser,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_access_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_refresh_token: Option<String>,
}

#[derive(Serialize, Debug)]
pub struct PhoneLoginUser {
    pub id: String,
    pub email: String,
    pub created_at: String,
    pub updated_at: String,
    pub user_metadata: serde_json::Value,
}

pub fn sms_scope() -> Scope {
    web::scope("/api/sms")
        .service(web::resource("/send-code").route(web::post().to(send_sms_code_handler)))
        .service(web::resource("/verify-code").route(web::post().to(verify_sms_code_handler)))
        .service(web::resource("/phone-login").route(web::post().to(phone_login_handler)))
}

/// 发送短信验证码
#[tracing::instrument(skip(state, payload), err)]
async fn send_sms_code_handler(
    payload: Json<SendSmsCodeRequest>,
    state: Data<AppState>,
    req: HttpRequest,
) -> Result<JsonAppResponse<SendSmsCodeResponse>> {
    let request = payload.into_inner();
    let purpose = request.purpose.unwrap_or_else(|| "login".to_string());
    
    info!(
        "Sending SMS code to phone: {}, purpose: {}, from IP: {:?}",
        request.phone,
        purpose,
        req.peer_addr()
    );

    // 验证手机号格式
    let validated_phone = validate_phone_number(&request.phone)
        .map_err(|e| AppResponseError::new(ErrorCode::InvalidRequest, e.to_string()))?;

    // 获取SMS服务
    let sms_service = state.sms_service.as_ref()
        .ok_or_else(|| AppResponseError::new(ErrorCode::Internal, "SMS service not configured"))?;

    // 发送验证码
    match sms_service
        .send_verification_code(&state.pg_pool, &validated_phone, &purpose)
        .await
    {
        Ok(request_id) => {
            let response = SendSmsCodeResponse {
                request_id,
                message: "验证码发送成功".to_string(),
            };
            Ok(AppResponse::Ok().with_data(response).into())
        }
        Err(e) => {
            let error_msg = e.to_string();
            Err(AppResponseError::new(ErrorCode::InvalidRequest, error_msg).into())
        }
    }
}

/// 验证短信验证码
#[tracing::instrument(skip(state, payload), err)]
async fn verify_sms_code_handler(
    payload: Json<VerifySmsCodeRequest>,
    state: Data<AppState>,
) -> Result<JsonAppResponse<VerifySmsCodeResponse>> {
    let request = payload.into_inner();
    let purpose = request.purpose.unwrap_or_else(|| "login".to_string());

    info!("Verifying SMS code for phone: {}, purpose: {}", request.phone, purpose);

    // 验证手机号格式
    let validated_phone = validate_phone_number(&request.phone)
        .map_err(|e| AppResponseError::new(ErrorCode::InvalidRequest, e.to_string()))?;

    // 获取SMS服务
    let sms_service = state.sms_service.as_ref()
        .ok_or_else(|| AppResponseError::new(ErrorCode::Internal, "SMS service not configured"))?;

    // 验证验证码
    match sms_service
        .verify_code(&state.pg_pool, &validated_phone, &request.code, &purpose)
        .await
    {
        Ok(true) => {
            let response = VerifySmsCodeResponse {
                success: true,
                message: "验证码验证成功".to_string(),
            };
            Ok(AppResponse::Ok().with_data(response).into())
        }
        Ok(false) => {
            let response = VerifySmsCodeResponse {
                success: false,
                message: "验证码错误或已失效".to_string(),
            };
            Ok(AppResponse::Ok().with_data(response).into())
        }
        Err(e) => {
            let error_msg = e.to_string();
            Err(AppResponseError::new(ErrorCode::InvalidRequest, error_msg).into())
        }
    }
}

/// 手机号验证码登录
#[tracing::instrument(skip(state, payload), err)]
async fn phone_login_handler(
    payload: Json<PhoneLoginRequest>,
    state: Data<AppState>,
    req: HttpRequest,
) -> Result<JsonAppResponse<GotrueTokenResponse>> {
    let request = payload.into_inner();
    
    info!(
        "Phone login attempt for: {}, from IP: {:?}",
        request.phone,
        req.peer_addr()
    );

    // 验证手机号格式
    let validated_phone = validate_phone_number(&request.phone)
        .map_err(|e| AppResponseError::new(ErrorCode::InvalidRequest, e.to_string()))?;

    // 执行手机号登录流程
    match phone_login(&state, &validated_phone, &request.code).await {
        Ok(auth_result) => {
            let response = GotrueTokenResponse {
                access_token: auth_result.access_token,
                token_type: "bearer".to_string(),
                expires_in: 3600, // 1 hour
                expires_at: chrono::Utc::now().timestamp() + 3600,
                refresh_token: auth_result.refresh_token,
                user: User {
                    id: auth_result.user_uuid.to_string(),
                    aud: "authenticated".to_string(),
                    role: "authenticated".to_string(),
                    email: auth_result.user_email,
                    phone: validated_phone.clone(),
                    created_at: auth_result.user_created_at.clone(),
                    updated_at: auth_result.user_updated_at,
                    user_metadata: auth_result.user_metadata,
                    app_metadata: serde_json::json!({}),
                    email_confirmed_at: Some(auth_result.user_created_at.clone()),
                    phone_confirmed_at: Some(auth_result.user_created_at.clone()),
                    confirmation_sent_at: None,
                    recovery_sent_at: None,
                    email_change_sent_at: None,
                    new_email: None,
                    invited_at: None,
                    new_phone: None,
                    phone_change_sent_at: None,
                    reauthentication_sent_at: None,
                    last_sign_in_at: None,
                    factors: None,
                    identities: None,
                    confirmed_at: Some(auth_result.user_created_at.clone()),
                    banned_until: None,
                    deleted_at: None,
                },
                provider_access_token: None,
                provider_refresh_token: None,
            };
            
            info!(
                "Phone login successful for user: {}, is_new_user: {}",
                auth_result.user_uuid,
                auth_result.is_new_user
            );
            
            Ok(AppResponse::Ok().with_data(response).into())
        }
        Err(e) => {
            let error_msg = e.to_string();
            Err(AppResponseError::new(ErrorCode::InvalidRequest, error_msg).into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_send_sms_code_request() {
        let request = SendSmsCodeRequest {
            phone: "13812345678".to_string(),
            purpose: Some("login".to_string()),
        };
        
        assert_eq!(request.phone, "13812345678");
        assert_eq!(request.purpose.unwrap(), "login");
    }

    #[test]
    fn test_verify_sms_code_request() {
        let request = VerifySmsCodeRequest {
            phone: "13812345678".to_string(),
            code: "123456".to_string(),
            purpose: Some("login".to_string()),
        };
        
        assert_eq!(request.phone, "13812345678");
        assert_eq!(request.code, "123456");
        assert_eq!(request.purpose.unwrap(), "login");
    }
}
