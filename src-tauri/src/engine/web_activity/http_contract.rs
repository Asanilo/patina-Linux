use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WebActivityBridgeHttpRequest {
    pub method: String,
    pub path: String,
    pub authorization: Option<String>,
    pub body: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WebActivityBridgeHttpResponse {
    pub status: u16,
    pub body: String,
}

impl WebActivityBridgeHttpResponse {
    pub fn json(status: u16, data: Value) -> Self {
        Self {
            status,
            body: data.to_string(),
        }
    }
}
