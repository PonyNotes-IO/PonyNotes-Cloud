use anyhow::{anyhow, Result};
use chrono::{DateTime, Duration, Utc};
use rand::Rng;
use sqlx::PgPool;
use tracing::info;

#[derive(Debug, Clone)]
pub struct VerificationCodeConfig {
    pub code_length: u8,
    pub expire_minutes: i64,
    pub rate_limit_minutes: i64,
    pub max_attempts: i32,
}

impl Default for VerificationCodeConfig {
    fn default() -> Self {
        Self {
            code_length: 6,
            expire_minutes: 5,
            rate_limit_minutes: 1,
            max_attempts: 3,
        }
    }
}

#[derive(Debug, sqlx::FromRow)]
pub struct SmsVerificationCode {
    pub id: i64,
    pub phone: String,
    pub code: String,
    pub purpose: String,
    pub used: bool,
    pub attempts: i32,
    pub max_attempts: i32,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, sqlx::FromRow)]
pub struct SmsRateLimit {
    pub id: i64,
    pub phone: String,
    pub last_sent_at: DateTime<Utc>,
    pub daily_count: i32,
    pub date: chrono::NaiveDate,
}

/// 验证码服务
#[derive(Clone)]
pub struct VerificationCodeService {
    config: VerificationCodeConfig,
}

impl VerificationCodeService {
    pub fn new(config: VerificationCodeConfig) -> Self {
        Self { config }
    }

    /// 生成随机验证码
    pub fn generate_code(&self) -> String {
        let mut rng = rand::thread_rng();
        let min = 10_u32.pow((self.config.code_length - 1) as u32);
        let max = 10_u32.pow(self.config.code_length as u32) - 1;
        rng.gen_range(min..=max).to_string()
    }

    /// 检查发送频率限制
    pub async fn check_rate_limit(&self, pool: &PgPool, phone: &str) -> Result<bool> {
        let cutoff_time = Utc::now() - Duration::minutes(self.config.rate_limit_minutes);
        
        let count: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*) as count
            FROM af_sms_verification_code
            WHERE phone = $1 AND created_at > $2
            "#
        )
        .bind(phone)
        .bind(cutoff_time)
        .fetch_one(pool)
        .await?;

        Ok(count == 0)
    }

    /// 保存验证码到数据库
    pub async fn save_verification_code(
        &self,
        pool: &PgPool,
        phone: &str,
        code: &str,
        purpose: &str,
    ) -> Result<i64> {
        let expires_at = Utc::now() + Duration::minutes(self.config.expire_minutes);

        // 先使同一手机号的旧验证码失效
        sqlx::query(
            r#"
            UPDATE af_sms_verification_code
            SET used = true
            WHERE phone = $1 AND purpose = $2 AND used = false
            "#
        )
        .bind(phone)
        .bind(purpose)
        .execute(pool)
        .await?;

        // 插入新验证码
        let result: (i64,) = sqlx::query_as(
            r#"
            INSERT INTO af_sms_verification_code (phone, code, purpose, expires_at, max_attempts)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING id
            "#
        )
        .bind(phone)
        .bind(code)
        .bind(purpose)
        .bind(expires_at)
        .bind(self.config.max_attempts)
        .fetch_one(pool)
        .await?;

        info!("Saved verification code for phone: {}, purpose: {}", phone, purpose);
        Ok(result.0)
    }

    /// 验证验证码
    pub async fn verify_code(
        &self,
        pool: &PgPool,
        phone: &str,
        code: &str,
        purpose: &str,
    ) -> Result<bool> {
        let now = Utc::now();

        // 查找有效的验证码
        let verification = sqlx::query_as::<_, SmsVerificationCode>(
            r#"
            SELECT id, phone, code, purpose, used, attempts, max_attempts, created_at, expires_at
            FROM af_sms_verification_code
            WHERE phone = $1 AND purpose = $2 AND used = false AND expires_at > $3
            ORDER BY created_at DESC
            LIMIT 1
            "#
        )
        .bind(phone)
        .bind(purpose)
        .bind(now)
        .fetch_optional(pool)
        .await?;

        let Some(mut verification) = verification else {
            return Ok(false);
        };

        // 增加尝试次数
        verification.attempts += 1;

        if verification.attempts > verification.max_attempts {
            // 超过最大尝试次数，标记为已使用
            sqlx::query(
                r#"
                UPDATE af_sms_verification_code
                SET used = true, attempts = $1
                WHERE id = $2
                "#
            )
            .bind(verification.attempts)
            .bind(verification.id)
            .execute(pool)
            .await?;

            return Err(anyhow!("验证码尝试次数过多，请重新获取"));
        }

        if verification.code == code {
            // 验证成功，标记为已使用
            sqlx::query(
                r#"
                UPDATE af_sms_verification_code
                SET used = true, attempts = $1
                WHERE id = $2
                "#
            )
            .bind(verification.attempts)
            .bind(verification.id)
            .execute(pool)
            .await?;

            info!("Verification code verified successfully for phone: {}", phone);
            return Ok(true);
        } else {
            // 验证失败，更新尝试次数
            sqlx::query(
                r#"
                UPDATE af_sms_verification_code
                SET attempts = $1
                WHERE id = $2
                "#
            )
            .bind(verification.attempts)
            .bind(verification.id)
            .execute(pool)
            .await?;

            return Ok(false);
        }
    }

    /// 更新发送速率限制记录
    pub async fn update_rate_limit(&self, pool: &PgPool, phone: &str) -> Result<()> {
        let today = Utc::now().naive_utc().date();
        let now = Utc::now();

        // 尝试更新现有记录
        let updated = sqlx::query(
            r#"
            UPDATE af_sms_rate_limit
            SET last_sent_at = $1, daily_count = daily_count + 1
            WHERE phone = $2 AND date = $3
            "#
        )
        .bind(now)
        .bind(phone)
        .bind(today)
        .execute(pool)
        .await?;

        if updated.rows_affected() == 0 {
            // 没有现有记录，插入新记录
            sqlx::query(
                r#"
                INSERT INTO af_sms_rate_limit (phone, last_sent_at, daily_count, date)
                VALUES ($1, $2, 1, $3)
                "#
            )
            .bind(phone)
            .bind(now)
            .bind(today)
            .execute(pool)
            .await?;
        }

        Ok(())
    }

    /// 清理过期的验证码
    pub async fn cleanup_expired_codes(&self, pool: &PgPool) -> Result<u64> {
        let result = sqlx::query(
            r#"
            DELETE FROM af_sms_verification_code
            WHERE expires_at < $1
            "#
        )
        .bind(Utc::now())
        .execute(pool)
        .await?;

        info!("Cleaned up {} expired verification codes", result.rows_affected());
        Ok(result.rows_affected())
    }

    /// 清理旧的速率限制记录
    pub async fn cleanup_old_rate_limits(&self, pool: &PgPool) -> Result<u64> {
        let cutoff_date = (Utc::now() - Duration::days(7)).naive_utc().date();
        
        let result = sqlx::query(
            r#"
            DELETE FROM af_sms_rate_limit
            WHERE date < $1
            "#
        )
        .bind(cutoff_date)
        .execute(pool)
        .await?;

        info!("Cleaned up {} old rate limit records", result.rows_affected());
        Ok(result.rows_affected())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_code() {
        let config = VerificationCodeConfig::default();
        let service = VerificationCodeService::new(config);
        
        let code = service.generate_code();
        assert_eq!(code.len(), 6);
        assert!(code.chars().all(|c| c.is_ascii_digit()));
    }

    #[test]
    fn test_custom_code_length() {
        let config = VerificationCodeConfig {
            code_length: 4,
            ..Default::default()
        };
        let service = VerificationCodeService::new(config);
        
        let code = service.generate_code();
        assert_eq!(code.len(), 4);
        assert!(code.chars().all(|c| c.is_ascii_digit()));
    }
}
