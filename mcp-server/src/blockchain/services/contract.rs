// src/blockchain/services/contract.rs

use anyhow::{anyhow, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Deserialize, Debug)]
struct EtherscanContractResponse {
    status: String,
    message: String,
    result: Vec<EtherscanContractResult>,
}

#[derive(Deserialize, Debug, Serialize)]
struct EtherscanContractResult {
    #[serde(rename = "SourceCode")]
    source_code: String,
    #[serde(rename = "ABI")]
    abi: String,
    #[serde(rename = "ContractName")]
    contract_name: String,
    #[serde(rename = "CompilerVersion")]
    compiler_version: String,
    #[serde(rename = "OptimizationUsed")]
    optimization_used: String,
    #[serde(rename = "Runs")]
    runs: String,
    #[serde(rename = "ConstructorArguments")]
    constructor_arguments: String,
    #[serde(rename = "EVMVersion")]
    evm_version: String,
    #[serde(rename = "Library")]
    library: String,
    #[serde(rename = "LicenseType")]
    license_type: String,
    #[serde(rename = "Proxy")]
    proxy: String,
    #[serde(rename = "Implementation")]
    implementation: String,
    #[serde(rename = "SwarmSource")]
    swarm_source: String,
}

// Generic EVM contract functions using standard RPC calls

pub async fn get_contract(client: &Client, rpc_url: &str, address: &str) -> Result<Value> {
    // Use eth_getCode to check if address is a contract
    let payload = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "eth_getCode",
        "params": [address, "latest"],
        "id": 1
    });

    let res = client.post(rpc_url).json(&payload).send().await?;
    let status = res.status();
    let body = res.text().await.unwrap_or_default();

    match serde_json::from_str::<Value>(&body) {
        Ok(v) => {
            if let Some(result) = v.get("result").and_then(|r| r.as_str()) {
                if result == "0x" {
                    // Not a contract
                    Ok(serde_json::json!({
                        "is_contract": false,
                        "address": address
                    }))
                } else {
                    // Is a contract
                    Ok(serde_json::json!({
                        "is_contract": true,
                        "address": address,
                        "code": result
                    }))
                }
            } else {
                Ok(serde_json::json!({
                    "status": status.as_u16(),
                    "raw": body
                }))
            }
        }
        Err(_) => Ok(serde_json::json!({
            "status": status.as_u16(),
            "raw": body
        })),
    }
}

pub async fn get_contract_code(client: &Client, rpc_url: &str, address: &str) -> Result<Value> {
    // Use eth_getCode to get contract bytecode
    let payload = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "eth_getCode",
        "params": [address, "latest"],
        "id": 1
    });

    let res = client.post(rpc_url).json(&payload).send().await?;
    let status = res.status();
    let body = res.text().await.unwrap_or_default();

    match serde_json::from_str::<Value>(&body) {
        Ok(v) => {
            if let Some(result) = v.get("result").and_then(|r| r.as_str()) {
                Ok(serde_json::json!({
                    "address": address,
                    "code": result,
                    "runtimeCode": result
                }))
            } else {
                Ok(serde_json::json!({ "status": status.as_u16(), "raw": body }))
            }
        }
        Err(_) => Ok(serde_json::json!({ "status": status.as_u16(), "raw": body })),
    }
}

pub async fn get_contract_transactions(
    _client: &Client,
    _rpc_url: &str,
    address: &str,
) -> Result<Value> {
    // EVM RPC doesn't have a standard way to get contract transactions
    // This would require indexing service or third-party API
    Ok(serde_json::json!({
        "message": "Contract transaction history not available via standard EVM RPC",
        "address": address,
        "note": "Consider using a blockchain explorer API for transaction history"
    }))
}

/// Check if an EVM address is a smart contract via eth_getCode.
pub async fn is_evm_contract(client: &Client, rpc_url: &str, address: &str) -> Result<bool> {
    let payload = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "eth_getCode",
        "params": [address, "latest"],
        "id": 1
    });

    let res = client.post(rpc_url).json(&payload).send().await?;
    let body = res.text().await.unwrap_or_default();

    match serde_json::from_str::<Value>(&body) {
        Ok(v) => {
            if let Some(result) = v.get("result").and_then(|r| r.as_str()) {
                // If code is "0x", it's not a contract
                Ok(result != "0x")
            } else {
                Ok(false)
            }
        }
        Err(_) => Ok(false),
    }
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
    let abi_arr = v
        .get("abi")
        .and_then(|x| x.as_array())
        .cloned()
        .unwrap_or_default();
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
                        let name = obj
                            .get("name")
                            .and_then(|x| x.as_str())
                            .unwrap_or("")
                            .to_string();
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

/// Get contract source code from Etherscan API
pub async fn get_contract_source_code(
    client: &Client,
    chain_id: &str,
    address: &str,
    etherscan_api_key: &str,
) -> Result<Value> {
    // Map chain IDs to Etherscan base URLs
    let base_url = match chain_id {
        "1" => "https://api.etherscan.io/v2/api",
        "11155111" => "https://api-sepolia.etherscan.io/v2/api",
        "324" | "300" => {
            // zkSync chains don't have Etherscan support, return error
            return Err(anyhow!("Etherscan API not supported for zkSync chains"));
        }
        _ => return Err(anyhow!("Unsupported chain ID for Etherscan: {}", chain_id)),
    };

    // Build the Etherscan API URL for getting source code
    let url = format!(
        "{}?chainid={}&module=contract&action=getsourcecode&address={}&apikey={}",
        base_url, chain_id, address, etherscan_api_key
    );

    let res: EtherscanContractResponse = client
        .get(&url)
        .send()
        .await?
        .json()
        .await
        .map_err(|e| anyhow!("Failed to parse Etherscan response: {}", e))?;

    if res.status != "1" {
        return Err(anyhow!(
            "Etherscan API error: {} - {}",
            res.message,
            if res.result.is_empty() {
                "No result".to_string()
            } else {
                res.result[0].source_code.clone()
            }
        ));
    }

    if res.result.is_empty() {
        return Err(anyhow!(
            "No contract source code found for address: {}",
            address
        ));
    }

    let contract = &res.result[0];

    // Parse ABI if it's valid JSON
    let abi_value: Value = if contract.abi.is_empty() {
        Value::Array(vec![])
    } else {
        serde_json::from_str(&contract.abi)
            .unwrap_or_else(|_| Value::String("Invalid ABI format".to_string()))
    };

    // Return structured response
    Ok(serde_json::json!({
        "address": address,
        "chainId": chain_id,
        "contractName": contract.contract_name,
        "sourceCode": contract.source_code,
        "abi": abi_value,
        "compiler": {
            "version": contract.compiler_version,
            "optimization": contract.optimization_used,
            "runs": contract.runs,
            "evmVersion": contract.evm_version
        },
        "constructorArguments": contract.constructor_arguments,
        "library": contract.library,
        "licenseType": contract.license_type,
        "proxy": contract.proxy,
        "implementation": contract.implementation,
        "swarmSource": contract.swarm_source
    }))
}
