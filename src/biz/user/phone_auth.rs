use anyhow::{anyhow, Result};
use app_error::AppError;
use database::workspace::select_user_profile;
use gotrue_entity::gotrue_jwt::{Amr, GoTrueJWTClaims};
use sqlx::Row;
use tracing::info;
use uuid::Uuid;
use jsonwebtoken::{encode, EncodingKey, Header};


use crate::state::AppState;

/// 手机号登录/注册结果
#[derive(Debug)]
pub struct PhoneAuthResult {
    pub user_uuid: Uuid,
    pub access_token: String,
    pub refresh_token: String,
    pub is_new_user: bool,
}

/// 通过手机号查找或创建用户
pub async fn find_or_create_user_by_phone(
    state: &AppState,
    phone: &str,
) -> Result<(Uuid, bool), AppError> {
    // 首先尝试查找现有用户
    let existing_user = sqlx::query(
        r#"
        SELECT uuid, uid, email, name, created_at
        FROM af_user
        WHERE phone = $1 AND deleted_at IS NULL
        "#
    )
    .bind(phone)
    .fetch_optional(&state.pg_pool)
    .await?;

    if let Some(user) = existing_user {
        let uuid: Uuid = user.try_get("uuid")?;
        let uid: i64 = user.try_get("uid")?;
        
        // 确保现有用户的metadata也包含手机号信息
        let phone_metadata = serde_json::json!({
            "phone_number": phone
        });
        
        sqlx::query(
            r#"
            UPDATE af_user
            SET metadata = $1
            WHERE uid = $2 AND (metadata IS NULL OR NOT metadata ? 'phone_number')
            "#
        )
        .bind(phone_metadata)
        .bind(uid)
        .execute(&state.pg_pool)
        .await?;
        
        info!("Found existing user for phone: {}", phone);
        return Ok((uuid, false));
    }

    // 用户不存在，创建新用户
    info!("Creating new user for phone: {}", phone);
    
    // 生成新的UUID和ID
    let user_uuid = Uuid::new_v4();
    let uid = state.next_user_id().await;
    let fake_email = format!("phone_{}@temp.local", phone);  // 生成临时邮箱
    let now = chrono::Utc::now();
    
    // 使用事务确保数据一致性
    let mut tx = state.pg_pool.begin().await?;
    
    // 首先在 auth.users 表中创建记录以满足外键约束
    sqlx::query(
        r#"
        INSERT INTO auth.users (id, email, phone, created_at, updated_at, email_confirmed_at, phone_confirmed_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        ON CONFLICT (id) DO NOTHING
        "#
    )
    .bind(&user_uuid)
    .bind(&fake_email)
    .bind(phone)
    .bind(now)
    .bind(now)
    .bind(now)
    .bind(now)
    .execute(&mut *tx)
    .await?;
    
    // 直接在af_user表中插入包含phone字段的记录，避免后续UPDATE造成的约束冲突
    let phone_metadata = serde_json::json!({
        "phone_number": phone
    });
    
    sqlx::query(
        r#"
        INSERT INTO af_user (uid, uuid, email, name, phone, metadata)
        VALUES ($1, $2, $3, $4, $5, $6)
        "#
    )
    .bind(uid)
    .bind(&user_uuid)
    .bind(&fake_email)
    .bind(&format!("用户{}", &phone[phone.len() - 4..])) // 默认昵称：用户+手机号后4位
    .bind(phone)
    .bind(phone_metadata)
    .execute(&mut *tx)
    .await?;
    
    // 创建工作区
    let _workspace_id: Uuid = sqlx::query("INSERT INTO af_workspace (owner_uid) VALUES ($1) RETURNING workspace_id")
        .bind(uid)
        .fetch_one(&mut *tx)
        .await?
        .get("workspace_id");
    
    // 提交事务
    tx.commit().await?;

    info!("Created new user with UUID: {} for phone: {}", user_uuid, phone);
    Ok((user_uuid, true))
}

/// 生成访问令牌
pub async fn generate_access_token_for_user(
    state: &AppState,
    user_uuid: Uuid,
) -> Result<(String, String), AppError> {
    // 获取用户资料以确保用户存在
    let user_profile = select_user_profile(&state.pg_pool, &user_uuid)
        .await?
        .ok_or_else(|| AppError::RecordNotFound(format!("User not found: {}", user_uuid)))?;
    
    // 单独查询用户的手机号
    let user_phone: Option<String> = sqlx::query_scalar(
        "SELECT phone FROM af_user WHERE uuid = $1"
    )
    .bind(user_uuid)
    .fetch_optional(&state.pg_pool)
    .await?;

    // 使用 GoTrue 生成 JWT token
    let claims = GoTrueJWTClaims {
        aud: Some("authenticated".to_string()),
        exp: Some(chrono::Utc::now().timestamp() + 3600 * 24), // 24小时过期
        jti: Some(Uuid::new_v4().to_string()),
        iat: Some(chrono::Utc::now().timestamp()),
        iss: Some("appflowy-cloud".to_string()),
        nbf: Some(chrono::Utc::now().timestamp()),
        sub: Some(user_uuid.to_string()),
        email: user_profile.email.unwrap_or_default(),
        phone: user_phone.unwrap_or_default(),
        app_metadata: serde_json::json!({}),
        user_metadata: serde_json::json!({}),
        role: "authenticated".to_string(),
        aal: Some("aal1".to_string()),
        amr: Some(vec![Amr {
            method: "sms".to_string(),
            timestamp: chrono::Utc::now().timestamp() as u64,
            provider: Some("appflowy".to_string()),
        }]),
        session_id: Some(Uuid::new_v4().to_string()),
    };

    // 使用真正的 JWT 签名
    
    let secret = std::env::var("GOTRUE_JWT_SECRET")
        .unwrap_or_else(|_| "your-256-bit-secret".to_string());
    
    let access_token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_ref()),
    )
    .map_err(|e| AppError::Internal(anyhow!("Failed to generate access token: {}", e)))?;
    
    // 生成refresh token (通常使用不同的过期时间和claims)
    let mut refresh_claims = claims.clone();
    refresh_claims.exp = Some(chrono::Utc::now().timestamp() + 3600 * 24 * 30); // 30天过期
    
    let refresh_token = encode(
        &Header::default(),
        &refresh_claims,
        &EncodingKey::from_secret(secret.as_ref()),
    )
    .map_err(|e| AppError::Internal(anyhow!("Failed to generate refresh token: {}", e)))?;

    Ok((access_token, refresh_token))
}

/// 完整的手机号登录流程
pub async fn phone_login(
    state: &AppState,
    phone: &str,
    code: &str,
) -> Result<PhoneAuthResult, AppError> {
    // 1. 验证短信验证码
    let sms_service = state
        .sms_service
        .as_ref()
        .ok_or_else(|| AppError::Internal(anyhow!("SMS service not configured")))?;

    let is_code_valid = sms_service
        .verify_code(&state.pg_pool, phone, code, "login")
        .await
        .map_err(|e| AppError::InvalidRequest(e.to_string()))?;

    if !is_code_valid {
        return Err(AppError::InvalidRequest("验证码错误或已失效".to_string()));
    }

    // 2. 查找或创建用户
    let (user_uuid, is_new_user) = find_or_create_user_by_phone(state, phone).await?;

    // 3. 生成访问令牌
    let (access_token, refresh_token) = generate_access_token_for_user(state, user_uuid).await?;

    info!(
        "Phone login successful for user: {}, is_new: {}",
        user_uuid, is_new_user
    );

    Ok(PhoneAuthResult {
        user_uuid,
        access_token,
        refresh_token,
        is_new_user,
    })
}

/// 验证手机号格式
pub fn validate_phone_number(phone: &str) -> Result<String, AppError> {
    // 移除所有非数字字符
    let cleaned_phone: String = phone.chars().filter(|c| c.is_ascii_digit()).collect();

    // 中国大陆手机号验证
    if cleaned_phone.len() == 11 && cleaned_phone.starts_with('1') {
        return Ok(cleaned_phone);
    }

    // 国际格式（以86开头的中国号码）
    if cleaned_phone.len() == 13 && cleaned_phone.starts_with("86") {
        let domestic_part = &cleaned_phone[2..];
        if domestic_part.len() == 11 && domestic_part.starts_with('1') {
            return Ok(domestic_part.to_string());
        }
    }

    Err(AppError::InvalidRequest(
        "手机号格式不正确，请输入有效的中国大陆手机号".to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_phone_number() {
        // 有效的手机号
        assert_eq!(validate_phone_number("13812345678").unwrap(), "13812345678");
        assert_eq!(validate_phone_number("159-8765-4321").unwrap(), "15987654321");
        assert_eq!(validate_phone_number("138 1234 5678").unwrap(), "13812345678");
        assert_eq!(validate_phone_number("+86 138 1234 5678").unwrap(), "13812345678");
        assert_eq!(validate_phone_number("8613812345678").unwrap(), "13812345678");

        // 无效的手机号
        assert!(validate_phone_number("1234567890").is_err()); // 不是11位
        assert!(validate_phone_number("21234567890").is_err()); // 不以1开头
        assert!(validate_phone_number("138123456789").is_err()); // 超过11位
        assert!(validate_phone_number("1381234567").is_err()); // 少于11位
    }
}
