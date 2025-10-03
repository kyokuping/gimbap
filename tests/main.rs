use gimbap::request::parser::{HttpMethod, HttpVersion, parse_connection, parse_headers};

#[test]
fn test_request_parser() {
    let request = "GET / HTTP/1.1\r\n\r\n";
    let result = parse_connection(request.as_bytes());
    let request = result.unwrap();
    assert_eq!(request.method, HttpMethod::GET);
    assert_eq!(request.path, "/");
    assert_eq!(request.version, HttpVersion::V1_1);
}

#[test]
fn test_parse_headers() {
    let headers = "Host: example.com\r\nUser-Agent: Mozilla/5.0\r\n\r\n";
    let result = parse_headers(headers.as_bytes());
    let headers = result.unwrap();
    println!("headers: {:?}", headers);
    assert_eq!(headers.get("host").unwrap(), &["example.com"]);
    assert_eq!(headers.get("user-agent").unwrap(), &["Mozilla/5.0"]);
}
