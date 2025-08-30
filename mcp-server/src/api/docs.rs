use axum::{extract::State, response::Redirect};
use crate::AppState;

// Simple HTTP handler that redirects to EVM documentation
pub async fn redirect_to_evmdocs_handler(State(_state): State<AppState>) -> Redirect {
    Redirect::to("https://ethereum.org/en/developers/docs/")
}
