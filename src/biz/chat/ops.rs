use anyhow::anyhow;

use app_error::AppError;
use appflowy_ai_client::client::AppFlowyAIClient;
use database::chat;
use database::chat::chat_ops::{
  delete_answer_message_by_question_message_id, insert_answer_message,
  insert_answer_message_with_transaction, insert_chat, insert_question_message,
  select_chat_message_matching_reply_message_id, select_chat_messages,
  select_chat_messages_with_author_uuid,
};
use shared_entity::dto::chat_dto::{
  ChatAuthor, ChatAuthorType, ChatAuthorWithUuid, ChatMessage, ChatMessageWithAuthorUuid,
  CreateChatMessageParams, CreateChatParams, GetChatMessageParams, RepeatedChatMessage,
  RepeatedChatMessageWithAuthorUuid, UpdateChatMessageContentParams,
};
use sqlx::PgPool;
use tracing::{info, trace};

use uuid::Uuid;
use validator::Validate;

pub(crate) async fn create_chat(
  pg_pool: &PgPool,
  params: CreateChatParams,
  workspace_id: &Uuid,
) -> Result<(), AppError> {
  params.validate()?;
  trace!("[Chat] create chat {:?}", params);

  insert_chat(pg_pool, workspace_id, params).await?;
  Ok(())
}

pub(crate) async fn delete_chat(pg_pool: &PgPool, chat_id: &str) -> Result<(), AppError> {
  let mut txn = pg_pool.begin().await?;
  chat::chat_ops::delete_chat(&mut txn, chat_id).await?;
  txn.commit().await?;
  Ok(())
}

pub async fn update_chat_message(
  workspace_id: String,
  pg_pool: &PgPool,
  params: UpdateChatMessageContentParams,
  ai_client: AppFlowyAIClient,
  ai_model: &str,
) -> Result<(), AppError> {
  let mut txn = pg_pool.begin().await?;
  delete_answer_message_by_question_message_id(&mut txn, params.message_id).await?;
  chat::chat_ops::update_chat_message_content(&mut txn, &params).await?;
  txn.commit().await.map_err(|err| {
    AppError::Internal(anyhow!(
      "Failed to commit transaction to update chat message: {}",
      err
    ))
  })?;

  // TODO(nathan): query the metadata from the database
  let new_answer = ai_client
    .send_question(
      &workspace_id,
      &params.chat_id,
      params.message_id,
      &params.content,
      ai_model,
      None,
    )
    .await?;
  let _answer = insert_answer_message(
    pg_pool,
    ChatAuthor::ai(),
    &params.chat_id,
    new_answer.content,
    new_answer.metadata,
    params.message_id,
  )
  .await?;

  Ok(())
}

pub async fn generate_chat_message_answer(
  workspace_id: String,
  pg_pool: &PgPool,
  ai_client: AppFlowyAIClient,
  question_message_id: i64,
  chat_id: &str,
  ai_model: &str,
) -> Result<ChatMessage, AppError> {
  let (content, metadata) =
    chat::chat_ops::select_chat_message_content(pg_pool, question_message_id).await?;
  let new_answer = ai_client
    .send_question(
      &workspace_id,
      chat_id,
      question_message_id,
      &content,
      ai_model,
      Some(metadata),
    )
    .await
    .map_err(|err| AppError::AIServiceUnavailable(err.to_string()))?;

  info!("new_answer: {:?}", new_answer);
  // Save the answer to the database
  let mut txn = pg_pool.begin().await?;
  let message = insert_answer_message_with_transaction(
    &mut txn,
    ChatAuthor::ai(),
    chat_id,
    new_answer.content,
    new_answer.metadata.unwrap_or_default(),
    question_message_id,
  )
  .await?;
  txn.commit().await.map_err(|err| {
    AppError::Internal(anyhow!(
      "Failed to commit transaction to update chat message: {}",
      err
    ))
  })?;

  Ok(message)
}

pub async fn create_chat_message(
  pg_pool: &PgPool,
  uid: i64,
  user_uuid: Uuid,
  chat_id: String,
  params: CreateChatMessageParams,
) -> Result<ChatMessageWithAuthorUuid, AppError> {
  let author = ChatAuthorWithUuid::new(uid, user_uuid, ChatAuthorType::Human);
  
  // 首次尝试插入消息
  let result = insert_question_message(
    pg_pool,
    author.clone(),
    &chat_id,
    params.content.clone(),
  ).await;

  match result {
    Ok(question) => Ok(question),
    Err(AppError::InvalidRequest(ref msg)) if msg.contains("does not exist") => {
      // 聊天不存在，自动创建聊天
      trace!("Chat {} not found, auto-creating", chat_id);
      
      // 获取用户的workspace_id
      let workspace_id: Option<Uuid> = sqlx::query_scalar(
        "SELECT workspace_id FROM af_workspace_member WHERE uid = $1 LIMIT 1"
      )
      .bind(uid)
      .fetch_optional(pg_pool)
      .await?;
      
      let workspace_id = workspace_id
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("User workspace not found")))?;

      // 创建聊天记录
      let create_chat_params = CreateChatParams {
        chat_id: chat_id.clone(),
        name: "AI聊天".to_string(),
        rag_ids: vec![],
      };
      
      insert_chat(pg_pool, &workspace_id, create_chat_params).await?;
      trace!("Auto-created chat: {}", chat_id);

      // 重新尝试插入消息
      insert_question_message(
        pg_pool,
        author,
        &chat_id,
        params.content,
      ).await
    },
    Err(e) => Err(e),
  }
}

// Deprecated since v0.9.24
pub async fn get_chat_messages(
  pg_pool: &PgPool,
  params: GetChatMessageParams,
  chat_id: &str,
) -> Result<RepeatedChatMessage, AppError> {
  params.validate()?;

  let mut txn = pg_pool.begin().await?;
  let messages = select_chat_messages(&mut txn, chat_id, params).await?;
  txn.commit().await?;
  Ok(messages)
}

pub async fn get_chat_messages_with_author_uuid(
  pg_pool: &PgPool,
  params: GetChatMessageParams,
  chat_id: &str,
) -> Result<RepeatedChatMessageWithAuthorUuid, AppError> {
  params.validate()?;

  let mut txn = pg_pool.begin().await?;
  let messages = select_chat_messages_with_author_uuid(&mut txn, chat_id, params).await?;
  txn.commit().await?;
  Ok(messages)
}

pub async fn get_question_message(
  pg_pool: &PgPool,
  chat_id: &str,
  answer_message_id: i64,
) -> Result<Option<ChatMessage>, AppError> {
  let mut txn = pg_pool.begin().await?;
  let message =
    select_chat_message_matching_reply_message_id(&mut txn, chat_id, answer_message_id).await?;
  txn.commit().await?;
  Ok(message)
}
