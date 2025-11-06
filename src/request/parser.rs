use brotli::Decompressor as BrotliDecompressor;
use derive_builder::Builder;
use encoding_rs::UTF_8;
use flate2::read::{DeflateDecoder, GzDecoder};
use mime::Mime;
use once_cell::unsync::Lazy;
use std::collections::HashMap;
use std::io::{BufRead, Read};
use std::str::FromStr;
use std::sync::LazyLock;
use url::Host;
use zstd::stream::Decoder as ZstdDecoder;

const MAX_BODY_SIZE: u64 = 10 * 1024 * 1024; // 10MB
const BROTLI_BUFFER_SIZE: usize = 4096;

static URL_REGEX: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"^([A-Z]+?) ([^ ]+?) (HTTP/[0-9.]+?)\s*$").unwrap());
static HEADER_NAME_REGEX: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"^[a-zA-Z0-9!#\$%&'*+-.^_`|~]+$").unwrap());
static HEADER_VALUE_REGEX: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new("^[\\t\\u0020-\\u007E\\u0080-\\u00FF]*$").unwrap());
static AUTHORIZATION_REGEX: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"^\w+ .+$").unwrap());

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum HttpMethod {
    GET,
    HEAD,
    POST,
    PUT,
    DELETE,
    CONNECT,
    TRACE,
    PATCH,
    Other(String),
}

impl HttpMethod {
    fn from_str(method: &str) -> Self {
        match method {
            "GET" => HttpMethod::GET,
            "HEAD" => HttpMethod::HEAD,
            "POST" => HttpMethod::POST,
            "PUT" => HttpMethod::PUT,
            "DELETE" => HttpMethod::DELETE,
            "CONNECT" => HttpMethod::CONNECT,
            "TRACE" => HttpMethod::TRACE,
            "PATCH" => HttpMethod::PATCH,
            _ => HttpMethod::Other(method.to_string()),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum HttpVersion {
    V1_1,
}

impl HttpVersion {
    fn from_str(version: &str) -> Self {
        match version {
            "HTTP/1.1" => HttpVersion::V1_1,
            _ => panic!("Unsupported HTTP version"),
        }
    }
}

pub struct Request {
    pub method: HttpMethod,
    pub path: String,
    pub version: HttpVersion,
    pub headers: HashMap<String, Vec<String>>,
    pub header_metadata: HeaderMetadata,
    pub body: Option<Body>,
}
impl Request {
    pub fn parse_connection<T: BufRead + 'static>(
        mut reader: T,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let mut line = String::new();
        let len = reader.read_line(&mut line)?;
        if len == 0 {
            return Err("Unexpected end of stream".into());
        }
        let (method, path, version) = Self::parse_start_line(&line)?;
        let (headers, header_metadata) = Self::parse_headers(&mut reader)?;
        let body = header_metadata
            .body_metadata
            .as_ref()
            .map(|metadata| Body::new(reader, metadata.clone()));
        Ok(Request {
            method,
            path,
            version,
            headers,
            header_metadata,
            body,
        })
    }

    fn parse_start_line(line: &str) -> Result<(HttpMethod, String, HttpVersion), String> {
        let captures = URL_REGEX.captures(line).ok_or("Invalid request line")?;
        let method = HttpMethod::from_str(&captures[1]);
        let url = captures[2].to_string();
        let version = HttpVersion::from_str(&captures[3]);
        Ok((method, url, version))
    }

    fn parse_headers<T: BufRead>(
        reader: &mut T,
    ) -> Result<ParsedHeader, Box<dyn std::error::Error>> {
        let mut headers = HashMap::new();
        let mut header_metadata_builder = HeaderMetadataBuilder::create_empty();
        let mut body_metadata_builder = Lazy::new(BodyMetadataBuilder::create_empty);

        for line in reader.lines() {
            let line = line?;
            if line.is_empty() {
                break;
            }

            if let Some((key_str, values_str)) = line.split_once(":") {
                let key = key_str.trim().to_lowercase();
                if !HEADER_NAME_REGEX.is_match(&key) {
                    continue;
                }

                let values: Vec<String> = values_str
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .collect();
                if values
                    .iter()
                    .any(|value| !HEADER_VALUE_REGEX.is_match(value))
                {
                    continue;
                }

                if values
                    .iter()
                    .any(|value| !Self::is_special_header_valid(&key, value))
                {
                    continue;
                }

                match key.as_str() {
                    "host" => match &*values {
                        [host] => {
                            header_metadata_builder.host(Host::parse(host)?);
                        }
                        [] => (),
                        _ => return Err("unexpected host header value".into()),
                    },
                    "content-type" => match &*values {
                        [content_type] => {
                            body_metadata_builder.content_type(Mime::from_str(content_type)?);
                        }
                        [] => (),
                        _ => return Err("unexpected content-type header value".into()),
                    },
                    "content-length" => match &*values {
                        [length] => {
                            body_metadata_builder
                                .content_length(ContentLength::Fixed(length.parse()?));
                        }
                        [] => (),
                        _ => return Err("unexpected content-length header value".into()),
                    },
                    "transfer-encoding" => match &*values {
                        [encoding] => {
                            if encoding == "chunked" {
                                body_metadata_builder.content_length(ContentLength::Chunked);
                            } else {
                                return Err(format!(
                                    "transfer-encoding `{encoding}` is not supported yet"
                                )
                                .into());
                            }
                        }
                        [] => (),
                        _ => return Err("unexpected transfer-encoding header value".into()),
                    },
                    "content-encoding" => {
                        if !values.is_empty() {
                            body_metadata_builder.content_encoding(Some(
                                values
                                    .iter()
                                    .map(|v| ContentEncoding::from_str(v))
                                    .collect::<Result<_, _>>()?,
                            ));
                        }
                    }
                    "boundary" => match &*values {
                        [boundary] => {
                            body_metadata_builder.boundary(Some(boundary.to_string()));
                        }
                        [] => (),
                        _ => return Err("unexpected boundary header value".into()),
                    },
                    _ => {}
                }

                headers
                    .entry(key)
                    .or_insert_with(Vec::new)
                    .extend_from_slice(&values);
            }
        }

        header_metadata_builder.body_metadata(
            Lazy::get(&body_metadata_builder)
                .map(BodyMetadataBuilder::build)
                .transpose()?,
        );
        Ok((headers, header_metadata_builder.build()?))
    }

    fn is_special_header_valid(key: &str, value: &str) -> bool {
        match key {
            "authorization" => AUTHORIZATION_REGEX.is_match(value),
            "accept" => {
                !value.is_empty()
                    && value
                        .split(",")
                        .all(|part| Mime::from_str(part.trim()).is_ok())
            }
            _ => true,
        }
    }
}

#[derive(Builder, Debug)]
pub struct HeaderMetadata {
    pub host: Host,
    pub body_metadata: Option<BodyMetadata>,
}

#[derive(Clone, Builder, Debug)]
pub struct BodyMetadata {
    pub content_type: Mime,
    pub content_length: ContentLength,
    #[builder(default)]
    pub content_encoding: Option<Vec<ContentEncoding>>,
    #[builder(default)]
    pub boundary: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ContentLength {
    Fixed(u64),
    Chunked,
}

#[derive(Clone, Debug, Copy, PartialEq, Eq, Hash)]
pub enum ContentEncoding {
    Gzip,
    Compress,
    Deflate,
    Br,
    Zstd,
    /// Dictionary-Compressed Brotli
    Dcb,
    /// Dictionary-Compressed Zstd
    Dcz,
}

impl FromStr for ContentEncoding {
    type Err = String;

    fn from_str(encoding: &str) -> Result<Self, Self::Err> {
        match encoding {
            "gzip" => Ok(ContentEncoding::Gzip),
            "compress" => Ok(ContentEncoding::Compress),
            "deflate" => Ok(ContentEncoding::Deflate),
            "br" => Ok(ContentEncoding::Br),
            "zstd" => Ok(ContentEncoding::Zstd),
            "dcb" => Ok(ContentEncoding::Dcb),
            "dcz" => Ok(ContentEncoding::Dcz),
            _ => Err(format!("Unknown content encoding: {}", encoding)),
        }
    }
}

type ParsedHeader = (HashMap<String, Vec<String>>, HeaderMetadata);

pub struct Body {
    pub reader: Box<dyn Read>,
    pub size_hint: Option<u64>,
    pub encoding: Option<Vec<ContentEncoding>>,
    pub content_type: Mime,
    pub boundary: Option<String>,
}

impl Body {
    pub fn new<T: Read + 'static>(reader: T, metadata: BodyMetadata) -> Self {
        Body {
            reader: Box::new(reader),
            encoding: metadata.content_encoding,
            size_hint: match metadata.content_length {
                ContentLength::Fixed(length) => Some(length),
                ContentLength::Chunked => None,
            },
            content_type: metadata.content_type,
            boundary: metadata.boundary,
        }
    }

    pub fn into_reader(self) -> Result<Box<dyn Read + 'static>, Box<dyn std::error::Error>> {
        let decoder = match self.encoding {
            Some(encodings) => {
                let mut decoder = self.reader;
                for encoding in encodings.into_iter().rev() {
                    decoder = Body::wrap_encoding_decoder(decoder, encoding)?;
                }
                decoder
            }
            None => self.reader,
        };

        Ok(Box::new(
            decoder.take(self.size_hint.unwrap_or(MAX_BODY_SIZE)),
        ))
    }

    fn wrap_encoding_decoder(
        reader: Box<dyn Read + 'static>,
        encoding: ContentEncoding,
    ) -> Result<Box<dyn Read + 'static>, Box<dyn std::error::Error>> {
        Ok(match encoding {
            ContentEncoding::Gzip => Box::new(GzDecoder::new(reader)),
            ContentEncoding::Deflate => Box::new(DeflateDecoder::new(reader)),
            ContentEncoding::Br => Box::new(BrotliDecompressor::new(reader, BROTLI_BUFFER_SIZE)),
            ContentEncoding::Zstd => Box::new(ZstdDecoder::new(reader)?),
            ContentEncoding::Compress | ContentEncoding::Dcb | ContentEncoding::Dcz => {
                return Err(format!("{:?} encoding not supported yet", encoding).into());
            }
        })
    }

    pub fn into_body_data(self) -> Result<BodyData, Box<dyn std::error::Error>> {
        let content_type = self.content_type.clone();
        let boundary = self.boundary.clone();
        let size_hint = self.size_hint;
        let mut reader = self.into_reader()?;
        Ok(match content_type.essence_str() {
            "application/x-www-form-urlencoded" => {
                let mut form_data: HashMap<String, FormDataValue> = HashMap::new();

                let mut main_buf = String::new();
                let mut temp_buf = [0u8; 8192]; // 8KB
                let mut decoded_buf = String::with_capacity(8192);
                let mut decoder = UTF_8.new_decoder();

                loop {
                    let bytes_read = reader.read(&mut temp_buf)?;

                    let (decode_result, bytes_parsed, _) = decoder.decode_to_string(
                        &temp_buf[..bytes_read],
                        &mut decoded_buf,
                        bytes_read == 0,
                    );

                    if decode_result == encoding_rs::CoderResult::OutputFull {
                        unreachable!(
                            "Output buffer is full when it is intended to be larger than the input buffer"
                        );
                    }

                    let mut start_pos = 0;
                    main_buf.push_str(&decoded_buf[..bytes_parsed]);

                    let mut extract_pair = |start_pos, end_pos| -> Result<usize, String> {
                        let pair = &main_buf[start_pos..end_pos];
                        let (key, value) = pair.split_once("=").ok_or("Invalid form data")?;
                        form_data.insert(key.to_string(), FormDataValue::Text(value.to_string()));
                        Ok(end_pos + 1)
                    };

                    if bytes_read > 0 {
                        while let Some(ampersand_pos) = main_buf[start_pos..].find("&") {
                            start_pos = extract_pair(start_pos, ampersand_pos)?;
                        }
                        if start_pos < main_buf.len() {
                            main_buf.drain(..start_pos);
                        }
                    } else {
                        extract_pair(start_pos, main_buf.len())?;
                        break;
                    }
                }

                BodyData::FormData(form_data)
            }
            "multipart/form-data" => {
                let mut data: HashMap<String, FormDataValue> = HashMap::new();

                let boundary = boundary.ok_or("Missing boundary for multipart")?;
                let part_boundary = format!("--{boundary}\r\n");
                let end_boundary = format!("--{boundary}--\r\n");

                let mut main_buf = Vec::new();
                let mut temp_buf = [0u8; 8192]; // 8KB

                loop {
                    // EOF and empty buffer
                    let bytes_read = reader.read(&mut temp_buf)?;
                    if bytes_read == 0 && main_buf.is_empty() {
                        break;
                    }

                    main_buf.extend_from_slice(&temp_buf[..bytes_read]);

                    loop {
                        // buffer position
                        let part_pos = Self::find_subslice(&main_buf, part_boundary.as_bytes());
                        let end_pos = Self::find_subslice(&main_buf, end_boundary.as_bytes());

                        let (index, is_end_boundary, boundary_len) = match (part_pos, end_pos) {
                            (Some(part_pos), Some(end_pos)) => {
                                if part_pos < end_pos {
                                    (part_pos, false, part_boundary.len())
                                } else {
                                    (end_pos, true, end_boundary.len())
                                }
                            }
                            (Some(part_pos), None) => (part_pos, false, part_boundary.len()),
                            (None, Some(end_pos)) => (end_pos, true, end_boundary.len()),
                            _ => break,
                        };

                        let part_data = &main_buf[..index];

                        if !part_data.is_empty() {
                            let (name, value) =
                                Self::parse_part(&part_data[..(part_data.len() - 2)])?;
                            data.insert(name, value);
                        }

                        main_buf.drain(..index + boundary_len);

                        if is_end_boundary {
                            return Ok(BodyData::FormData(data));
                        }
                    }

                    if bytes_read == 0 {
                        // couldn't reach end boundary
                        return Err("multipart: missing end boundary".into());
                    }
                }

                BodyData::FormData(data)
            }
            "text/plain" => {
                let mut text = match size_hint {
                    Some(size) => String::with_capacity(size as usize),
                    None => String::new(),
                };
                reader.read_to_string(&mut text)?;
                BodyData::Text(text)
            }
            _ => {
                let mut data = match size_hint {
                    Some(size) => Vec::with_capacity(size as usize),
                    None => Vec::new(),
                };
                reader.read_to_end(&mut data)?;
                BodyData::Other { content_type, data }
            }
        })
    }

    fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
        haystack
            .windows(needle.len())
            .position(|window| window == needle)
    }

    fn parse_part(part_data: &[u8]) -> Result<(String, FormDataValue), Box<dyn std::error::Error>> {
        let separator = b"\r\n\r\n";
        let sep_pos =
            Self::find_subslice(part_data, separator).ok_or("missing header/body seperator")?;

        let headers_raw = &part_data[..sep_pos];
        let body_raw = &part_data[sep_pos + separator.len()..];

        let (name, filename, content_type) = Self::parse_part_headers(headers_raw)?;

        if let Some(filename) = filename {
            Ok((
                name,
                FormDataValue::File {
                    filename,
                    content_type: content_type
                        .unwrap_or_else(|| "application/octet-stream".to_string()),
                    data: body_raw.to_vec(), //change this avoid OOM
                },
            ))
        } else {
            Ok((
                name,
                FormDataValue::Text(String::from_utf8(body_raw.to_vec())?), //change this avoid OOM
            ))
        }
    }

    fn parse_part_headers(headers_raw: &[u8]) -> Result<PartHeaders, Box<dyn std::error::Error>> {
        let header_str = std::str::from_utf8(headers_raw);
        let mut name = None;
        let mut filename = None;
        let mut content_type = None;

        for line in header_str?.lines() {
            if let Some((header_name, header_value)) = line.split_once(':') {
                let header_name_lowercase = header_name.trim().to_lowercase();
                let header_value = header_value.trim();

                if header_name_lowercase == "content-disposition" {
                    for param in header_value.split(';') {
                        if let Some((key, value)) = param.trim().split_once('=') {
                            let decoded_val =
                                Self::decode_disposition_param(value.trim_matches('"'));

                            match key.trim() {
                                "name" => name = Some(decoded_val),
                                "filename" => filename = Some(decoded_val),
                                _ => {}
                            }
                        }
                    }
                } else if header_name_lowercase == "content-type" {
                    content_type = Some(header_value.to_string());
                }
            }
        }
        let name = name.ok_or("missing 'name' in Content-Disposition")?;
        Ok((name, filename, content_type))
    }

    fn decode_disposition_param(s: &str) -> String {
        s.replace("%0A", "\n")
            .replace("%0D", "\r")
            .replace("%22", "\"")
    }
}

type PartHeaders = (String, Option<String>, Option<String>);

pub enum BodyData {
    Json(serde_json::Value),
    FormData(HashMap<String, FormDataValue>),
    Text(String),
    Other { content_type: Mime, data: Vec<u8> },
}

pub enum FormDataValue {
    Text(String),
    File {
        filename: String,
        content_type: String,
        data: Vec<u8>, // use PathBuf for OOM
    },
}
#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_parse_start_line_comprehensive() {
        type TestCase<'a> = (HttpMethod, &'a str, HttpVersion);
        let test_cases: Vec<(&str, Result<TestCase<'_>, &str>)> = vec![
            (
                "GET / HTTP/1.1",
                Ok((HttpMethod::GET, "/", HttpVersion::V1_1)),
            ),
            (
                "POST /submit HTTP/1.1",
                Ok((HttpMethod::POST, "/submit", HttpVersion::V1_1)),
            ),
            (
                "PUT /items/123 HTTP/1.1",
                Ok((HttpMethod::PUT, "/items/123", HttpVersion::V1_1)),
            ),
            (
                "DELETE /items/123 HTTP/1.1",
                Ok((HttpMethod::DELETE, "/items/123", HttpVersion::V1_1)),
            ),
            (
                "PATCH /items/123 HTTP/1.1",
                Ok((HttpMethod::PATCH, "/items/123", HttpVersion::V1_1)),
            ),
            (
                "HEAD / HTTP/1.1",
                Ok((HttpMethod::HEAD, "/", HttpVersion::V1_1)),
            ),
            (
                "CUSTOM /api HTTP/1.1",
                Ok((
                    HttpMethod::Other("CUSTOM".to_string()),
                    "/api",
                    HttpVersion::V1_1,
                )),
            ),
            (
                "GET /path/with/multiple/segments HTTP/1.1",
                Ok((
                    HttpMethod::GET,
                    "/path/with/multiple/segments",
                    HttpVersion::V1_1,
                )),
            ),
            (
                "GET /search?q=test&lang=ko HTTP/1.1",
                Ok((HttpMethod::GET, "/search?q=test&lang=ko", HttpVersion::V1_1)),
            ),
            (
                "GET /path-with-hyphen_and_underscore HTTP/1.1",
                Ok((
                    HttpMethod::GET,
                    "/path-with-hyphen_and_underscore",
                    HttpVersion::V1_1,
                )),
            ),
            (
                "GET /path.with.dots HTTP/1.1",
                Ok((HttpMethod::GET, "/path.with.dots", HttpVersion::V1_1)),
            ),
            ("GET /", Err("Invalid request line")),
            ("GET HTTP/1.1", Err("Invalid request line")),
            ("GET / TTP/1.1", Err("Invalid request line")),
            ("GET  /two-spaces HTTP/1.1", Err("Invalid request line")),
            ("", Err("Invalid request line")),
            ("\r\n", Err("Invalid request line")),
        ];

        for (input, expected) in test_cases {
            let result = Request::parse_start_line(input);
            match (result, expected) {
                (Ok((method, path, version)), Ok((exp_method, exp_path, exp_version))) => {
                    assert_eq!(
                        method, exp_method,
                        "Mismatch in method for input: {}",
                        input
                    );
                    assert_eq!(path, exp_path, "Mismatch in path for input: {}", input);
                    assert_eq!(
                        version, exp_version,
                        "Mismatch in version for input: {}",
                        input
                    );
                }
                (Err(e), Err(exp_e)) => {
                    assert_eq!(e, exp_e, "Mismatch in error for input: {}", input);
                }
                (res, exp) => {
                    panic!(
                        "Result {:?} does not match expected {:?} for input: {}",
                        res, exp, input
                    );
                }
            }
        }
    }

    #[test]
    #[should_panic(expected = "Unsupported HTTP version")]
    fn test_parse_start_line_unsupported_version_panic() {
        let http_1_0 = "GET / HTTP/2.0";
        let _ = Request::parse_start_line(http_1_0);
    }

    #[test]
    fn test_parse_headers_comprehensive() {
        let raw_headers = concat!(
            "Host: example.com\r\n",
            "User-Agent: gimbap-test/1.0\r\n",
            "ACCEPT: text/html, application/xhtml+xml, application/xml;q=0.9, */*;q=0.8\r\n",
            "content-type: application/json\r\n",
            "Content-Length: 42\r\n",
            "Content-Encoding: gzip, deflate\r\n",
            "Authorization: Basic dXNlcjpwYXNz\r\n",
            "Invalid@Header: should be ignored\r\n",
            "Another-Header: with, multiple, values\r\n",
            "Whitespace-Header: value with spaces \r\n",
            "\r\n"
        );

        let (headers, header_metadata) =
            Request::parse_headers(&mut raw_headers.as_bytes()).unwrap();

        assert_eq!(
            headers.get("host").unwrap(),
            &vec!["example.com".to_string()]
        );
        assert_eq!(
            headers.get("user-agent").unwrap(),
            &vec!["gimbap-test/1.0".to_string()]
        );
        assert_eq!(
            headers.get("accept").unwrap(),
            &vec![
                "text/html".to_string(),
                "application/xhtml+xml".to_string(),
                "application/xml;q=0.9".to_string(),
                "*/*;q=0.8".to_string()
            ]
        );
        assert_eq!(
            headers.get("content-type").unwrap(),
            &vec!["application/json".to_string()]
        );
        assert_eq!(
            headers.get("content-length").unwrap(),
            &vec!["42".to_string()]
        );
        assert_eq!(
            headers.get("content-encoding").unwrap(),
            &vec!["gzip".to_string(), "deflate".to_string()]
        );

        assert_eq!(
            headers.get("authorization").unwrap(),
            &vec!["Basic dXNlcjpwYXNz".to_string()]
        );
        assert_eq!(
            headers.get("another-header").unwrap(),
            &vec![
                "with".to_string(),
                "multiple".to_string(),
                "values".to_string()
            ]
        );
        assert_eq!(
            headers.get("whitespace-header").unwrap(),
            &vec!["value with spaces".to_string()]
        );

        assert!(!headers.contains_key("Invalid@Header"));

        assert_eq!(header_metadata.host, Host::parse("example.com").unwrap());

        let body_metadata = header_metadata.body_metadata.as_ref().unwrap();
        assert_eq!(body_metadata.content_type, mime::APPLICATION_JSON);
        assert_eq!(body_metadata.content_length, ContentLength::Fixed(42));
        assert_eq!(
            body_metadata.content_encoding.as_ref().unwrap(),
            &[ContentEncoding::Gzip, ContentEncoding::Deflate]
        );

        let chunked_headers =
            "Host: example.com\r\nContent-Type: text/plain\r\nTransfer-Encoding: chunked\r\n\r\n";
        let (_, chunked_metadata) =
            Request::parse_headers(&mut chunked_headers.as_bytes()).unwrap();
        let chunked_body_metadata = chunked_metadata.body_metadata.as_ref().unwrap();
        assert_eq!(chunked_body_metadata.content_type, mime::TEXT_PLAIN);
        assert_eq!(chunked_body_metadata.content_length, ContentLength::Chunked);
        assert_eq!(chunked_body_metadata.content_encoding, None);

        let empty_headers = "\r\n";
        let result = Request::parse_headers(&mut empty_headers.as_bytes());
        assert!(result.is_err());
    }

    #[test]
    fn test_transfer_encoding_identity() {
        let headers =
            "Host: example.com\r\nContent-Type: text/plain\r\nTransfer-Encoding: identity\r\n\r\n";
        let result = Request::parse_headers(&mut headers.as_bytes());
        assert!(result.is_err());
        let error = result.unwrap_err();
        let expected_error_message = "transfer-encoding `identity` is not supported yet";
        assert_eq!(error.to_string(), expected_error_message);
    }
}
