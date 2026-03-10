use std::str::FromStr;

use crate::common::{HeaderName, HeaderValue, Headers, InvalidStatusCode, StatusCode};
use serde::Serialize;

pub struct Response {
    status_code: StatusCode,
    headers: Headers,
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
            headers: Headers::default(),
            body: None,
        }
    }
    pub fn body(&self) -> Option<&[u8]> {
        self.body.as_deref()
    }

    pub fn status_code(&self) -> u16 {
        self.status_code.as_u16()
    }

    pub fn status(&mut self, code: u16) -> Result<&mut Self, InvalidStatusCode> {
        let status = StatusCode::from_u16(code)?;
        self.status_code = status;
        Ok(self)
    }
    pub fn send(&mut self, body: &[u8]) -> &mut Self {
        self.body = Some(body.to_vec());
        self
    }
    pub fn json<T: Serialize>(&mut self, data: &T) -> Result<&mut Self, serde_json::Error> {
        let bytes = serde_json::to_vec(data)?;
        self.header("content-type", "application/json");
        self.send(&bytes);
        Ok(self)
    }

    fn sanitize<T>(input: &str) -> Result<T, T::Err>
    where
        T: FromStr + Default,
    {
        let replaced = input.replace(['\r', '\n'], "");
        <T as FromStr>::from_str(&replaced)
    }

    pub fn header(&mut self, key: &str, value: &str) -> &mut Self {
        if let (Ok(key), Ok(values)) = (
            Self::sanitize::<HeaderName>(key),
            Self::sanitize::<HeaderValue>(value),
        ) {
            self.headers.append(key, values);
        }
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
        assert_eq!(response.headers, Headers::default());
    }

    #[test]
    fn test_api_chaining() {
        let mut response = Response::new();
        response
            .status(404)
            .unwrap()
            .header("X-Test", "True")
            .send(b"Not Found");

        assert_eq!(response.status_code(), 404);
        assert_eq!(response.body(), Some("Not Found".as_bytes()));
        assert_eq!(response.headers.get_str("X-Test").unwrap()[0], "True");
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
        let result = response.json(&data);
        assert!(result.is_ok());

        assert_eq!(
            response.headers.get_str("content-type").unwrap()[0],
            "application/json"
        );

        let body = response.body().unwrap();
        let body_str = std::str::from_utf8(body).unwrap();
        assert_eq!(body_str, r#"{"msg":"Hello"}"#);
    }
}
