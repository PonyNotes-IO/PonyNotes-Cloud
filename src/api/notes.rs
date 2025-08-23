use actix_web::{web, Result, Scope};
use shared_entity::response::{AppResponse, JsonAppResponse};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::biz::authentication::jwt::UserUuid;
use crate::state::AppState;
use shared_entity::response::AppResponseError;
use tracing::instrument;
use actix_web::web::Data;

// 笔记数据传输对象
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateNoteRequest {
    pub title: String,
    pub content: String,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateNoteRequest {
    pub title: Option<String>,
    pub content: Option<String>,
    pub tags: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteResponse {
    pub id: String,
    pub title: String,
    pub content: String,
    pub tags: Vec<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub user_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListNotesResponse {
    pub notes: Vec<NoteResponse>,
    pub total: usize,
}

// 路由配置
pub fn notes_scope() -> Scope {
    web::scope("/api/notes")
        .service(web::resource("").route(web::post().to(create_note_handler)))
        .service(web::resource("").route(web::get().to(list_notes_handler)))
        .service(web::resource("/{note_id}").route(web::get().to(get_note_handler)))
        .service(web::resource("/{note_id}").route(web::put().to(update_note_handler)))
        .service(web::resource("/{note_id}").route(web::delete().to(delete_note_handler)))
}

// 处理函数
#[instrument(skip(state, auth), err)]
async fn create_note_handler(
    auth: UserUuid,
    state: Data<AppState>,
    payload: web::Json<CreateNoteRequest>,
) -> Result<JsonAppResponse<NoteResponse>, AppResponseError> {
    let note_id = Uuid::new_v4().to_string();
    let current_time = chrono::Utc::now().timestamp();
    let _ = state; // 避免未使用变量警告
    
    // 这里模拟创建笔记的逻辑
    // 在实际实现中，您需要将笔记保存到数据库
    let note = NoteResponse {
        id: note_id,
        title: payload.title.clone(),
        content: payload.content.clone(),
        tags: payload.tags.clone(),
        created_at: current_time,
        updated_at: current_time,
        user_id: auth.as_uuid().to_string(),
    };

    Ok(AppResponse::Ok().with_data(note).into())
}

#[instrument(skip(state, auth), err)]
async fn list_notes_handler(
    auth: UserUuid,
    state: Data<AppState>,
) -> Result<JsonAppResponse<ListNotesResponse>, AppResponseError> {
    let _ = state; // 避免未使用变量警告
    
    // 这里模拟获取笔记列表的逻辑
    // 在实际实现中，您需要从数据库查询用户的笔记
    let notes = vec![
        NoteResponse {
            id: Uuid::new_v4().to_string(),
            title: "示例笔记".to_string(),
            content: "这是一个示例笔记内容".to_string(),
            tags: vec!["示例".to_string(), "测试".to_string()],
            created_at: chrono::Utc::now().timestamp(),
            updated_at: chrono::Utc::now().timestamp(),
            user_id: auth.as_uuid().to_string(),
        }
    ];

    let response = ListNotesResponse {
        total: notes.len(),
        notes,
    };

    Ok(AppResponse::Ok().with_data(response).into())
}

#[instrument(skip(state, auth), err)]
async fn get_note_handler(
    auth: UserUuid,
    state: Data<AppState>,
    path: web::Path<String>,
) -> Result<JsonAppResponse<NoteResponse>, AppResponseError> {
    let note_id = path.into_inner();
    let _ = state; // 避免未使用变量警告
    
    // 这里模拟获取单个笔记的逻辑
    // 在实际实现中，您需要从数据库查询指定的笔记
    let note = NoteResponse {
        id: note_id,
        title: "示例笔记".to_string(),
        content: "这是一个示例笔记内容".to_string(),
        tags: vec!["示例".to_string(), "测试".to_string()],
        created_at: chrono::Utc::now().timestamp(),
        updated_at: chrono::Utc::now().timestamp(),
        user_id: auth.as_uuid().to_string(),
    };

    Ok(AppResponse::Ok().with_data(note).into())
}

#[instrument(skip(state, auth), err)]
async fn update_note_handler(
    auth: UserUuid,
    state: Data<AppState>,
    path: web::Path<String>,
    payload: web::Json<UpdateNoteRequest>,
) -> Result<JsonAppResponse<NoteResponse>, AppResponseError> {
    let note_id = path.into_inner();
    let current_time = chrono::Utc::now().timestamp();
    let _ = state; // 避免未使用变量警告
    
    // 这里模拟更新笔记的逻辑
    // 在实际实现中，您需要更新数据库中的笔记
    let note = NoteResponse {
        id: note_id,
        title: payload.title.clone().unwrap_or_else(|| "示例笔记".to_string()),
        content: payload.content.clone().unwrap_or_else(|| "这是一个示例笔记内容".to_string()),
        tags: payload.tags.clone().unwrap_or_else(|| vec!["示例".to_string(), "测试".to_string()]),
        created_at: current_time - 3600, // 假设创建时间是1小时前
        updated_at: current_time,
        user_id: auth.as_uuid().to_string(),
    };

    Ok(AppResponse::Ok().with_data(note).into())
}

#[instrument(skip(state, auth), err)]
async fn delete_note_handler(
    auth: UserUuid,
    state: Data<AppState>,
    path: web::Path<String>,
) -> Result<JsonAppResponse<()>, AppResponseError> {
    let _note_id = path.into_inner();
    let _ = (auth, state); // 避免未使用变量警告
    
    // 这里模拟删除笔记的逻辑
    // 在实际实现中，您需要从数据库删除指定的笔记
    
    Ok(AppResponse::Ok().with_data(()).into())
}
