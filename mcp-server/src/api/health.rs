use axum::{response::IntoResponse, Json};

pub async fn health_handler() -> impl IntoResponse {
    Json(serde_json::json!({"status": "ok"}))
}
