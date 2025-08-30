use crate::blockchain::{
    client::EvmClient,
    models::{EventQuery, SearchEventsResponse},
};
use anyhow::Result;

/// Searches for transactions based on event criteria.
/// Note: Event search is currently a beta feature and returns an empty result.
/// Future work: implement EVM (ethers-rs) and Cosmos event filtering.
pub async fn search_events(
    _client: &EvmClient,
    _query: EventQuery,
) -> Result<SearchEventsResponse> {
    Ok(SearchEventsResponse { txs: vec![], total_count: 0 })
}
