// src/api/contract.rs

use crate::AppState;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use serde_json::json;
use tracing::error;

#[derive(Deserialize)]
pub struct ContractPath {
    pub chain_id: String,
    pub address: String,
}

pub async fn get_contract_handler(
    State(state): State<AppState>,
    Path(params): Path<ContractPath>,
) -> impl IntoResponse {
    match state
        .evm_client
        .get_contract(&params.chain_id, &params.address)
        .await
    {
        Ok(contract) => (StatusCode::OK, Json(contract)).into_response(),
        Err(e) => {
            error!("Failed to get contract: {:?}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
        }
    }
}

pub async fn get_contract_code_handler(
    State(state): State<AppState>,
    Path(params): Path<ContractPath>,
) -> impl IntoResponse {
    match state
        .evm_client
        .get_contract_code(&params.chain_id, &params.address)
        .await
    {
        Ok(code) => (StatusCode::OK, Json(code)).into_response(),
        Err(e) => {
            error!("Failed to get contract code: {:?}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
        }
    }
}

pub async fn get_contract_transactions_handler(
    State(state): State<AppState>,
    Path(params): Path<ContractPath>,
) -> impl IntoResponse {
    match state
        .evm_client
        .get_contract_transactions(&params.chain_id, &params.address)
        .await
    {
        Ok(txs) => (StatusCode::OK, Json(txs)).into_response(),
        Err(e) => {
            error!("Failed to get contract transactions: {:?}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
        }
    }
}

pub async fn get_is_contract_handler(
    State(state): State<AppState>,
    Path(params): Path<ContractPath>,
) -> impl IntoResponse {
    match state
        .evm_client
        .is_contract(&params.chain_id, &params.address)
        .await
    {
        Ok(is_contract) => {
            (StatusCode::OK, Json(json!({ "is_contract": is_contract }))).into_response()
        }
        Err(e) => {
            error!("Failed to check is_contract: {:?}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
        }
    }
}

pub async fn get_contract_source_code_handler(
    State(state): State<AppState>,
    Path(params): Path<ContractPath>,
) -> impl IntoResponse {
    let etherscan_api_key = match &state.config.etherscan_api_key {
        Some(key) => key,
        None => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Etherscan API key not configured".to_string(),
            )
                .into_response();
        }
    };

    match state
        .evm_client
        .get_contract_source_code(&params.chain_id, &params.address, etherscan_api_key)
        .await
    {
        Ok(source_code) => (StatusCode::OK, Json(source_code)).into_response(),
        Err(e) => {
            error!("Failed to get contract source code: {:?}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
        }
    }
}
