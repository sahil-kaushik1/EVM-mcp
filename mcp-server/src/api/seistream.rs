// src/api/seistream.rs

use crate::AppState;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use reqwest::Client;
use serde::Deserialize;
use tracing::error;

#[derive(Deserialize)]
pub struct AddressPath {
    pub address: String,
}

#[derive(Deserialize)]
pub struct TxPath {
    pub hash: String,
}

#[derive(Deserialize)]
pub struct PageQuery {
    pub page: Option<u64>,
}

pub async fn get_chain_info_handler(State(_state): State<AppState>) -> impl IntoResponse {
    let client = Client::new();
    match crate::blockchain::services::seistream::get_chain_info(&client).await {
        Ok(v) => (StatusCode::OK, Json(v)).into_response(),
        Err(e) => {
            error!("Failed to get chain info: {:?}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
        }
    }
}

pub async fn get_transaction_info_handler(
    State(_state): State<AppState>,
    Path(TxPath { hash }): Path<TxPath>,
) -> impl IntoResponse {
    let client = Client::new();
    match crate::blockchain::services::seistream::get_transaction_info(&client, &hash).await {
        Ok(v) => (StatusCode::OK, Json(v)).into_response(),
        Err(e) => {
            error!("Failed to get transaction info: {:?}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
        }
    }
}

pub async fn get_transaction_history_handler(
    State(_state): State<AppState>,
    Path(AddressPath { address }): Path<AddressPath>,
    Query(PageQuery { page }): Query<PageQuery>,
) -> impl IntoResponse {
    let client = Client::new();
    match crate::blockchain::services::seistream::get_transaction_history(&client, &address, page)
        .await
    {
        Ok(v) => (StatusCode::OK, Json(v)).into_response(),
        Err(e) => {
            error!("Failed to get transaction history: {:?}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
        }
    }
}

pub async fn get_nft_metadata_items_handler(
    State(_state): State<AppState>,
    Path(AddressPath { address }): Path<AddressPath>,
    Query(PageQuery { page }): Query<PageQuery>,
) -> impl IntoResponse {
    let client = Client::new();
    match crate::blockchain::services::seistream::get_nft_metadata_erc721_items(
        &client, &address, page,
    )
    .await
    {
        Ok(v) => (StatusCode::OK, Json(v)).into_response(),
        Err(e) => {
            error!("Failed to get NFT metadata: {:?}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
        }
    }
}
