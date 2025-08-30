//! Utility functions for the SEI MCP server

use serde::de::DeserializeOwned;
use serde_json::{Value, from_value};
use crate::mcp::protocol::{Response, error_codes};

/// Helper function to extract a required argument from a JSON object
pub fn get_required_arg<T: DeserializeOwned>(
    args: &Value,
    key: &str,
    req_id: &Value,
) -> Result<T, Response> {
    from_value(args.get(key).cloned().unwrap_or(Value::Null)).map_err(|_| {
        Response::error(
            req_id.clone(),
            error_codes::INVALID_PARAMS,
            format!("Missing or invalid required argument: '{}'", key),
        )
    })
}

/// Helper function to convert any value to a string
pub fn to_string<T: std::fmt::Display>(value: T) -> String {
    value.to_string()
}
