use anyhow::{anyhow, Result};
use chrono::Utc;
use hmac::{Hmac, Mac};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha1::Sha1;
use std::collections::HashMap;
use url::form_urlencoded;
use uuid::Uuid;

type HmacSha1 = Hmac<Sha1>;

#[derive(Debug, Clone)]
pub struct AliyunSmsConfig {
    pub access_key_id: String,
    pub access_key_secret: String,
    pub sign_name: String,
    pub template_code: String,
    pub endpoint: String,
    pub api_version: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SendSmsResponse {
    #[serde(rename = "RequestId")]
    pub request_id: String,
    #[serde(rename = "BizId")]
    pub biz_id: String,
    #[serde(rename = "Code")]
    pub code: String,
    #[serde(rename = "Message")]
    pub message: String,
}

#[derive(Debug)]
#[derive(Clone)]
pub struct AliyunSmsClient {
    config: AliyunSmsConfig,
    http_client: Client,
}

impl AliyunSmsClient {
    pub fn new(config: AliyunSmsConfig) -> Self {
        Self {
            config,
            http_client: Client::new(),
        }
    }

    /// 发送短信验证码
    pub async fn send_verification_code(&self, phone: &str, code: &str) -> Result<SendSmsResponse> {
        let mut params = HashMap::new();
        
        // 基础参数
        params.insert("Format", "JSON");
        params.insert("Version", &self.config.api_version);
        params.insert("AccessKeyId", &self.config.access_key_id);
        params.insert("SignatureVersion", "1.0");
        params.insert("SignatureMethod", "HMAC-SHA1");
        let signature_nonce = Uuid::new_v4().to_string();
        let timestamp = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
        params.insert("SignatureNonce", &signature_nonce);
        params.insert("Timestamp", &timestamp);
        
        // SMS 特定参数
        params.insert("Action", "SendSms");
        params.insert("PhoneNumbers", phone);
        params.insert("SignName", &self.config.sign_name);
        params.insert("TemplateCode", &self.config.template_code);
        let template_param = json!({"code": code}).to_string();
        params.insert("TemplateParam", &template_param);

        // 生成签名
        let signature = self.generate_signature(&params)?;
        params.insert("Signature", &signature);

        // 构建请求URL
        let url = format!("https://{}", self.config.endpoint);
        
        // 发送请求
        let response = self
            .http_client
            .post(&url)
            .form(&params)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(anyhow!("SMS request failed with status {}: {}", status, text));
        }

        let response_text = response.text().await?;
        
        // 解析响应
        let sms_response: SendSmsResponse = serde_json::from_str(&response_text)
            .map_err(|e| anyhow!("Failed to parse SMS response: {}. Response: {}", e, response_text))?;

        // 检查业务错误
        if sms_response.code != "OK" {
            return Err(anyhow!(
                "SMS API error: {} - {}", 
                sms_response.code, 
                sms_response.message
            ));
        }

        Ok(sms_response)
    }

    /// 生成阿里云API签名
    fn generate_signature(&self, params: &HashMap<&str, &str>) -> Result<String> {
        // 1. 按字典序排序参数
        let mut sorted_params: Vec<(&str, &str)> = params.iter().map(|(k, v)| (*k, *v)).collect();
        sorted_params.sort_by(|a, b| a.0.cmp(b.0));

        // 2. 构建查询字符串
        let query_string = form_urlencoded::Serializer::new(String::new())
            .extend_pairs(sorted_params.iter())
            .finish();

        // 3. 构建待签名字符串
        let string_to_sign = format!(
            "POST&{}&{}",
            percent_encode("/"),
            percent_encode(&query_string)
        );

        // 4. 计算签名
        let signing_key = format!("{}&", self.config.access_key_secret);
        let mut mac = HmacSha1::new_from_slice(signing_key.as_bytes())
            .map_err(|e| anyhow!("Failed to create HMAC: {}", e))?;
        
        mac.update(string_to_sign.as_bytes());
        let signature_bytes = mac.finalize().into_bytes();
        
        use base64::{Engine as _, engine::general_purpose};
        Ok(general_purpose::STANDARD.encode(signature_bytes))
    }
}

/// URL编码函数
fn percent_encode(input: &str) -> String {
    input
        .chars()
        .map(|c| match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => c.to_string(),
            _ => format!("%{:02X}", c as u8),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_percent_encode() {
        assert_eq!(percent_encode("hello world"), "hello%20world");
        assert_eq!(percent_encode("test=123&foo=bar"), "test%3D123%26foo%3Dbar");
    }

    #[test]
    fn test_aliyun_sms_config() {
        let config = AliyunSmsConfig {
            access_key_id: "test_id".to_string(),
            access_key_secret: "test_secret".to_string(),
            sign_name: "测试签名".to_string(),
            template_code: "SMS_123456".to_string(),
            endpoint: "dysmsapi.aliyuncs.com".to_string(),
            api_version: "2017-05-25".to_string(),
        };

        let client = AliyunSmsClient::new(config);
        assert_eq!(client.config.access_key_id, "test_id");
    }
}
