use crate::blockchain::client::SeiClient;
use crate::AppState;
use axum::{
    extract::{Query, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;

#[derive(Deserialize, Debug)]
pub struct SearchQuery {
    pub event_type: Option<String>,
    pub attribute_key: Option<String>,
    pub attribute_value: Option<String>,
    pub from_block: Option<u64>,
    pub to_block: Option<u64>,
    pub page: Option<u32>,
    pub per_page: Option<u8>,
}

#[derive(Deserialize, Debug)]
pub struct ContractEventsQuery {
    pub contract_address: String,
    pub event_type: Option<String>,
    pub from_block: Option<u64>,
    pub to_block: Option<u64>,
    pub page: Option<u32>,
    pub per_page: Option<u8>,
}

/// GET /search-events
/// Searches for past transaction events based on various criteria.
pub async fn search_events(
    State(state): State<AppState>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let client = SeiClient::new(&state.config.chain_rpc_urls, &state.config.websocket_url);

    let event_query = crate::blockchain::models::EventQuery {
        contract_address: None,
        event_type: query.event_type.clone(),
        attribute_key: query.attribute_key.clone(),
        attribute_value: query.attribute_value.clone(),
        from_block: query.from_block,
        to_block: query.to_block,
    };

    match crate::blockchain::services::event::search_events(&client, event_query)
        .await
    {
        Ok(result) => Ok(Json(serde_json::to_value(result).map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Serialization error: {}", e),
            )
        })?)),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string())),
    }
}

/// GET /get-contract-events
/// Fetches historical events for a specific contract.
pub async fn get_contract_events(
    State(state): State<AppState>,
    Query(query): Query<ContractEventsQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let client = SeiClient::new(&state.config.chain_rpc_urls, &state.config.websocket_url);

    let event_query = crate::blockchain::models::EventQuery {
        contract_address: Some(query.contract_address.clone()),
        event_type: query.event_type.clone(),
        attribute_key: None,   // Not used for direct contract event search
        attribute_value: None, // Not used for direct contract event search
        from_block: query.from_block,
        to_block: query.to_block,
    };

    let _page = query.page.unwrap_or(1);
    // Remove explicit pagination handling here; let the service handle it
    match crate::blockchain::services::event::search_events(&client, event_query).await
    {
        Ok(result) => Ok(Json(serde_json::to_value(result).map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Serialization error: {}", e),
            )
        })?)),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string())),
    }
}

/// GET /subscribe-contract-events?contract_address={address}
/// Subscribes to live events from a specific contract via WebSocket.
pub async fn subscribe_contract_events(
    State(_state): State<AppState>,
    Query(query): Query<ContractEventsQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    // For now, return a message indicating WebSocket support is not yet implemented
    // TODO: Implement proper WebSocket support for axum
    Ok(Json(serde_json::json!({
        "message": "WebSocket subscription not yet implemented for axum",
        "contract_address": query.contract_address
    })))
}
