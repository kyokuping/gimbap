use flate2::Compression;
use flate2::read::GzEncoder;
use gimbap::request::parser::{
    Body, BodyData, ContentEncoding, ContentLength, FormDataValue, HttpMethod, HttpVersion,
    parse_connection, parse_headers,
};
use std::collections::HashMap;
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
        content_type: mime::TEXT_PLAIN,
        boundary: None,
    };

    let mut buf = Vec::new();
    empty_body
        .into_reader()
        .unwrap()
        .read_to_end(&mut buf)
        .unwrap();

    assert_eq!(buf, [] as [u8; 0]);
}

#[test]
fn test_into_reader_none_encoded() {
    let hello = "Hello, World!";
    let reader = hello.as_bytes();
    let none_encoded_body = Body {
        reader: Box::new(reader),
        size_hint: Some(hello.len() as u64),
        encoding: None,
        content_type: mime::TEXT_PLAIN,
        boundary: None,
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
        content_type: mime::TEXT_PLAIN,
        boundary: None,
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
        content_type: mime::TEXT_PLAIN,
        boundary: None,
    };

    let mut buf = Vec::new();
    multiple_encoded_body
        .into_reader()
        .unwrap()
        .read_to_end(&mut buf)
        .unwrap();
    assert_eq!(buf, reader);
}

#[test]
fn test_into_body_data_text_plain() {
    let text = "Hello, World!";
    let body = Body {
        reader: Box::new(text.as_bytes()),
        size_hint: Some(text.len() as u64),
        encoding: None,
        content_type: mime::TEXT_PLAIN,
        boundary: None,
    };

    let body_data = body.into_body_data().unwrap();
    match body_data {
        BodyData::Text(data) => assert_eq!(data, text),
        _ => panic!("Expected BodyData::Text"),
    }
}

#[test]
fn test_into_body_data_x_www_form_urlencoded() {
    let form_data = "key1=value1&key2=value2";
    let body = Body {
        reader: Box::new(form_data.as_bytes()),
        size_hint: Some(form_data.len() as u64),
        encoding: None,
        content_type: mime::APPLICATION_WWW_FORM_URLENCODED,
        boundary: None,
    };

    let body_data = body.into_body_data().unwrap();
    match body_data {
        BodyData::FormData(data) => {
            let mut expected = HashMap::new();
            expected.insert(
                "key1".to_string(),
                FormDataValue::Text("value1".to_string()),
            );
            expected.insert(
                "key2".to_string(),
                FormDataValue::Text("value2".to_string()),
            );
            assert_eq!(
                data.len(),
                expected.len(),
                "HashMaps have different lengths"
            );
            for (key, value) in &expected {
                assert!(data.contains_key(key));
                let val1 = match data.get(key) {
                    Some(FormDataValue::Text(v)) => v,
                    _ => panic!(""),
                };
                let val2 = match value {
                    FormDataValue::Text(v) => v,
                    _ => panic!(""),
                };
                assert_eq!(val1, val2);
            }
        }
        _ => panic!("Expected BodyData::FormData"),
    }
}

#[test]
fn test_into_body_data_multipart_form_data() {
    let boundary = "TestBoundary123";
    let multipart_data = concat!(
        "--TestBoundary123\r\n",
        "Content-Disposition: form-data; name=\"key1\"\r\n",
        "\r\n",
        "value1\r\n",
        "--TestBoundary123\r\n",
        "Content-Disposition: form-data; name=\"key2\"; filename=\"file.txt\"\r\n",
        "Content-Type: text/plain\r\n",
        "\r\n",
        "file content\r\n",
        "--TestBoundary123--\r\n",
    )
    .as_bytes();

    let multipart_data_len = multipart_data.len();
    let body = Body {
        reader: Box::new(std::io::Cursor::new(multipart_data)),
        size_hint: Some(multipart_data_len as u64),
        encoding: None,
        content_type: mime::MULTIPART_FORM_DATA,
        boundary: Some(boundary.to_string()),
    };

    let body_data = body.into_body_data().unwrap();
    match body_data {
        BodyData::FormData(data) => {
            assert_eq!(data.len(), 2);
            match data.get("key1") {
                Some(FormDataValue::Text(text)) => assert_eq!(text, "value1"),
                _ => panic!("Expected text value for key1"),
            }
            match data.get("key2") {
                Some(FormDataValue::File {
                    filename,
                    content_type,
                    data,
                }) => {
                    assert_eq!(filename, "file.txt");
                    assert_eq!(content_type, "text/plain");
                    assert_eq!(data, b"file content");
                }
                _ => panic!("Expected file value for key2"),
            }
        }
        _ => panic!("Expected BodyData::FormData"),
    }
}
