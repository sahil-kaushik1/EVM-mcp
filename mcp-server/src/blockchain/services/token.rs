// src/blockchain/services/token.rs

use anyhow::{anyhow, Result};
use ethers_core::abi::{decode, encode, Abi, Function, ParamType, Token};
use ethers_core::types::{Address, Bytes, TransactionRequest, U256};
use ethers_core::utils::keccak256;
use reqwest::Client;
use serde_json::{json, Value};
use std::str::FromStr;

fn selector(sig: &str) -> [u8; 4] {
    let mut sel = [0u8; 4];
    sel.copy_from_slice(&keccak256(sig.as_bytes())[0..4]);
    sel
}

fn hex_to_bytes(v: &Value) -> Result<Vec<u8>> {
    let s = v.as_str().ok_or_else(|| anyhow!("eth_call result not string"))?;
    let s = s.strip_prefix("0x").unwrap_or(s);
    Ok(hex::decode(s)?)
}

fn decode_string(v: &Value) -> Option<String> {
    // Try standard ABI string
    if let Ok(bytes) = hex_to_bytes(v) {
        if let Ok(tokens) = decode(&[ParamType::String], &bytes) {
            if let Some(Token::String(s)) = tokens.get(0) { return Some(s.clone()); }
        }
        // Fallback: bytes32 to string (strip zeros)
        if let Ok(tokens) = decode(&[ParamType::FixedBytes(32)], &bytes) {
            if let Some(Token::FixedBytes(b)) = tokens.get(0) {
                let s = String::from_utf8(b.clone().into_iter().take_while(|c| *c != 0u8).collect()).ok();
                if s.is_some() { return s; }
            }
        }
    }
    None
}

fn decode_u256(v: &Value) -> Option<U256> {
    if let Ok(bytes) = hex_to_bytes(v) {
        if let Ok(tokens) = decode(&[ParamType::Uint(256)], &bytes) {
            if let Some(Token::Uint(n)) = tokens.get(0) { return Some(*n); }
        }
    }
    None
}

fn encode_call(sig: &str, tokens: Vec<Token>) -> Bytes {
    let mut out = selector(sig).to_vec();
    let mut tail = encode(&tokens);
    out.append(&mut tail);
    Bytes::from(out)
}

async fn eth_call(client: &Client, rpc_url: &str, to: &str, data: Bytes) -> Result<Value> {
    let payload = json!({
        "jsonrpc": "2.0",
        "method": "eth_call",
        "params": [{"to": to, "data": format!("0x{}", hex::encode(data))}, "latest"],
        "id": 1
    });
    let resp = client.post(rpc_url).json(&payload).send().await?;
    let v: Value = resp.json().await?;
    if let Some(err) = v.get("error") { return Err(anyhow!("eth_call error: {}", err)); }
    Ok(v["result"].clone())
}

pub async fn erc20_info(client: &Client, rpc_url: &str, token: &str) -> Result<Value> {
    let name_raw = eth_call(client, rpc_url, token, encode_call("name()", vec![])).await.unwrap_or(json!(null));
    let symbol_raw = eth_call(client, rpc_url, token, encode_call("symbol()", vec![])).await.unwrap_or(json!(null));
    let decimals_raw = eth_call(client, rpc_url, token, encode_call("decimals()", vec![])).await.unwrap_or(json!(null));
    let total_raw = eth_call(client, rpc_url, token, encode_call("totalSupply()", vec![])).await.unwrap_or(json!(null));
    let name = decode_string(&name_raw);
    let symbol = decode_string(&symbol_raw);
    let decimals = decode_u256(&decimals_raw).map(|n| n.to_string());
    let total_supply = decode_u256(&total_raw).map(|n| n.to_string());
    Ok(json!({
        "raw": {"name": name_raw, "symbol": symbol_raw, "decimals": decimals_raw, "totalSupply": total_raw},
        "decoded": {"name": name, "symbol": symbol, "decimals": decimals, "totalSupply": total_supply}
    }))
}

pub async fn erc20_balance_of(client: &Client, rpc_url: &str, token: &str, owner: &str) -> Result<Value> {
    let owner_addr = Address::from_str(owner)?;
    let data = encode_call("balanceOf(address)", vec![Token::Address(owner_addr)]);
    let raw = eth_call(client, rpc_url, token, data).await?;
    let decoded = decode_u256(&raw).map(|n| n.to_string());
    Ok(json!({"raw": raw, "decoded": decoded}))
}

pub fn erc20_transfer_tx(token: &str, to: &str, amount_wei: &str) -> Result<TransactionRequest> {
    let to_addr = Address::from_str(to)?;
    let amount = U256::from_dec_str(amount_wei)?;
    let data = encode_call("transfer(address,uint256)", vec![Token::Address(to_addr), Token::Uint(amount)]);
    let contract = Address::from_str(token)?;
    Ok(TransactionRequest::new().to(contract).data(data))
}

pub fn erc20_approve_tx(token: &str, spender: &str, amount_wei: &str) -> Result<TransactionRequest> {
    let spender_addr = Address::from_str(spender)?;
    let amount = U256::from_dec_str(amount_wei)?;
    let data = encode_call("approve(address,uint256)", vec![Token::Address(spender_addr), Token::Uint(amount)]);
    let contract = Address::from_str(token)?;
    Ok(TransactionRequest::new().to(contract).data(data))
}

pub async fn is_contract(client: &Client, rpc_url: &str, address: &str) -> Result<bool> {
    let payload = json!({"jsonrpc": "2.0", "method": "eth_getCode", "params": [address, "latest"], "id": 1});
    let v: Value = client.post(rpc_url).json(&payload).send().await?.json().await?;
    if let Some(err) = v.get("error") { return Err(anyhow!("eth_getCode error: {}", err)); }
    let code = v["result"].as_str().unwrap_or("0x");
    Ok(code != "0x" && code != "0x0")
}

pub async fn erc721_token_uri(client: &Client, rpc_url: &str, token: &str, token_id: &str) -> Result<Value> {
    let id = U256::from_dec_str(token_id)?;
    let data = encode_call("tokenURI(uint256)", vec![Token::Uint(id)]);
    let raw = eth_call(client, rpc_url, token, data).await?;
    let decoded = decode_string(&raw);
    Ok(json!({"raw": raw, "decoded": decoded}))
}

pub async fn erc721_owner_of(client: &Client, rpc_url: &str, token: &str, token_id: &str) -> Result<Value> {
    let id = U256::from_dec_str(token_id)?;
    let data = encode_call("ownerOf(uint256)", vec![Token::Uint(id)]);
    let raw = eth_call(client, rpc_url, token, data).await?;
    Ok(json!({"raw": raw}))
}

pub async fn erc721_balance_of(client: &Client, rpc_url: &str, token: &str, owner: &str) -> Result<Value> {
    let owner_addr = Address::from_str(owner)?;
    let data = encode_call("balanceOf(address)", vec![Token::Address(owner_addr)]);
    let raw = eth_call(client, rpc_url, token, data).await?;
    let decoded = decode_u256(&raw).map(|n| n.to_string());
    Ok(json!({"raw": raw, "decoded": decoded}))
}

pub fn erc1155_safe_transfer_from_tx(token: &str, from: &str, to: &str, token_id: &str, amount: &str) -> Result<TransactionRequest> {
    let from_addr = Address::from_str(from)?;
    let to_addr = Address::from_str(to)?;
    let id = U256::from_dec_str(token_id)?;
    let amt = U256::from_dec_str(amount)?;
    let empty = Bytes::from(Vec::<u8>::new());
    let data = encode_call(
        "safeTransferFrom(address,address,uint256,uint256,bytes)",
        vec![Token::Address(from_addr), Token::Address(to_addr), Token::Uint(id), Token::Uint(amt), Token::Bytes(empty.to_vec())]
    );
    let contract = Address::from_str(token)?;
    Ok(TransactionRequest::new().to(contract).data(data))
}

pub async fn erc1155_uri(client: &Client, rpc_url: &str, token: &str, token_id: &str) -> Result<Value> {
    let id = U256::from_dec_str(token_id)?;
    let data = encode_call("uri(uint256)", vec![Token::Uint(id)]);
    let raw = eth_call(client, rpc_url, token, data).await?;
    let decoded = decode_string(&raw);
    Ok(json!({"raw": raw, "decoded": decoded}))
}

pub async fn erc1155_balance_of(client: &Client, rpc_url: &str, token: &str, owner: &str, token_id: &str) -> Result<Value> {
    let owner_addr = Address::from_str(owner)?;
    let id = U256::from_dec_str(token_id)?;
    let data = encode_call("balanceOf(address,uint256)", vec![Token::Address(owner_addr), Token::Uint(id)]);
    let raw = eth_call(client, rpc_url, token, data).await?;
    let decoded = decode_u256(&raw).map(|n| n.to_string());
    Ok(json!({"raw": raw, "decoded": decoded}))
}

pub async fn erc20_allowance(client: &Client, rpc_url: &str, token: &str, owner: &str, spender: &str) -> Result<Value> {
    let owner_addr = Address::from_str(owner)?;
    let spender_addr = Address::from_str(spender)?;
    let data = encode_call(
        "allowance(address,address)",
        vec![Token::Address(owner_addr), Token::Address(spender_addr)]
    );
    let raw = eth_call(client, rpc_url, token, data).await?;
    let decoded = decode_u256(&raw).map(|n| n.to_string());
    Ok(json!({"raw": raw, "decoded": decoded}))
}

pub async fn read_contract_via_abi(client: &Client, rpc_url: &str, contract: &str, abi_json: &str, function_name: &str, args: Option<Vec<Value>>) -> Result<Value> {
    let abi: Abi = serde_json::from_str(abi_json)?;
    let func: &Function = abi.functions().find(|f| f.name == function_name).ok_or_else(|| anyhow!("function not found in ABI"))?;
    let tokens = coerce_tokens(func, args.unwrap_or_default())?;
    let sig = function_signature(func);
    let data = encode_call(&sig, tokens);
    eth_call(client, rpc_url, contract, data).await
}

pub fn write_contract_tx(contract: &str, abi_json: &str, function_name: &str, args: Option<Vec<Value>>) -> Result<TransactionRequest> {
    let abi: Abi = serde_json::from_str(abi_json)?;
    let func: &Function = abi.functions().find(|f| f.name == function_name).ok_or_else(|| anyhow!("function not found in ABI"))?;
    let tokens = coerce_tokens(func, args.unwrap_or_default())?;
    let sig = function_signature(func);
    let data = encode_call(&sig, tokens);
    let contract_addr = Address::from_str(contract)?;
    Ok(TransactionRequest::new().to(contract_addr).data(data))
}

fn coerce_tokens(func: &Function, args: Vec<Value>) -> Result<Vec<Token>> {
    if func.inputs.len() != args.len() {
        return Err(anyhow!("arg count mismatch: expected {}, got {}", func.inputs.len(), args.len()));
    }
    let mut out = Vec::new();
    for (i, param) in func.inputs.iter().enumerate() {
        let ty = &param.kind;
        let val = &args[i];
        let tok = match ty {
            ethers_core::abi::ParamType::Address => Token::Address(Address::from_str(val.as_str().ok_or_else(|| anyhow!("address arg must be string"))?)?),
            ethers_core::abi::ParamType::Uint(_) => Token::Uint(U256::from_dec_str(val.as_str().ok_or_else(|| anyhow!("uint arg must be decimal string"))?)?),
            ethers_core::abi::ParamType::Bool => Token::Bool(val.as_bool().ok_or_else(|| anyhow!("bool arg must be boolean"))?),
            ethers_core::abi::ParamType::String => Token::String(val.as_str().unwrap_or("").to_string()),
            ethers_core::abi::ParamType::Bytes => {
                let s = val.as_str().unwrap_or("");
                let bytes = if s.starts_with("0x") { hex::decode(&s[2..])? } else { s.as_bytes().to_vec() };
                Token::Bytes(bytes)
            }
            // Fallback: unsupported types for now
            other => return Err(anyhow!("unsupported ABI param type: {:?}", other)),
        };
        out.push(tok);
    }
    Ok(out)
}

fn function_signature(func: &Function) -> String {
    let types: Vec<String> = func
        .inputs
        .iter()
        .map(|p| param_type_to_string(&p.kind))
        .collect();
    format!("{}({})", func.name, types.join(","))
}

fn param_type_to_string(p: &ParamType) -> String {
    match p {
        ParamType::Address => "address".to_string(),
        ParamType::Bytes => "bytes".to_string(),
        ParamType::FixedBytes(n) => format!("bytes{}", n),
        ParamType::Int(n) => format!("int{}", n),
        ParamType::Uint(n) => format!("uint{}", n),
        ParamType::Bool => "bool".to_string(),
        ParamType::String => "string".to_string(),
        ParamType::Array(inner) => format!("{}[]", param_type_to_string(inner)),
        ParamType::FixedArray(inner, n) => format!("{}[{}]", param_type_to_string(inner), n),
        ParamType::Tuple(components) => {
            let inner: Vec<String> = components.iter().map(param_type_to_string).collect();
            format!("({})", inner.join(","))
        }
    }
}
