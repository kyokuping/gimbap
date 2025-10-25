use flate2::Compression;
use flate2::read::GzEncoder;
use gimbap::request::parser::{
    Body, ContentEncoding, ContentLength, HttpMethod, HttpVersion, parse_connection, parse_headers,
};
use std::io::{self, Read};
use url::Host;

#[test]
fn test_request_parser() {
    let request = "GET / HTTP/1.1\r\nHost: example.com\r\n\r\n";
    let result = parse_connection(request.as_bytes());
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
        ContentLength::Fixed(12)
    );
    assert_eq!(headers_metadata.host, Host::parse("example.com").unwrap());
}
#[test]
fn test_transfer_encoding_chunked() {
    let headers =
        "Host: example.com\r\nContent-Type: text/plain\r\nTransfer-Encoding: chunked\r\n\r\n";
    let result = parse_headers(&mut headers.as_bytes());
    let (headers, headers_metadata) = result.unwrap();
    assert_eq!(headers.get("transfer-encoding").unwrap(), &["chunked"]);
    assert_eq!(
        headers_metadata
            .body_metadata
            .as_ref()
            .unwrap()
            .content_length,
        ContentLength::Chunked
    );
}

#[test]
fn test_transfer_encoding_identity() {
    let headers =
        "Host: example.com\r\nContent-Type: text/plain\r\nTransfer-Encoding: identity\r\n\r\n";
    let result = parse_headers(&mut headers.as_bytes());
    assert!(result.is_err());
    let error = result.unwrap_err();
    let expected_error_message = "transfer-encoding `identity` is not supported yet";
    assert_eq!(error.to_string(), expected_error_message);
}

#[test]
fn test_into_reader_empty() {
    let empty_reader = io::empty();
    let empty_body = Body {
        reader: Box::new(empty_reader),
        size_hint: Some(0),
        encoding: None,
    };

    let mut buf = Vec::new();
    empty_body
        .into_reader()
        .unwrap()
        .read_to_end(&mut buf)
        .unwrap();
    assert_eq!(buf, []);
}

#[test]
fn test_into_reader_none_encoded() {
    let hello = "Hello, World!";
    let reader = hello.as_bytes();
    let none_encoded_body = Body {
        reader: Box::new(reader),
        size_hint: Some(hello.len() as u64),
        encoding: None,
    };

    let mut buf = Vec::new();
    none_encoded_body
        .into_reader()
        .unwrap()
        .read_to_end(&mut buf)
        .unwrap();
    assert_eq!(buf, reader);
}

#[test]
fn test_into_reader_single_encoded() {
    let hello = "Hello, World!";
    let reader = hello.as_bytes();
    let gzip_encoded_reader = GzEncoder::new(reader, Compression::default());
    let single_encoded_body = Body {
        reader: Box::new(gzip_encoded_reader),
        size_hint: Some(hello.len() as u64),
        encoding: Some(vec![ContentEncoding::Gzip]),
    };

    let mut buf = Vec::new();
    single_encoded_body
        .into_reader()
        .unwrap()
        .read_to_end(&mut buf)
        .unwrap();
    assert_eq!(buf, reader);
}

#[test]
fn test_into_reader_multiple_encoded() {
    let hello = "Hello, World!";
    let reader = hello.as_bytes();
    let gzip_encoded_reader = GzEncoder::new(reader, Compression::default());
    let brotli_encoded_reader = brotli::CompressorReader::new(gzip_encoded_reader, 4098, 0, 22);
    let multiple_encoded_body = Body {
        reader: Box::new(brotli_encoded_reader),
        size_hint: Some(hello.len() as u64),
        encoding: Some(vec![ContentEncoding::Gzip, ContentEncoding::Br]),
    };

    let mut buf = Vec::new();
    multiple_encoded_body
        .into_reader()
        .unwrap()
        .read_to_end(&mut buf)
        .unwrap();
    assert_eq!(buf, reader);
}
