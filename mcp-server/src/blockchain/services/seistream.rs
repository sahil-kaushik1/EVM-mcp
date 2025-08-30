// src/blockchain/services/seistream.rs

use anyhow::Result;
use reqwest::Client;
use serde_json::Value;

const BASE: &str = "https://api.seistream.app";

pub async fn get_chain_info(client: &Client) -> Result<Value> {
    let url = format!("{}/chain/network", BASE);
    let res = client.get(&url).send().await?;
    let status = res.status();
    let body = res.text().await.unwrap_or_default();
    Ok(serde_json::from_str::<Value>(&body)
        .unwrap_or_else(|_| serde_json::json!({"status": status.as_u16(), "raw": body})))
}

pub async fn get_transaction_info(client: &Client, tx_hash: &str) -> Result<Value> {
    let url = format!("{}/transactions/evm/{}", BASE, tx_hash);
    let res = client.get(&url).send().await?;
    let status = res.status();
    let body = res.text().await.unwrap_or_default();
    Ok(serde_json::from_str::<Value>(&body)
        .unwrap_or_else(|_| serde_json::json!({"status": status.as_u16(), "raw": body})))
}

pub async fn get_transaction_history(client: &Client, address: &str, page: Option<u64>) -> Result<Value> {
    let mut url = format!("{}/accounts/evm/{}/transactions", BASE, address);
    if let Some(p) = page { url.push_str(&format!("?page={}", p)); }
    let res = client.get(&url).send().await?;
    let status = res.status();
    let body = res.text().await.unwrap_or_default();
    Ok(serde_json::from_str::<Value>(&body)
        .unwrap_or_else(|_| serde_json::json!({"status": status.as_u16(), "raw": body})))
}

pub async fn get_nft_metadata_erc721_items(client: &Client, contract: &str, page: Option<u64>) -> Result<Value> {
    let mut url = format!("{}/tokens/evm/erc721/{}/items", BASE, contract);
    if let Some(p) = page { url.push_str(&format!("?page={}", p)); }
    let res = client.get(&url).send().await?;
    let status = res.status();
    let body = res.text().await.unwrap_or_default();
    Ok(serde_json::from_str::<Value>(&body)
        .unwrap_or_else(|_| serde_json::json!({"status": status.as_u16(), "raw": body})))
}
