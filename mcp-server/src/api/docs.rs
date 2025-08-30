use axum::{extract::State, response::Redirect};
use crate::AppState;

// Simple HTTP handler that redirects to Sei documentation
pub async fn redirect_to_seidocs_handler(State(_state): State<AppState>) -> Redirect {
    Redirect::to("https://docs.sei.io/")
}
