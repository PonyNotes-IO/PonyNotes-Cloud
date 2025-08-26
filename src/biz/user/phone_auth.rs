use anyhow::{anyhow, Result};
use app_error::AppError;
use database::workspace::{select_user_profile, select_workspace};
use gotrue_entity::gotrue_jwt::{Amr, GoTrueJWTClaims};
use sqlx::Row;
use tracing::info;
use uuid::Uuid;
use jsonwebtoken::{encode, EncodingKey, Header};
use workspace_template::document::getting_started::GettingStartedTemplate;
use database_entity::dto::AFRole;

use crate::biz::user::user_init::initialize_workspace_for_user;
use crate::state::AppState;

/// 手机号登录/注册结果
#[derive(Debug)]
pub struct PhoneAuthResult {
    pub user_uuid: Uuid,
    pub user_uid: i64,
    pub user_email: String,
    pub user_name: String,
    pub user_created_at: String,
    pub user_updated_at: String,
    pub access_token: String,
    pub refresh_token: String,
    pub is_new_user: bool,
    pub user_metadata: serde_json::Value,
    pub latest_workspace_id: Uuid,
}

/// 用户信息结构体
#[derive(Debug)]
pub struct UserInfo {
    pub uuid: Uuid,
    pub uid: i64,
    pub email: String,
    pub name: String,
    pub created_at: String,
    pub updated_at: String,
    pub metadata: serde_json::Value,
    pub latest_workspace_id: Uuid,
}

/// 通过手机号查找或创建用户
pub async fn find_or_create_user_by_phone(
    state: &AppState,
    phone: &str,
) -> Result<(UserInfo, bool), AppError> {
    // 首先尝试查找现有用户
    let existing_user = sqlx::query(
        r#"
        SELECT uuid, uid, email, name, created_at, updated_at, metadata
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
        let email: String = user.try_get("email")?;
        let name: String = user.try_get("name")?;
        let created_at: chrono::DateTime<chrono::Utc> = user.try_get("created_at")?;
        let updated_at: chrono::DateTime<chrono::Utc> = user.try_get("updated_at")?;
        let mut metadata: serde_json::Value = user.try_get("metadata").unwrap_or_else(|_| serde_json::json!({}));
        
        // 确保现有用户的metadata也包含手机号信息
        if !metadata.as_object().map_or(false, |m| m.contains_key("phone_number")) {
            if let Some(obj) = metadata.as_object_mut() {
                obj.insert("phone_number".to_string(), serde_json::Value::String(phone.to_string()));
            } else {
                metadata = serde_json::json!({
                    "phone_number": phone
                });
            }
            
            sqlx::query(
                r#"
                UPDATE af_user
                SET metadata = $1
                WHERE uid = $2
                "#
            )
            .bind(&metadata)
            .bind(uid)
            .execute(&state.pg_pool)
            .await?;
        }
        
        // 获取用户的workspace_id
        let latest_workspace_id: Uuid = sqlx::query_scalar(
            "SELECT workspace_id FROM af_workspace WHERE owner_uid = $1 LIMIT 1"
        )
        .bind(uid)
        .fetch_optional(&state.pg_pool)
        .await?
        .ok_or_else(|| AppError::Internal(anyhow!("User has no workspace")))?;
        
        // 检查现有用户是否有完整的工作区初始化
        let needs_workspace_init = check_user_needs_workspace_init(state, uid, &uuid).await?;
        if needs_workspace_init {
            info!("Existing user {} needs workspace initialization", uuid);
            ensure_user_workspace_initialized(state, uid, &uuid).await?;
        }
        
        info!("Found existing user for phone: {}", phone);
        
        let user_info = UserInfo {
            uuid,
            uid,
            email,
            name,
            created_at: created_at.to_rfc3339(),
            updated_at: updated_at.to_rfc3339(),
            metadata,
            latest_workspace_id,
        };
        
        return Ok((user_info, false));
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
    .bind(&phone_metadata)
    .execute(&mut *tx)
    .await?;
    
    // 创建工作区
    let workspace_id: Uuid = sqlx::query("INSERT INTO af_workspace (owner_uid) VALUES ($1) RETURNING workspace_id")
        .bind(uid)
        .fetch_one(&mut *tx)
        .await?
        .get("workspace_id");
    
    // 提交事务
    tx.commit().await?;

    // 初始化工作区（包括用户认知对象）
    let workspace_row = select_workspace(&state.pg_pool, &workspace_id).await?;
    
    // 添加用户角色权限
    state
        .workspace_access_control
        .insert_role(&uid, &workspace_id, AFRole::Owner)
        .await?;
    
    // 创建完整的工作区结构（包括用户认知对象）
    let mut txn2 = state.pg_pool.begin().await?;
    initialize_workspace_for_user(
        uid,
        &user_uuid,
        &workspace_row,
        &mut txn2,
        vec![GettingStartedTemplate],
        &state.collab_storage,
    )
    .await?;
    txn2.commit().await?;

    info!("Created new user with UUID: {} for phone: {} and initialized workspace", user_uuid, phone);
    
    let user_name = format!("用户{}", &phone[phone.len() - 4..]);
    let user_info = UserInfo {
        uuid: user_uuid,
        uid,
        email: fake_email,
        name: user_name,
        created_at: now.to_rfc3339(),
        updated_at: now.to_rfc3339(),
        metadata: serde_json::json!({
            "phone_number": phone
        }),
        latest_workspace_id: workspace_id,
    };
    
    Ok((user_info, true))
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
    let (user_info, is_new_user) = find_or_create_user_by_phone(state, phone).await?;

    // 3. 生成访问令牌
    let (access_token, refresh_token) = generate_access_token_for_user(state, user_info.uuid).await?;

    info!(
        "Phone login successful for user: {}, is_new: {}",
        user_info.uuid, is_new_user
    );

    Ok(PhoneAuthResult {
        user_uuid: user_info.uuid,
        user_uid: user_info.uid,
        user_email: user_info.email,
        user_name: user_info.name,
        user_created_at: user_info.created_at,
        user_updated_at: user_info.updated_at,
        access_token,
        refresh_token,
        is_new_user,
        user_metadata: user_info.metadata,
        latest_workspace_id: user_info.latest_workspace_id,
    })
}

/// 检查用户是否需要工作区初始化
async fn check_user_needs_workspace_init(
    state: &AppState,
    uid: i64,
    user_uuid: &Uuid,
) -> Result<bool, AppError> {
    // 检查用户的工作区是否存在用户认知对象
    let workspace_ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT workspace_id FROM af_workspace WHERE owner_uid = $1"
    )
    .bind(uid)
    .fetch_all(&state.pg_pool)
    .await?;
    
    for workspace_id in workspace_ids {
        // 检查这个工作区是否有用户认知对象
        let has_user_awareness = sqlx::query_scalar!(
            "SELECT EXISTS(SELECT 1 FROM af_collab WHERE workspace_id = $1 AND oid = $2 AND partition_key = 5)",
            workspace_id,
            crate::biz::user::user_init::user_awareness_object_id(user_uuid, &workspace_id)
        )
        .fetch_one(&state.pg_pool)
        .await?;
        
        if !has_user_awareness.unwrap_or(false) {
            return Ok(true);
        }
    }
    
    Ok(false)
}

/// 确保用户工作区完全初始化
async fn ensure_user_workspace_initialized(
    state: &AppState,
    uid: i64,
    user_uuid: &Uuid,
) -> Result<(), AppError> {
    let workspace_ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT workspace_id FROM af_workspace WHERE owner_uid = $1"
    )
    .bind(uid)
    .fetch_all(&state.pg_pool)
    .await?;
    
    for workspace_id in workspace_ids {
        let workspace_row = select_workspace(&state.pg_pool, &workspace_id).await?;
        
        // 确保用户有Owner角色
        state
            .workspace_access_control
            .insert_role(&uid, &workspace_id, AFRole::Owner)
            .await?;
        
        // 初始化工作区（这会创建缺失的用户认知对象）
        let mut txn = state.pg_pool.begin().await?;
        initialize_workspace_for_user(
            uid,
            user_uuid,
            &workspace_row,
            &mut txn,
            vec![GettingStartedTemplate], // 使用明确的类型
            &state.collab_storage,
        )
        .await?;
        txn.commit().await?;
        
        info!("Initialized workspace {} for existing user {}", workspace_id, user_uuid);
    }
    
    Ok(())
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
