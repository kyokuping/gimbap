use gimbap::request::parser::{HttpMethod, HttpVersion, parse_connection, parse_headers};
use url::Host;

#[test]
fn test_request_parser() {
    let request = "GET / HTTP/1.1\r\nHost: example.com\r\n\r\n";
    let result = parse_connection(&mut request.as_bytes());
    let request = result.unwrap();
    assert_eq!(request.method, HttpMethod::GET);
    assert_eq!(request.path, "/");
    assert_eq!(request.version, HttpVersion::V1_1);
}

#[test]
fn test_parse_headers() {
    let headers = "Host: example.com\r\nUser-Agent: Mozilla/5.0\r\nContent-Type: text/plain\r\nContent-Length: 12\r\n\r\n";
    let result = parse_headers(&mut headers.as_bytes());
    let (headers, headers_metadata) = result.unwrap(); //Err: UninitializedField("content_type")
    assert_eq!(headers.get("host").unwrap(), &["example.com"]);
    assert_eq!(headers.get("user-agent").unwrap(), &["Mozilla/5.0"]);
    assert_eq!(
        headers_metadata
            .body_metadata
            .as_ref()
            .unwrap()
            .content_type,
        mime::TEXT_PLAIN
    );
    assert_eq!(
        headers_metadata
            .body_metadata
            .as_ref()
            .unwrap()
            .content_length,
        12u64
    );
    assert_eq!(headers_metadata.host, Host::parse("example.com").unwrap());
}
