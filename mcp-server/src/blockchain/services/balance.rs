use crate::blockchain::models::BalanceResponse;
use anyhow::{anyhow, Result, Context};
use reqwest::Client;
use serde_json::{json, Value};
use tracing::{error};

pub async fn get_balance(client: &Client, rpc_url: &str, address: &str, is_native: bool) -> Result<BalanceResponse> {
    if is_native {
        // Native SEI (Cosmos) balance query
        let url = format!("{}/cosmos/bank/v1beta1/balances/{}", rpc_url.trim_end_matches('/'), address);
        let res: Value = client.get(&url).send().await?.json().await?;
        let balances = res["balances"].as_array().context("No balances array in response")?;
        // Find the 'usei' denom or fallback to the first
        let balance = balances.iter().find(|b| b["denom"] == "usei").or(balances.first());
        if let Some(bal) = balance {
            let amount = bal["amount"].as_str().unwrap_or("0").to_string();
            let denom = bal["denom"].as_str().unwrap_or("").to_string();
            Ok(BalanceResponse { amount, denom })
        } else {
            Ok(BalanceResponse { amount: "0".to_string(), denom: "usei".to_string() })
        }
    } else {
        // EVM balance logic
        let payload = json!({
            "jsonrpc": "2.0",
            "method": "eth_getBalance",
            "params": [address, "latest"],
            "id": 1
        });
        let res: Value = client
            .post(rpc_url)
            .json(&payload)
            .send()
            .await?
            .json()
            .await?;
        let result = res["result"]
            .as_str()
            .ok_or_else(|| anyhow!("RPC response missing 'result' field: {:?}", res))?;
        let amount_decimal = u128::from_str_radix(result.trim_start_matches("0x"), 16)
            .map(|val| val.to_string())
            .unwrap_or_else(|_| {
                error!(
                    "Failed to parse hex balance '{}' to u128. Defaulting to '0'.",
                    result
                );
                "0".to_string()
            });
        Ok(BalanceResponse {
            amount: amount_decimal,
            // For EVM chains, the native balance is returned in wei
            denom: "wei".to_string(),
        })
    }
}
