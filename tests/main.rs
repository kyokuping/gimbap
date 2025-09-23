use gimbap::request::parser::{HttpMethod, HttpVersion, parse_connection};

#[test]
fn test_request_parser() {
    let request = "GET / HTTP/1.1\r\n\r\n";
    let result = parse_connection(request.as_bytes());
    let request = result.unwrap();
    assert_eq!(request.method, HttpMethod::GET);
    assert_eq!(request.path, "/");
    assert_eq!(request.version, HttpVersion::V1_1);
}
