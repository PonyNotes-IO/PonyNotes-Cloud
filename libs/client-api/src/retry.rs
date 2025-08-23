use crate::notify::ClientToken;
use crate::ws::{
  ConnectState, ConnectStateNotify, StateNotify, WSClientConnectURLProvider, WSError,
};

use app_error::gotrue::GoTrueError;
use client_websocket::{connect_async, WebSocketStream};
use gotrue::grant::{Grant, RefreshTokenGrant};
use parking_lot::RwLock;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Weak};
use std::time::Duration;
use tokio_retry::strategy::{ExponentialBackoff, FixedInterval};
use tokio_retry::{Action, Condition, RetryIf};
use tokio_tungstenite::tungstenite::http::HeaderMap;
use tracing::{debug, info, trace};

pub(crate) struct RefreshTokenAction {
  token: Arc<RwLock<ClientToken>>,
  gotrue_client: Arc<gotrue::api::Client>,
}

impl RefreshTokenAction {
  pub fn new(token: Arc<RwLock<ClientToken>>, gotrue_client: gotrue::api::Client) -> Self {
    Self {
      token,
      gotrue_client: Arc::new(gotrue_client),
    }
  }
}

impl Action for RefreshTokenAction {
  type Future = Pin<Box<dyn Future<Output = Result<Self::Item, Self::Error>> + Send + Sync>>;
  type Item = ();
  type Error = GoTrueError;

  fn run(&mut self) -> Self::Future {
    let weak_token = Arc::downgrade(&self.token);
    let weak_gotrue_client = Arc::downgrade(&self.gotrue_client);
    Box::pin(async move {
      if let (Some(token), Some(gotrue_client)) =
        (weak_token.upgrade(), weak_gotrue_client.upgrade())
      {
        let (refresh_token, provider_access_token, provider_refresh_token) = {
          let mut token_write = token.write();
          let gotrue_resp_token = token_write.as_mut().ok_or(GoTrueError::NotLoggedIn(
            "fail to refresh user token".to_owned(),
          ))?;
          let refresh_token = gotrue_resp_token.refresh_token.as_str().to_owned();
          let provider_access_token = gotrue_resp_token.provider_access_token.take();
          let provider_refresh_token = gotrue_resp_token.provider_refresh_token.take();
          (refresh_token, provider_access_token, provider_refresh_token)
        };

        let mut access_token_resp = gotrue_client
          .token(&Grant::RefreshToken(RefreshTokenGrant { refresh_token }))
          .await?;

        // refresh does not preserve provider token and refresh token
        // so we need to set it manually to preserve this information
        access_token_resp.provider_access_token = provider_access_token;
        access_token_resp.provider_refresh_token = provider_refresh_token;

        token.write().set(access_token_resp);
      }
      Ok(())
    })
  }
}

pub(crate) struct RefreshTokenRetryCondition;
impl Condition<GoTrueError> for RefreshTokenRetryCondition {
  fn should_retry(&mut self, error: &GoTrueError) -> bool {
    error.is_network_error()
  }
}

pub async fn retry_connect(
  connect_provider: Arc<dyn WSClientConnectURLProvider>,
  state_notify: Weak<StateNotify>,
) -> Result<WebSocketStream, WSError> {
  // 使用指数退避策略：1秒开始，每次翻倍，最大30秒，最多重试10次
  let retry_strategy = ExponentialBackoff::from_millis(1000)
    .factor(2)
    .max_delay(Duration::from_secs(30))
    .take(10);
    
  let stream = RetryIf::spawn(
    retry_strategy,
    ConnectAction::new(connect_provider),
    RetryCondition { state_notify, attempt_count: 0 },
  )
  .await?;
  Ok(stream)
}

struct ConnectAction {
  connect_provider: Arc<dyn WSClientConnectURLProvider>,
}

impl ConnectAction {
  fn new(connect_provider: Arc<dyn WSClientConnectURLProvider>) -> Self {
    Self { connect_provider }
  }
}

impl Action for ConnectAction {
  type Future = Pin<Box<dyn Future<Output = Result<Self::Item, Self::Error>> + Send>>;
  type Item = WebSocketStream;
  type Error = WSError;

  fn run(&mut self) -> Self::Future {
    let connect_provider = self.connect_provider.clone();
    Box::pin(async move {
      info!("🔵WebSocket开始连接...");
      let url = connect_provider.connect_ws_url();
      let headers: HeaderMap = connect_provider.connect_info().await?.into();
      trace!("WebSocket URL: {}, Headers: {:?}", url, headers);
      
      let start_time = std::time::Instant::now();
      match connect_async(&url, headers).await {
        Ok(stream) => {
          let duration = start_time.elapsed();
          info!("🟢WebSocket连接成功，耗时: {:?}", duration);
          Ok(stream)
        },
        Err(e) => {
          let duration = start_time.elapsed();
          debug!("❌WebSocket连接失败，耗时: {:?}, 错误: {}", duration, e);
          Err(e.into())
        },
      }
    })
  }
}

struct RetryCondition {
  state_notify: Weak<parking_lot::Mutex<ConnectStateNotify>>,
  attempt_count: u32,
}
impl Condition<WSError> for RetryCondition {
  fn should_retry(&mut self, error: &WSError) -> bool {
    self.attempt_count += 1;
    
    match error {
      WSError::AuthError(err) => {
        debug!("认证错误，停止重试: {}", err);
        if let Some(state_notify) = self.state_notify.upgrade() {
          state_notify.lock().set_state(ConnectState::Unauthorized);
        }
        false
      },
      WSError::Close(msg) => {
        debug!("连接关闭，停止重试: {}", msg);
        false
      },
      WSError::LostConnection(msg) => {
        debug!("连接丢失，尝试重连 (第{}次): {}", self.attempt_count, msg);
        true
      },
      WSError::TungsteniteError(err) => {
        debug!("WebSocket错误，尝试重连 (第{}次): {}", self.attempt_count, err);
        true
      },
      WSError::Http(msg) => {
        debug!("HTTP错误，尝试重连 (第{}次): {}", self.attempt_count, msg);
        true
      },
      WSError::Internal(err) => {
        debug!("内部错误，尝试重连 (第{}次): {}", self.attempt_count, err);
        true
      },
    }
  }
}
