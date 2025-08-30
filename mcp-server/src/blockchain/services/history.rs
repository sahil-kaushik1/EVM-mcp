use anyhow::{Result, anyhow};
use reqwest::Client;
use serde::Deserialize;
use tracing::{debug, info};

use crate::blockchain::models::{Transaction, TransactionHistoryResponse, TransactionType};

// --- Helper Structs for Deserializing the Seistream API Response ---

/// Represents a single transaction object from the "items" array in the API response.
/// This structure is now flattened to match the actual API output.
#[derive(Deserialize, Debug)]
struct SeiApiTransaction {
    hash: String,
    // The API uses 'from' and 'to', so we rename them during deserialization.
    #[serde(rename = "from")]
    from_address: String,
    #[serde(rename = "to")]
    to_address: Option<String>,
    value: String,
    timestamp: String,
}

/// Represents the top-level structure of the Seistream API response.
/// The key field is `items`, not `txs` or `transactions`.
#[derive(Deserialize, Debug)]
struct SeiApiResponse {
    items: Vec<SeiApiTransaction>,
}

/// Fetches transaction history for a given address using the public Seistream API.
///
/// This function queries the Seistream endpoint to get a list of the most recent
/// transactions for a specific EVM address on the Sei network.
pub async fn get_transaction_history(
    client: &Client,
    address: &str,
    limit: u64,
) -> Result<TransactionHistoryResponse> {
    info!(
        "Fetching transaction history for address: {} from Seistream API with limit: {}",
        address, limit
    );

    // Construct the API URL.
    let api_url = format!(
        "https://api.seistream.app/accounts/evm/{}/transactions?limit={}",
        address, limit
    );

    // Perform the GET request and get the response text for debugging.
    let response_text = client.get(&api_url).send().await?.text().await?;
    debug!("Received response from Seistream API: {}", response_text);

    // Deserialize the JSON response text into our corrected structs.
    let api_response: SeiApiResponse = serde_json::from_str(&response_text).map_err(|e| {
        anyhow!(
            "Error decoding Seistream API response: {}. Response body: {}",
            e,
            response_text
        )
    })?;

    // Map the API response to our internal `Transaction` model.
    let transactions: Vec<Transaction> = api_response
        .items // Use .items, which matches the actual API response
        .into_iter()
        .map(|tx| Transaction {
            tx_hash: tx.hash,
            from_address: tx.from_address,
            to_address: tx.to_address.unwrap_or_else(|| "N/A".to_string()),
            amount: tx.value, // Value is now a direct field
            denom: "usei".to_string(),
            timestamp: tx.timestamp,
            transaction_type: TransactionType::Native,
            contract_address: None,
        })
        .collect();

    // Return the final response structure.
    Ok(TransactionHistoryResponse { transactions })
}
