use crate::common::HttpMethod;
use parser::Request as RawRequest;
use percent_encoding::percent_decode_str;
use std::collections::HashMap;

pub mod parser;

pub struct Request {
    #[allow(dead_code)]
    raw_request: RawRequest,
    pub method: HttpMethod,
    pub segments: Vec<String>,
    query: Query,
    pub params: Params,
    body: Option<Vec<u8>>,
}

impl Request {
    pub fn new(raw_request: RawRequest) -> Self {
        let segments: Vec<String> = raw_request
            .url
            .path()
            .split('/')
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect();
        let query = Query::new(&raw_request);
        let params = Params::default();

        Self {
            method: raw_request.method.clone(),
            raw_request,
            segments,
            query,
            params,
            body: None,
        }
    }

    pub fn path(&self) -> &str {
        &self.raw_request.url.path()
    }

    pub fn query(&self) -> &Query {
        &self.query
    }

    pub fn params(&self) -> &Params {
        &self.params
    }

    pub fn body(&self) -> Option<&[u8]> {
        self.body.as_deref()
    }
}

pub struct Query(HashMap<String, Vec<String>>);

impl Query {
    fn new(raw_request: &RawRequest) -> Self {
        let mut query_map: HashMap<String, Vec<String>> = HashMap::new();
        if let Some(query) = raw_request.url.query() {
            for parameter in query.split('&') {
                let (key, value) = parameter.split_once('=').unwrap_or((parameter, ""));
                let key_decoded = percent_decode_str(key).decode_utf8_lossy();
                let value_decoded = percent_decode_str(value).decode_utf8_lossy();
                query_map
                    .entry(key_decoded.to_string())
                    .or_default()
                    .push(value_decoded.to_string());
            }
        }
        Self(query_map)
    }

    pub fn get(&self, key: &str) -> Option<&Vec<String>> {
        self.0.get(key)
    }
}

pub struct Params(HashMap<String, String>);

impl Params {
    pub fn default() -> Self {
        Self(HashMap::new())
    }

    pub fn new(route_segments: &[String], req_segments: &[String]) -> Self {
        let mut params_map = HashMap::new();
        for (route_seg, req_seg) in route_segments.iter().zip(req_segments) {
            if let Some(key) = route_seg.strip_prefix(':') {
                params_map.insert(key.to_string(), req_seg.clone());
            }
        }
        Self(params_map)
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).map(|value| value.as_str())
    }

    pub fn insert(&mut self, key: &str, value: &str) {
        self.0.insert(key.to_string(), value.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::HttpMethod;
    use crate::request::parser::Request as RawRequest;
    use url::Url;

    fn mock_raw_request(url_str: &str) -> RawRequest {
        RawRequest {
            method: HttpMethod::GET,
            url: Url::parse(url_str).unwrap(),
            version: crate::request::parser::HttpVersion::V1_1,
            headers: Default::default(),
            header_metadata: crate::request::parser::HeaderMetadata {
                host: url::Host::parse("localhost").unwrap(),
                body_metadata: None,
                is_secure: false,
            },
            body: None,
        }
    }

    #[test]
    fn test_query_parsing() {
        let raw = mock_raw_request(
            "http://localhost/path?name=gimbap&tag=rust&tag=web&empty=&msg=hello%20world",
        );
        let query = Query::new(&raw);

        assert_eq!(query.get("name").unwrap(), &vec!["gimbap".to_string()]);

        let tags = query.get("tag").unwrap();
        assert_eq!(tags.len(), 2);
        assert!(tags.contains(&"rust".to_string()));
        assert!(tags.contains(&"web".to_string()));

        assert_eq!(query.get("msg").unwrap(), &vec!["hello world".to_string()]);

        assert_eq!(query.get("empty").unwrap(), &vec!["".to_string()]);

        assert!(query.get("invalid").is_none());
    }

    #[test]
    fn test_request_segments_parsing() {
        let raw = mock_raw_request("http://localhost//users///123/?q=ignore");
        let req = Request::new(raw);

        assert_eq!(req.segments, vec!["users", "123"]);
    }

    #[test]
    fn test_params_extraction() {
        let route_segs = vec![
            "users".to_string(),
            ":id".to_string(),
            "posts".to_string(),
            ":post_id".to_string(),
        ];
        let req_segs = vec![
            "users".to_string(),
            "42".to_string(),
            "posts".to_string(),
            "999".to_string(),
        ];

        let params = Params::new(&route_segs, &req_segs);

        assert_eq!(params.get("id"), Some("42"));
        assert_eq!(params.get("post_id"), Some("999"));
        assert!(params.get("users").is_none());
    }
}
