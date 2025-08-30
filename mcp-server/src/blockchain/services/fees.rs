use anyhow::{Result, anyhow};
use ethers_core::types::U256;
use reqwest::Client;
use serde_json::{Value, json};
use tracing::info;
use crate::blockchain::models::{EstimateFeesRequest, EstimateFeesResponse};

pub async fn estimate_fees(
    client: &Client,
    rpc_url: &str,
    request: &EstimateFeesRequest,
) -> Result<EstimateFeesResponse> {
    info!(
        "Attempting to estimate fees for a transaction on rpc_url: {}",
        rpc_url
    );

    let amount = U256::from_dec_str(&request.amount)
        .map_err(|e| anyhow!("Invalid amount format: {}", e))?;
    let amount_hex = format!("0x{:x}", amount);

    let estimate_gas_payload = json!({
        "jsonrpc": "2.0",
        "method": "eth_estimateGas",
        "params": [{
            "from": request.from,
            "to": request.to,
            "value": amount_hex,
        }],
        "id": 1,
    });

    let res_gas: Value = client
        .post(rpc_url)
        .json(&estimate_gas_payload)
        .send()
        .await?
        .json()
        .await?;

    let estimated_gas_hex = res_gas["result"].as_str().ok_or_else(|| {
        anyhow!(
            "RPC response for estimateGas missing 'result' field: {:?}",
            res_gas
        )
    })?;

    let estimated_gas_u256 =
        U256::from_str_radix(estimated_gas_hex.trim_start_matches("0x"), 16)?;

    let gas_price_payload = json!({
        "jsonrpc": "2.0",
        "method": "eth_gasPrice",
        "params": [],
        "id": 2
    });

    let res_price: Value = client
        .post(rpc_url)
        .json(&gas_price_payload)
        .send()
        .await?
        .json()
        .await?;

    let gas_price_hex = res_price["result"].as_str().ok_or_else(|| {
        anyhow!(
            "RPC response for gasPrice missing 'result' field: {:?}",
            res_price
        )
    })?;

    let gas_price_u256 = U256::from_str_radix(gas_price_hex.trim_start_matches("0x"), 16)?;

    let total_fee_u256 = gas_price_u256
        .checked_mul(estimated_gas_u256)
        .ok_or_else(|| anyhow!("Fee calculation overflow"))?;

    Ok(EstimateFeesResponse {
        estimated_gas: estimated_gas_u256.to_string(),
        gas_price: gas_price_u256.to_string(),
        total_fee: total_fee_u256.to_string(),
        denom: "usei".to_string(),
    })
}
