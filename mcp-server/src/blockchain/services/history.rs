use anyhow::Result;
use reqwest::Client;
use tracing::info;

use crate::blockchain::models::{Transaction, TransactionHistoryResponse, TransactionType};

/// Fetches transaction history for a given address.
/// Note: Standard EVM RPC doesn't provide transaction history.
/// This would require an indexing service or third-party API.
pub async fn get_transaction_history(
    _client: &Client,
    address: &str,
    _limit: u64,
) -> Result<TransactionHistoryResponse> {
    info!(
        "Transaction history for address: {} - not available via standard EVM RPC",
        address
    );

    // Return empty result with a note
    let transactions = vec![Transaction {
        tx_hash: "N/A".to_string(),
        from_address: "N/A".to_string(),
        to_address: "N/A".to_string(),
        amount: "0".to_string(),
        denom: "wei".to_string(),
        timestamp: "N/A".to_string(),
        transaction_type: TransactionType::Native,
        contract_address: None,
    }];

    Ok(TransactionHistoryResponse { transactions })
}
