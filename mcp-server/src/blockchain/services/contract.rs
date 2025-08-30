// src/blockchain/services/contract.rs

use anyhow::Result;
use reqwest::Client;
use serde_json::Value;

// Seistream contract API (chain-agnostic base; network inferred by address)
const SEISCAN_API_BASE: &str = "https://api.seistream.app/contracts/evm";
// Seistream Cosmos contracts base (for native Sei addresses like sei1...)
const SEISCAN_COSMOS_API_BASE: &str = "https://api.seistream.app/contracts/cosmos";

fn get_seiscan_api_base(chain_id: &str) -> &str {
    // Currently the API host/path does not vary per chain; keep function for future flexibility.
    let _ = chain_id; // suppress unused warning in case of future use
    SEISCAN_API_BASE
}

pub async fn get_contract(client: &Client, chain_id: &str, address: &str) -> Result<Value> {
    let base_url = get_seiscan_api_base(chain_id);
    let url = format!("{}/{}", base_url, address);
    let res = client.get(&url).send().await?;
    let status = res.status();
    let body = res.text().await.unwrap_or_default();
    match serde_json::from_str::<Value>(&body) {
        Ok(v) => Ok(v),
        Err(_) => {
            // Return a wrapper to avoid decode errors while surfacing raw body
            Ok(serde_json::json!({
                "status": status.as_u16(),
                "raw": body
            }))
        }
    }
}

pub async fn get_contract_code(client: &Client, chain_id: &str, address: &str) -> Result<Value> {
    let base_url = get_seiscan_api_base(chain_id);
    let url = format!("{}/{}/code", base_url, address);
    let res = client.get(&url).send().await?;
    let status = res.status();
    let body = res.text().await.unwrap_or_default();
    match serde_json::from_str::<Value>(&body) {
        Ok(v) => Ok(normalize_contract_code(v)),
        Err(_) => Ok(serde_json::json!({ "status": status.as_u16(), "raw": body })),
    }
}

pub async fn get_contract_transactions(
    client: &Client,
    chain_id: &str,
    address: &str,
) -> Result<Value> {
    let base_url = get_seiscan_api_base(chain_id);
    let url = format!("{}/{}/transactions", base_url, address);
    let res = client.get(&url).send().await?;
    let status = res.status();
    let body = res.text().await.unwrap_or_default();
    match serde_json::from_str::<Value>(&body) {
        Ok(v) => Ok(v),
        Err(_) => Ok(serde_json::json!({ "status": status.as_u16(), "raw": body })),
    }
}

/// Check if a Cosmos (Sei native) address is a smart contract by querying Seistream.
/// Returns true if the endpoint responds with 200 and a plausible contract JSON, false on 404.
/// Any non-200/404 status or network error is propagated as an error.
pub async fn is_cosmos_contract(client: &Client, address: &str) -> Result<bool> {
    let url = format!("{}/{}", SEISCAN_COSMOS_API_BASE, address);
    let res = client.get(&url).send().await?;
    let status = res.status();
    if status.as_u16() == 404 {
        return Ok(false);
    }
    let body = res.text().await.unwrap_or_default();
    if !status.is_success() {
        // Surface upstream error for observability
        anyhow::bail!("Upstream error {}: {}", status.as_u16(), body);
    }
    // 200 means the address corresponds to a contract document
    Ok(true)
}

/// Check if an EVM address is a smart contract via Seistream EVM contracts API.
pub async fn is_evm_contract(client: &Client, address: &str) -> Result<bool> {
    let url = format!("{}/{}", SEISCAN_API_BASE, address);
    let res = client.get(&url).send().await?;
    let status = res.status();
    if status.as_u16() == 404 {
        return Ok(false);
    }
    let body = res.text().await.unwrap_or_default();
    if !status.is_success() {
        anyhow::bail!("Upstream error {}: {}", status.as_u16(), body);
    }
    Ok(true)
}

// Normalize upstream contract code JSON into the strict schema required by clients.
// Target schema:
// {
//   "abi": ["string"],
//   "compilerSettings": [ { ... } ],
//   "externalLibraries": [ { ... } ],
//   "runtimeCode": "string",
//   "creationCode": "string",
//   "sources": [ { "name": "string", "sourceCode": "string" } ]
// }
fn normalize_contract_code(v: Value) -> Value {
    use serde_json::json;

    // abi: coerce any array elements into strings; if object, stringify; else empty
    let abi_arr = v.get("abi").and_then(|x| x.as_array()).cloned().unwrap_or_default();
    let abi: Vec<String> = abi_arr
        .into_iter()
        .map(|el| match el {
            Value::String(s) => s,
            other => other.to_string(),
        })
        .collect();

    // compilerSettings: accept object or array; coerce to array of objects
    let compiler_settings = match v.get("compilerSettings") {
        Some(Value::Array(a)) => a.clone(),
        Some(Value::Object(_)) => vec![v.get("compilerSettings").unwrap().clone()],
        _ => vec![],
    };

    // externalLibraries: accept array or object; coerce to array
    let external_libraries = match v.get("externalLibraries") {
        Some(Value::Array(a)) => a.clone(),
        Some(Value::Object(_)) => vec![v.get("externalLibraries").unwrap().clone()],
        _ => vec![],
    };

    // runtimeCode / creationCode: support camelCase and snake_case fallbacks
    let runtime_code = v
        .get("runtimeCode")
        .or_else(|| v.get("runtime_code"))
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string();
    let creation_code = v
        .get("creationCode")
        .or_else(|| v.get("creation_code"))
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string();

    // sources: if array, map to desired; if object map name -> {content|source|sourceCode}, convert to array
    let sources = match v.get("sources") {
        Some(Value::Array(arr)) => {
            let mapped: Vec<Value> = arr
                .iter()
                .map(|item| {
                    if let Value::Object(obj) = item {
                        let name = obj.get("name").and_then(|x| x.as_str()).unwrap_or("").to_string();
                        let sc = obj
                            .get("sourceCode")
                            .or_else(|| obj.get("content"))
                            .or_else(|| obj.get("source"))
                            .and_then(|x| x.as_str())
                            .unwrap_or("")
                            .to_string();
                        json!({ "name": name, "sourceCode": sc })
                    } else {
                        json!({ "name": "", "sourceCode": item.to_string() })
                    }
                })
                .collect();
            mapped
        }
        Some(Value::Object(map)) => {
            let mut out: Vec<Value> = Vec::new();
            for (name, val) in map.iter() {
                let source_code = match val {
                    Value::String(s) => s.clone(),
                    Value::Object(o) => o
                        .get("content")
                        .or_else(|| o.get("source"))
                        .or_else(|| o.get("sourceCode"))
                        .and_then(|x| x.as_str())
                        .unwrap_or("")
                        .to_string(),
                    other => other.to_string(),
                };
                out.push(json!({ "name": name, "sourceCode": source_code }));
            }
            out
        }
        Some(Value::String(s)) => vec![json!({ "name": "", "sourceCode": s })],
        _ => vec![],
    };

    json!({
        "abi": abi,
        "compilerSettings": compiler_settings,
        "externalLibraries": external_libraries,
        "runtimeCode": runtime_code,
        "creationCode": creation_code,
        "sources": sources
    })
}

/// Get CosmWasm contract info via node REST API
pub async fn get_cosmos_contract(client: &Client, rpc_url: &str, address: &str) -> Result<Value> {
    let url = format!("{}/cosmwasm/wasm/v1/contract/{}", rpc_url.trim_end_matches('/'), address);
    let res = client.get(&url).send().await?;
    let status = res.status();
    let body = res.text().await.unwrap_or_default();
    if !status.is_success() {
        anyhow::bail!("Cosmos contract lookup {}: {}", status.as_u16(), body);
    }
    Ok(serde_json::from_str(&body).unwrap_or_else(|_| serde_json::json!({ "raw": body })))
}

/// Get CosmWasm code info for a contract via node REST API
pub async fn get_cosmos_contract_code(client: &Client, rpc_url: &str, address: &str) -> Result<Value> {
    // First fetch contract to discover code_id
    let contract = get_cosmos_contract(client, rpc_url, address).await?;
    let code_id = contract
        .get("contract_info")
        .and_then(|ci| ci.get("code_id"))
        .and_then(|c| c.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing contract_info.code_id in response"))?;
    let url = format!("{}/cosmwasm/wasm/v1/code/{}", rpc_url.trim_end_matches('/'), code_id);
    let res = client.get(&url).send().await?;
    let status = res.status();
    let body = res.text().await.unwrap_or_default();
    if !status.is_success() {
        anyhow::bail!("Cosmos code lookup {}: {}", status.as_u16(), body);
    }
    Ok(serde_json::from_str(&body).unwrap_or_else(|_| serde_json::json!({ "raw": body })))
}

/// Node-based contract existence check (no third-party service)
pub async fn is_cosmos_contract_node(client: &Client, rpc_url: &str, address: &str) -> Result<bool> {
    let url = format!("{}/cosmwasm/wasm/v1/contract/{}", rpc_url.trim_end_matches('/'), address);
    let res = client.get(&url).send().await?;
    let status = res.status();
    if status.as_u16() == 404 { return Ok(false); }
    if !status.is_success() {
        let body = res.text().await.unwrap_or_default();
        anyhow::bail!("Cosmos contract lookup {}: {}", status.as_u16(), body);
    }
    Ok(true)
}

/// List transactions that involve a CosmWasm contract using the node REST API
pub async fn get_cosmos_contract_transactions(
    client: &Client,
    rpc_url: &str,
    address: &str,
    page: Option<u64>,
    limit: Option<u64>,
) -> Result<Value> {
    let base = format!("{}/cosmos/tx/v1beta1/txs", rpc_url.trim_end_matches('/'));
    let mut req = client
        .get(&base)
        .query(&[("order_by", "ORDER_BY_DESC")]);
    if let Some(p) = page { req = req.query(&[("pagination.page", &p.to_string())]); }
    if let Some(l) = limit { req = req.query(&[("pagination.limit", &l.to_string())]); }
    // events=wasm._contract_address='sei1...'
    let ev = format!("wasm._contract_address='{}'", address);
    req = req.query(&[("events", &ev)]);

    let res = req.send().await?;
    let status = res.status();
    let body = res.text().await.unwrap_or_default();
    if !status.is_success() {
        anyhow::bail!("Cosmos tx search {}: {}", status.as_u16(), body);
    }
    Ok(serde_json::from_str(&body).unwrap_or_else(|_| serde_json::json!({ "raw": body })))
}
