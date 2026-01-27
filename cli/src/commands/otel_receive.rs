use crate::cache::debug_log;
use crate::editors::codex::otel::{self, CodexOtelContext, CodexOtelEvent};
use crate::error::Result;
use crate::event_bus;
use crate::events::{AikiEvent, AikiSessionStartPayload, AikiTurnStartedPayload};
use crate::provenance::{AgentType, DetectionMethod};
use crate::session::{AikiSession, AikiSessionFile};
use chrono::Utc;
use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

/// Run the OTel receiver: read HTTP from stdin, parse OTLP, update session state.
///
/// Socket-activated: reads a single HTTP request, processes it, responds 200 OK, exits.
/// All errors are non-fatal (always responds 200 OK to never block Codex).
pub fn run() -> Result<()> {
    // Read HTTP request from stdin
    let request = match read_http_request() {
        Ok(r) => r,
        Err(e) => {
            debug_log(|| format!("Failed to read HTTP request: {}", e));
            write_http_response();
            return Ok(());
        }
    };

    // Validate endpoint and content type
    if request.path.contains("/v1/traces") {
        debug_log(|| {
            "Received traces request at /v1/traces - only /v1/logs is supported. \
             Update Codex config: endpoint = \"http://127.0.0.1:19876/v1/logs\""
                .to_string()
        });
        write_http_response();
        return Ok(());
    }

    if let Some(ref ct) = request.content_type {
        let ct_lower = ct.to_lowercase();
        if ct_lower.contains("application/json") {
            debug_log(|| {
                format!(
                    "OTLP JSON payload detected (Content-Type: {}). Protobuf parsing may fail.",
                    ct
                )
            });
        } else if !ct_lower.contains("application/x-protobuf")
            && !ct_lower.contains("application/protobuf")
        {
            debug_log(|| {
                format!(
                    "Unexpected Content-Type: {}. Expected application/x-protobuf.",
                    ct
                )
            });
            // Still try to parse - some clients may not set content-type correctly
        }
    }

    // Decompress if gzip (or if payload starts with gzip magic bytes)
    let body_is_gzip = body_looks_gzipped(&request.body);
    let wants_gzip = request
        .content_encoding
        .iter()
        .any(|enc| enc == "gzip");
    if body_is_gzip && !wants_gzip {
        debug_log(|| "Body looks gzipped but Content-Encoding is missing gzip".to_string());
    }

    let body = if wants_gzip || body_is_gzip {
        match decompress_gzip(&request.body) {
            Ok(decompressed) => decompressed,
            Err(e) => {
                debug_log(|| format!("Failed to decompress gzip body: {}", e));
                write_http_response();
                return Ok(());
            }
        }
    } else {
        request.body
    };

    // Parse OTLP protobuf and process events
    // Try traces first (Codex sends traces even to /v1/logs endpoint)
    let mut events = otel::parse_otlp_traces(&body);

    // Fall back to logs parsing if no trace events found
    if events.is_empty() {
        events = otel::parse_otlp_logs(&body);
    }

    if events.is_empty() && !body.is_empty() {
        debug_log(|| {
            format!(
                "No OTel events parsed (content_type={:?}, body_len={})",
                request.content_type,
                body.len()
            )
        });
        dump_otlp_payload(&body, request.content_type.as_deref(), &request.path);
    }

    for (event, context) in events {
        process_event(event, &context);
    }

    // Always respond 200 OK
    write_http_response();
    Ok(())
}

/// Parsed HTTP request from stdin
struct HttpRequest {
    body: Vec<u8>,
    content_encoding: Vec<String>,
    /// Request path (e.g., "/v1/logs" or "/v1/traces")
    path: String,
    /// Content-Type header value
    content_type: Option<String>,
}

/// Read an HTTP request from stdin.
///
/// Parses the HTTP/1.1 request line and headers to find Content-Length
/// and Content-Encoding. Then reads the body (Content-Length or chunked).
fn read_http_request() -> io::Result<HttpRequest> {
    let mut stdin = io::stdin().lock();
    let mut header_buf = Vec::with_capacity(4096);

    // Read headers byte by byte until we find \r\n\r\n
    let mut last_four = [0u8; 4];
    loop {
        let mut byte = [0u8; 1];
        stdin.read_exact(&mut byte)?;
        header_buf.push(byte[0]);

        // Shift last_four window
        last_four[0] = last_four[1];
        last_four[1] = last_four[2];
        last_four[2] = last_four[3];
        last_four[3] = byte[0];

        if &last_four == b"\r\n\r\n" {
            break;
        }

        // Safety: prevent unbounded header reads
        if header_buf.len() > 65536 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "HTTP headers too large",
            ));
        }
    }

    // Parse headers
    let headers_str = String::from_utf8_lossy(&header_buf);
    let mut content_length: Option<usize> = None;
    let mut content_encoding = Vec::new();
    let mut transfer_encoding = Vec::new();
    let mut method = String::new();
    let mut path = String::new();
    let mut content_type = None;

    for (i, line) in headers_str.lines().enumerate() {
        if i == 0 {
            // Parse request line: "POST /v1/logs HTTP/1.1"
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                method = parts[0].to_string();
                path = parts[1].to_string();
            }
            continue;
        }
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        let name = name.trim().to_lowercase();
        let value = value.trim();

        match name.as_str() {
            "content-length" => {
                content_length = value.parse().ok();
            }
            "content-encoding" => {
                content_encoding.extend(parse_header_list(value));
            }
            "transfer-encoding" => {
                transfer_encoding.extend(parse_header_list(value));
            }
            "content-type" => {
                content_type = Some(value.to_string());
            }
            _ => {}
        }
    }

    let is_chunked = transfer_encoding.iter().any(|enc| enc == "chunked");

    // Read body
    let body = if is_chunked {
        read_chunked_body(&mut stdin)?
    } else {
        let len = content_length.unwrap_or(0);
        let mut body = vec![0u8; len];
        if len > 0 {
            stdin.read_exact(&mut body)?;
        }
        body
    };

    debug_log(|| {
        format!(
            "OTel HTTP: method={}, path={}, content_length={:?}, transfer_encoding={:?}, content_encoding={:?}, content_type={:?}, body_len={}",
            if method.is_empty() { "-" } else { method.as_str() },
            if path.is_empty() { "-" } else { path.as_str() },
            content_length,
            transfer_encoding,
            content_encoding,
            content_type,
            body.len()
        )
    });

    Ok(HttpRequest {
        body,
        content_encoding,
        path,
        content_type,
    })
}

fn parse_header_list(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(|v| v.trim().to_lowercase())
        .filter(|v| !v.is_empty())
        .collect()
}

fn read_crlf_line(reader: &mut impl Read) -> io::Result<Vec<u8>> {
    let mut buf = Vec::new();
    let mut last_two = [0u8; 2];

    loop {
        let mut byte = [0u8; 1];
        reader.read_exact(&mut byte)?;
        buf.push(byte[0]);

        last_two[0] = last_two[1];
        last_two[1] = byte[0];

        if &last_two == b"\r\n" {
            buf.truncate(buf.len() - 2);
            return Ok(buf);
        }

        if buf.len() > 65536 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "HTTP line too long",
            ));
        }
    }
}

fn read_chunked_body(reader: &mut impl Read) -> io::Result<Vec<u8>> {
    let mut body = Vec::new();

    loop {
        let line = read_crlf_line(reader)?;
        let line_str = String::from_utf8_lossy(&line);
        let size_str = line_str.split(';').next().unwrap_or("").trim();

        if size_str.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Missing chunk size",
            ));
        }

        let size = usize::from_str_radix(size_str, 16).map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidData, "Invalid chunk size")
        })?;

        if size == 0 {
            // Consume trailer headers (if any), then final CRLF
            loop {
                let trailer = read_crlf_line(reader)?;
                if trailer.is_empty() {
                    break;
                }
            }
            break;
        }

        let mut chunk = vec![0u8; size];
        reader.read_exact(&mut chunk)?;
        body.extend_from_slice(&chunk);

        let mut crlf = [0u8; 2];
        reader.read_exact(&mut crlf)?;
        if &crlf != b"\r\n" {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid chunk terminator",
            ));
        }
    }

    Ok(body)
}

#[inline]
fn body_looks_gzipped(data: &[u8]) -> bool {
    data.len() >= 2 && data[0] == 0x1f && data[1] == 0x8b
}

/// Decompress gzip-encoded body
fn decompress_gzip(data: &[u8]) -> io::Result<Vec<u8>> {
    use flate2::read::GzDecoder;
    let mut decoder = GzDecoder::new(data);
    let mut decompressed = Vec::new();
    decoder.read_to_end(&mut decompressed)?;
    Ok(decompressed)
}

/// Write HTTP 200 OK response to stdout
fn write_http_response() {
    let response = "HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n";
    let _ = io::stdout().write_all(response.as_bytes());
    let _ = io::stdout().flush();
}

/// Dump raw OTLP payload to a file (always on parse failure).
fn dump_otlp_payload(body: &[u8], content_type: Option<&str>, request_path: &str) {
    let dump_dir = std::path::PathBuf::from("/tmp");
    if let Err(e) = std::fs::create_dir_all(&dump_dir) {
        debug_log(|| format!("Failed to create OTel dump dir: {}", e));
        return;
    }

    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let file_name = format!(
        "aiki-otel-dump-{}-{}-{}.bin",
        ts.as_secs(),
        ts.subsec_millis(),
        std::process::id()
    );
    let bin_path = dump_dir.join(file_name);

    if let Err(e) = std::fs::write(&bin_path, body) {
        debug_log(|| format!("Failed to write OTel dump: {}", e));
        return;
    }

    let meta = format!(
        "request_path={}\ncontent_type={}\nbody_len={}\n",
        request_path,
        content_type.unwrap_or("-"),
        body.len()
    );
    let meta_path = bin_path.with_extension("bin.meta");
    if let Err(e) = std::fs::write(&meta_path, meta) {
        debug_log(|| format!("Failed to write OTel dump metadata: {}", e));
    }

    debug_log(|| format!("Wrote OTel payload dump to {}", bin_path.display()));
}

/// Process a single Codex OTel event
///
/// Turn tracking uses JJ history (same as stdin integrations).
/// Modified files from tool_result are ignored - they come from JJ file tracking.
fn process_event(event: CodexOtelEvent, context: &CodexOtelContext) {
    match event {
        CodexOtelEvent::ConversationStarts { conversation_id } => {
            debug_log(|| format!("OTel: conversation_starts: {}", conversation_id));

            let Some(cwd) = context.cwd.clone() else {
                return;
            };

            maybe_emit_session_started(&conversation_id, context, &cwd);
        }

        CodexOtelEvent::UserPrompt {
            conversation_id,
            prompt,
        } => {
            debug_log(|| {
                format!(
                    "OTel: user_prompt: conv={}, prompt_len={}",
                    conversation_id,
                    prompt.as_ref().map_or(0, |p| p.len())
                )
            });

            let Some(cwd) = context.cwd.clone() else {
                return;
            };

            maybe_emit_turn_started(&conversation_id, context, &cwd, prompt.unwrap_or_default());
        }

        CodexOtelEvent::ToolResult { conversation_id, .. } => {
            // Modified files come from JJ file tracking, not OTel
            debug_log(|| format!("OTel: tool_result: conv={} (ignored, files from JJ)", conversation_id));
        }

        CodexOtelEvent::Unknown { event_name } => {
            debug_log(|| format!("OTel: acknowledged (not mapped): {}", event_name));
        }
    }
}

fn maybe_emit_session_started(
    conversation_id: &str,
    context: &CodexOtelContext,
    cwd: &PathBuf,
) {
    // Check if session already started via session file existence
    let session = AikiSession::new(
        AgentType::Codex,
        conversation_id,
        context.agent_version.as_deref(),
        DetectionMethod::Hook,
    )
    .with_parent_pid(context.agent_pid);

    let session_file = AikiSessionFile::new(&session);
    if session_file.exists() {
        return;
    }

    if !cwd.is_absolute() {
        debug_log(|| {
            format!(
                "OTel: skipping session.started for {} (cwd not absolute)",
                conversation_id
            )
        });
        return;
    }

    let now = Utc::now();
    let event = AikiEvent::SessionStarted(AikiSessionStartPayload {
        session,
        cwd: cwd.clone(),
        timestamp: now,
    });

    if let Err(e) = event_bus::dispatch(event) {
        debug_log(|| format!("Failed to dispatch session.started from OTel: {}", e));
    }
    // Session file is created by the session.started handler
}

fn maybe_emit_turn_started(
    conversation_id: &str,
    context: &CodexOtelContext,
    cwd: &PathBuf,
    prompt: String,
) {
    let session = AikiSession::new(
        AgentType::Codex,
        conversation_id,
        context.agent_version.as_deref(),
        DetectionMethod::Hook,
    )
    .with_parent_pid(context.agent_pid);

    let session_file = AikiSessionFile::new(&session);

    // Need session to exist first
    if !session_file.exists() {
        return;
    }

    // OTel user_prompt events may arrive multiple times or out of order.
    // The turn.started handler will query JJ for the actual turn number and
    // record the prompt. We just dispatch the event and let the handler
    // manage deduplication via JJ history.

    if !cwd.is_absolute() {
        debug_log(|| {
            format!(
                "OTel: skipping turn.started for {} (cwd not absolute)",
                conversation_id
            )
        });
        return;
    }

    let now = Utc::now();
    let event = AikiEvent::TurnStarted(AikiTurnStartedPayload {
        session,
        cwd: cwd.clone(),
        timestamp: now,
        turn: crate::events::Turn::unknown(), // Set by handle_turn_started
        prompt,
        injected_refs: vec![],
    });

    if let Err(e) = event_bus::dispatch(event) {
        debug_log(|| format!("Failed to dispatch turn.started from OTel: {}", e));
    }
}
