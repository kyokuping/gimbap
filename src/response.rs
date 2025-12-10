use crate::common::StatusCode;
use serde::Serialize;
use std::collections::HashMap;

pub struct Response {
    status_code: StatusCode,
    headers: HashMap<String, Vec<String>>,
    body: Option<Vec<u8>>,
}

impl Default for Response {
    fn default() -> Self {
        Self::new()
    }
}

impl Response {
    pub fn new() -> Self {
        Response {
            status_code: StatusCode::from_u16(200).unwrap(),
            headers: HashMap::new(),
            body: None,
        }
    }
    pub fn body(&self) -> Option<&[u8]> {
        self.body.as_deref()
    }

    pub fn status_code(&self) -> u16 {
        self.status_code.as_u16()
    }

    pub fn status(&mut self, code: u16) -> &mut Self {
        match StatusCode::from_u16(code) {
            Ok(status) => {
                self.status_code = status;
            }
            Err(_) => {
                self.status_code = StatusCode::from_u16(500).unwrap();
                self.send(b"Internal Server Error");
            }
        }
        self
    }
    pub fn send(&mut self, body: &[u8]) -> &mut Self {
        self.body = Some(body.to_vec());
        self
    }
    pub fn json<T: Serialize>(&mut self, data: &T) -> &mut Self {
        match serde_json::to_vec(data) {
            Ok(bytes) => {
                self.header("content-type", "application/json");
                self.send(&bytes)
            }
            Err(_) => {
                self.status(500);
                self.send(b"Internal Server Error")
            }
        }
    }
    pub fn header(&mut self, key: &str, value: &str) -> &mut Self {
        self.headers
            .entry(key.to_string())
            .or_default()
            .push(value.to_string());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_response() {
        let response = Response::default();
        assert_eq!(response.status_code(), 200);
        assert!(response.body.is_none());
        assert!(response.headers.is_empty());
    }

    #[test]
    fn test_api_chaining() {
        let mut response = Response::new();
        response
            .status(404)
            .header("X-Test", "True")
            .send(b"Not Found");

        assert_eq!(response.status_code(), 404);
        assert_eq!(response.body(), Some("Not Found".as_bytes()));
        assert_eq!(response.headers.get("X-Test").unwrap()[0], "True");
    }

    #[test]
    fn test_json_serialization() {
        #[derive(Serialize)]
        struct Message {
            msg: String,
        }

        let data = Message {
            msg: "Hello".to_string(),
        };

        let mut response = Response::new();
        response.json(&data);

        assert_eq!(
            response.headers.get("content-type").unwrap()[0],
            "application/json"
        );

        let body = response.body().unwrap();
        let body_str = std::str::from_utf8(body).unwrap();
        assert_eq!(body_str, r#"{"msg":"Hello"}"#);
    }
}
