-- 手机短信验证码认证相关表
-- Add phone number field to af_user table
ALTER TABLE af_user ADD COLUMN IF NOT EXISTS phone TEXT DEFAULT NULL UNIQUE;
CREATE INDEX IF NOT EXISTS idx_af_user_phone ON af_user(phone) WHERE phone IS NOT NULL;

-- SMS verification codes table
CREATE TABLE IF NOT EXISTS af_sms_verification_code (
    id BIGSERIAL PRIMARY KEY,
    phone TEXT NOT NULL,
    code TEXT NOT NULL,
    purpose TEXT NOT NULL DEFAULT 'login', -- 'login', 'register', 'reset_password'
    used BOOLEAN NOT NULL DEFAULT FALSE,
    attempts INTEGER NOT NULL DEFAULT 0,
    max_attempts INTEGER NOT NULL DEFAULT 3,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    expires_at TIMESTAMP WITH TIME ZONE NOT NULL
);

-- Indexes for SMS verification codes
CREATE INDEX IF NOT EXISTS idx_sms_verification_phone ON af_sms_verification_code(phone);
CREATE INDEX IF NOT EXISTS idx_sms_verification_code ON af_sms_verification_code(code);
CREATE INDEX IF NOT EXISTS idx_sms_verification_expires ON af_sms_verification_code(expires_at);
CREATE INDEX IF NOT EXISTS idx_sms_verification_phone_purpose ON af_sms_verification_code(phone, purpose);

-- SMS rate limiting table
CREATE TABLE IF NOT EXISTS af_sms_rate_limit (
    id BIGSERIAL PRIMARY KEY,
    phone TEXT NOT NULL,
    last_sent_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    daily_count INTEGER NOT NULL DEFAULT 1,
    date DATE NOT NULL DEFAULT CURRENT_DATE
);

-- Indexes for SMS rate limiting
CREATE UNIQUE INDEX IF NOT EXISTS idx_sms_rate_limit_phone_date ON af_sms_rate_limit(phone, date);
CREATE INDEX IF NOT EXISTS idx_sms_rate_limit_date ON af_sms_rate_limit(date);

-- Function to clean up expired verification codes
CREATE OR REPLACE FUNCTION cleanup_expired_sms_codes() RETURNS void AS $$
BEGIN
    DELETE FROM af_sms_verification_code 
    WHERE expires_at < CURRENT_TIMESTAMP - INTERVAL '1 hour';
END;
$$ LANGUAGE plpgsql;

-- Function to clean up old rate limit records (keep only last 7 days)
CREATE OR REPLACE FUNCTION cleanup_old_rate_limits() RETURNS void AS $$
BEGIN
    DELETE FROM af_sms_rate_limit 
    WHERE date < CURRENT_DATE - INTERVAL '7 days';
END;
$$ LANGUAGE plpgsql;
