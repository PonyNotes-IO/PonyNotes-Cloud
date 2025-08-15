use anyhow::{anyhow, Result};
use sqlx::PgPool;
use tracing::{error, info};

use super::{AliyunSmsClient, aliyun_sms::AliyunSmsConfig, VerificationCodeConfig, VerificationCodeService};

/// SMS 服务，整合阿里云短信和验证码管理
#[derive(Clone)]
pub struct SmsService {
    aliyun_client: AliyunSmsClient,
    verification_service: VerificationCodeService,
}

impl SmsService {
    pub fn new(aliyun_config: AliyunSmsConfig, verification_config: VerificationCodeConfig) -> Self {
        Self {
            aliyun_client: AliyunSmsClient::new(aliyun_config),
            verification_service: VerificationCodeService::new(verification_config),
        }
    }

    /// 发送验证码短信
    pub async fn send_verification_code(
        &self,
        pool: &PgPool,
        phone: &str,
        purpose: &str,
    ) -> Result<String> {
        // 验证手机号格式
        if !self.is_valid_phone(phone) {
            return Err(anyhow!("手机号格式不正确"));
        }

        // 检查发送频率限制
        if !self.verification_service.check_rate_limit(pool, phone).await? {
            return Err(anyhow!("发送过于频繁，请稍后再试"));
        }

        // 生成验证码
        let code = self.verification_service.generate_code();

        // 发送短信
        match self.aliyun_client.send_verification_code(phone, &code).await {
            Ok(response) => {
                info!("SMS sent successfully: {}", response.request_id);
                
                // 保存验证码到数据库
                self.verification_service
                    .save_verification_code(pool, phone, &code, purpose)
                    .await?;

                // 更新发送速率限制
                self.verification_service
                    .update_rate_limit(pool, phone)
                    .await?;

                Ok(response.request_id)
            }
            Err(e) => {
                error!("Failed to send SMS: {}", e);
                Err(anyhow!("发送短信失败: {}", e))
            }
        }
    }

    /// 验证短信验证码
    pub async fn verify_code(
        &self,
        pool: &PgPool,
        phone: &str,
        code: &str,
        purpose: &str,
    ) -> Result<bool> {
        // 验证手机号格式
        if !self.is_valid_phone(phone) {
            return Err(anyhow!("手机号格式不正确"));
        }

        // 验证码不能为空
        if code.trim().is_empty() {
            return Err(anyhow!("验证码不能为空"));
        }

        // 执行验证
        self.verification_service
            .verify_code(pool, phone, code, purpose)
            .await
    }

    /// 清理过期数据
    pub async fn cleanup_expired_data(&self, pool: &PgPool) -> Result<()> {
        // 清理过期的验证码
        self.verification_service.cleanup_expired_codes(pool).await?;
        
        // 清理旧的速率限制记录
        self.verification_service.cleanup_old_rate_limits(pool).await?;
        
        Ok(())
    }

    /// 验证手机号格式（简单验证）
    fn is_valid_phone(&self, phone: &str) -> bool {
        // 中国大陆手机号验证：11位数字，以1开头
        if phone.len() != 11 {
            return false;
        }

        if !phone.starts_with('1') {
            return false;
        }

        phone.chars().all(|c| c.is_ascii_digit())
    }

    /// 获取手机号的规范化格式（去除空格、特殊字符等）
    pub fn normalize_phone(&self, phone: &str) -> String {
        phone
            .chars()
            .filter(|c| c.is_ascii_digit())
            .collect::<String>()
    }
}

/// SMS 服务构建器
pub struct SmsServiceBuilder {
    aliyun_config: Option<AliyunSmsConfig>,
    verification_config: Option<VerificationCodeConfig>,
}

impl SmsServiceBuilder {
    pub fn new() -> Self {
        Self {
            aliyun_config: None,
            verification_config: None,
        }
    }

    pub fn with_aliyun_config(mut self, config: AliyunSmsConfig) -> Self {
        self.aliyun_config = Some(config);
        self
    }

    pub fn with_verification_config(mut self, config: VerificationCodeConfig) -> Self {
        self.verification_config = Some(config);
        self
    }

    pub fn build(self) -> Result<SmsService> {
        let aliyun_config = self.aliyun_config.ok_or_else(|| anyhow!("AliyunSmsConfig is required"))?;
        let verification_config = self.verification_config.unwrap_or_default();

        Ok(SmsService::new(aliyun_config, verification_config))
    }
}

impl Default for SmsServiceBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_phone() {
        let config = AliyunSmsConfig {
            access_key_id: "test".to_string(),
            access_key_secret: "test".to_string(),
            sign_name: "test".to_string(),
            template_code: "test".to_string(),
            endpoint: "test".to_string(),
            api_version: "test".to_string(),
        };
        
        let service = SmsService::new(config, VerificationCodeConfig::default());

        assert!(service.is_valid_phone("13812345678"));
        assert!(service.is_valid_phone("15987654321"));
        assert!(!service.is_valid_phone("1234567890")); // 不是11位
        assert!(!service.is_valid_phone("21234567890")); // 不以1开头
        assert!(!service.is_valid_phone("1381234567a")); // 包含非数字
    }

    #[test]
    fn test_normalize_phone() {
        let config = AliyunSmsConfig {
            access_key_id: "test".to_string(),
            access_key_secret: "test".to_string(),
            sign_name: "test".to_string(),
            template_code: "test".to_string(),
            endpoint: "test".to_string(),
            api_version: "test".to_string(),
        };
        
        let service = SmsService::new(config, VerificationCodeConfig::default());

        assert_eq!(service.normalize_phone("138-1234-5678"), "13812345678");
        assert_eq!(service.normalize_phone("138 1234 5678"), "13812345678");
        assert_eq!(service.normalize_phone("+86 138 1234 5678"), "8613812345678");
    }

    #[test]
    fn test_sms_service_builder() {
        let aliyun_config = AliyunSmsConfig {
            access_key_id: "test".to_string(),
            access_key_secret: "test".to_string(),
            sign_name: "test".to_string(),
            template_code: "test".to_string(),
            endpoint: "test".to_string(),
            api_version: "test".to_string(),
        };

        let service = SmsServiceBuilder::new()
            .with_aliyun_config(aliyun_config)
            .build()
            .unwrap();

        assert!(service.is_valid_phone("13812345678"));
    }
}
