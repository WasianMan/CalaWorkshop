//! Steam QR-code auth sessions via `IAuthenticationService` (public web API).
//!
//! This speaks just enough of Steam's protobuf-over-HTTPS RPC to drive
//! `BeginAuthSessionViaQR` / `PollAuthSessionStatus`: the user scans a QR code
//! with the Steam Mobile app, approves the sign-in, and Steam hands back a
//! refresh token plus the account name. No password ever touches the helper.
//!
//! The two messages involved are tiny, so we hand-encode/decode the protobuf
//! wire format here instead of pulling in a protobuf toolchain. Field numbers
//! come from Valve's `steammessages_auth.steamclient.proto` (as implemented by
//! SteamKit2 / node-steam-session).
//!
//! The auth session is created with platform type `SteamClient` so the token
//! Steam issues carries the `client` audience — the kind of session a Steam
//! client (and therefore SteamCMD) uses. Whether the local `steamcmd` build
//! accepts the refresh token in place of a password is verified at runtime by
//! the login-session worker; this module only produces the token.

use anyhow::{anyhow, bail, Context, Result};
use base64::prelude::*;

const STEAM_API: &str = "https://api.steampowered.com";
/// EAuthTokenPlatformType::k_EAuthTokenPlatformType_SteamClient
const PLATFORM_STEAM_CLIENT: u64 = 1;
/// EOSType for a generic Linux host; Steam only uses this for display.
const OS_TYPE_LINUX: i64 = -203;

// ---------------------------------------------------------------------------
// Minimal protobuf wire helpers
// ---------------------------------------------------------------------------

fn put_varint(out: &mut Vec<u8>, mut n: u64) {
    loop {
        let byte = (n & 0x7f) as u8;
        n >>= 7;
        if n == 0 {
            out.push(byte);
            return;
        }
        out.push(byte | 0x80);
    }
}

fn put_tag(out: &mut Vec<u8>, field: u32, wire: u8) {
    put_varint(out, ((field as u64) << 3) | wire as u64);
}

fn put_varint_field(out: &mut Vec<u8>, field: u32, n: u64) {
    put_tag(out, field, 0);
    put_varint(out, n);
}

fn put_bytes_field(out: &mut Vec<u8>, field: u32, bytes: &[u8]) {
    put_tag(out, field, 2);
    put_varint(out, bytes.len() as u64);
    out.extend_from_slice(bytes);
}

fn put_str_field(out: &mut Vec<u8>, field: u32, s: &str) {
    put_bytes_field(out, field, s.as_bytes());
}

/// A decoded top-level protobuf field (we only need varints and byte blobs).
#[derive(Debug, Clone)]
enum PbValue {
    Varint(u64),
    Bytes(Vec<u8>),
}

/// Decode a protobuf message into `(field_number, value)` pairs, skipping
/// fixed32/fixed64 fields we don't use. Errors on truncated input.
fn decode_fields(mut buf: &[u8]) -> Result<Vec<(u32, PbValue)>> {
    fn take_varint(buf: &mut &[u8]) -> Result<u64> {
        let mut n: u64 = 0;
        for shift in (0..64).step_by(7) {
            let (&byte, rest) = buf.split_first().context("truncated varint")?;
            *buf = rest;
            n |= ((byte & 0x7f) as u64) << shift;
            if byte & 0x80 == 0 {
                return Ok(n);
            }
        }
        bail!("varint too long");
    }

    let mut out = Vec::new();
    while !buf.is_empty() {
        let key = take_varint(&mut buf)?;
        let field = (key >> 3) as u32;
        match key & 7 {
            0 => out.push((field, PbValue::Varint(take_varint(&mut buf)?))),
            1 => {
                if buf.len() < 8 {
                    bail!("truncated fixed64");
                }
                buf = &buf[8..];
            }
            2 => {
                let len = take_varint(&mut buf)? as usize;
                if buf.len() < len {
                    bail!("truncated length-delimited field");
                }
                out.push((field, PbValue::Bytes(buf[..len].to_vec())));
                buf = &buf[len..];
            }
            5 => {
                if buf.len() < 4 {
                    bail!("truncated fixed32");
                }
                buf = &buf[4..];
            }
            wire => bail!("unsupported protobuf wire type {wire}"),
        }
    }
    Ok(out)
}

fn field_varint(fields: &[(u32, PbValue)], field: u32) -> Option<u64> {
    fields.iter().find_map(|(f, v)| match v {
        PbValue::Varint(n) if *f == field => Some(*n),
        _ => None,
    })
}

fn field_bytes(fields: &[(u32, PbValue)], field: u32) -> Option<&[u8]> {
    fields.iter().find_map(|(f, v)| match v {
        PbValue::Bytes(b) if *f == field => Some(b.as_slice()),
        _ => None,
    })
}

fn field_string(fields: &[(u32, PbValue)], field: u32) -> Option<String> {
    field_bytes(fields, field).map(|b| String::from_utf8_lossy(b).into_owned())
}

// ---------------------------------------------------------------------------
// RPC transport
// ---------------------------------------------------------------------------

/// POST a protobuf request to an `IAuthenticationService` method and return the
/// decoded response fields. Steam reports the RPC result in the `x-eresult`
/// header (1 = OK) — HTTP status is 200 even for most protocol-level errors.
async fn call(
    client: &reqwest::Client,
    method: &str,
    request: &[u8],
) -> Result<(u64, Vec<(u32, PbValue)>)> {
    let url = format!("{STEAM_API}/IAuthenticationService/{method}/v1/");
    let response = client
        .post(&url)
        .form(&[("input_protobuf_encoded", BASE64_STANDARD.encode(request))])
        .send()
        .await
        .with_context(|| format!("calling Steam {method}"))?;

    let eresult: u64 = response
        .headers()
        .get("x-eresult")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let status = response.status();
    let body = response
        .bytes()
        .await
        .with_context(|| format!("reading Steam {method} response"))?;

    if !status.is_success() {
        bail!("Steam {method} returned HTTP {status}");
    }
    let fields =
        decode_fields(&body).with_context(|| format!("decoding Steam {method} response"))?;
    Ok((eresult, fields))
}

// ---------------------------------------------------------------------------
// QR auth session
// ---------------------------------------------------------------------------

/// An in-flight QR auth session, as returned by `BeginAuthSessionViaQR`.
#[derive(Debug, Clone)]
pub struct QrSession {
    pub client_id: u64,
    pub request_id: Vec<u8>,
    /// URL the QR code encodes (`https://s.team/q/...`); rotates periodically.
    pub challenge_url: String,
    /// Suggested poll interval in seconds.
    pub interval: u64,
}

/// Start a QR auth session. Anonymous call — no credentials involved.
pub async fn begin_qr(client: &reqwest::Client, device_name: &str) -> Result<QrSession> {
    // CAuthentication_BeginAuthSessionViaQR_Request:
    //   1: device_friendly_name, 2: platform_type,
    //   3: device_details { 1: device_friendly_name, 2: platform_type, 3: os_type },
    //   4: website_id
    let mut details = Vec::new();
    put_str_field(&mut details, 1, device_name);
    put_varint_field(&mut details, 2, PLATFORM_STEAM_CLIENT);
    put_varint_field(&mut details, 3, OS_TYPE_LINUX as u64);

    let mut req = Vec::new();
    put_str_field(&mut req, 1, device_name);
    put_varint_field(&mut req, 2, PLATFORM_STEAM_CLIENT);
    put_bytes_field(&mut req, 3, &details);
    put_str_field(&mut req, 4, "Client");

    let (eresult, fields) = call(client, "BeginAuthSessionViaQR", &req).await?;
    if eresult != 1 {
        bail!("Steam refused to start a QR auth session (EResult {eresult})");
    }

    // CAuthentication_BeginAuthSessionViaQR_Response:
    //   1: client_id, 2: challenge_url, 3: request_id, 4: interval, 5: allowed_confirmations...
    let client_id =
        field_varint(&fields, 1).ok_or_else(|| anyhow!("QR session response missing client_id"))?;
    let challenge_url = field_string(&fields, 2)
        .filter(|u| !u.is_empty())
        .ok_or_else(|| anyhow!("QR session response missing challenge_url"))?;
    let request_id = field_bytes(&fields, 3)
        .map(<[u8]>::to_vec)
        .filter(|r| !r.is_empty())
        .ok_or_else(|| anyhow!("QR session response missing request_id"))?;

    Ok(QrSession {
        client_id,
        request_id,
        challenge_url,
        // interval is a float on the wire (fixed32), which decode_fields skips;
        // Steam has used 5s for years, and it's only a politeness hint.
        interval: 5,
    })
}

/// One poll of an in-flight QR session.
#[derive(Debug, Clone)]
pub enum QrPoll {
    /// Not scanned/approved yet; keep polling.
    Pending,
    /// Steam rotated the challenge — re-render the QR and poll with the new id.
    NewChallenge {
        client_id: u64,
        challenge_url: String,
    },
    /// The user approved the sign-in in the mobile app.
    Approved {
        account_name: String,
        refresh_token: String,
    },
    /// The session is dead (denied, expired, or revoked); start over.
    Gone(String),
}

/// Poll a QR session for approval.
pub async fn poll_qr(
    client: &reqwest::Client,
    client_id: u64,
    request_id: &[u8],
) -> Result<QrPoll> {
    // CAuthentication_PollAuthSessionStatus_Request: 1: client_id, 2: request_id
    let mut req = Vec::new();
    put_varint_field(&mut req, 1, client_id);
    put_bytes_field(&mut req, 2, request_id);

    let (eresult, fields) = call(client, "PollAuthSessionStatus", &req).await?;
    match eresult {
        1 => {}
        // Expired / FileNotFound / DuplicateRequest all mean this session is
        // no longer pollable.
        n => return Ok(QrPoll::Gone(format!("auth session ended (EResult {n})"))),
    }

    // CAuthentication_PollAuthSessionStatus_Response:
    //   1: new_client_id, 2: new_challenge_url, 3: refresh_token,
    //   4: access_token, 5: had_remote_interaction, 6: account_name
    let refresh_token = field_string(&fields, 3).filter(|t| !t.is_empty());
    let account_name = field_string(&fields, 6).filter(|a| !a.is_empty());
    if let Some(refresh_token) = refresh_token {
        let account_name = account_name
            .ok_or_else(|| anyhow!("Steam approved the login but sent no account name"))?;
        return Ok(QrPoll::Approved {
            account_name,
            refresh_token,
        });
    }

    if let (Some(client_id), Some(challenge_url)) = (
        field_varint(&fields, 1),
        field_string(&fields, 2).filter(|u| !u.is_empty()),
    ) {
        return Ok(QrPoll::NewChallenge {
            client_id,
            challenge_url,
        });
    }

    Ok(QrPoll::Pending)
}

/// Render a challenge URL as a standalone SVG image (dark modules on white).
pub fn challenge_qr_svg(challenge_url: &str) -> Result<String> {
    let code = qrcode::QrCode::new(challenge_url.as_bytes()).context("encoding QR code")?;
    Ok(code
        .render::<qrcode::render::svg::Color<'_>>()
        .min_dimensions(220, 220)
        .quiet_zone(true)
        .dark_color(qrcode::render::svg::Color("#0e141b"))
        .light_color(qrcode::render::svg::Color("#ffffff"))
        .build())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn varint_roundtrip() {
        for n in [0u64, 1, 127, 128, 300, u32::MAX as u64, u64::MAX] {
            let mut buf = Vec::new();
            put_varint_field(&mut buf, 7, n);
            let fields = decode_fields(&buf).unwrap();
            assert_eq!(field_varint(&fields, 7), Some(n));
        }
    }

    #[test]
    fn bytes_and_string_roundtrip() {
        let mut buf = Vec::new();
        put_str_field(&mut buf, 2, "https://s.team/q/1/234");
        put_bytes_field(&mut buf, 3, &[0xde, 0xad, 0xbe, 0xef]);
        let fields = decode_fields(&buf).unwrap();
        assert_eq!(
            field_string(&fields, 2).as_deref(),
            Some("https://s.team/q/1/234")
        );
        assert_eq!(field_bytes(&fields, 3), Some(&[0xde, 0xad, 0xbe, 0xef][..]));
    }

    #[test]
    fn decoder_skips_fixed_width_fields() {
        // field 4: fixed32 (like the real interval field), then field 6: string.
        let mut buf = Vec::new();
        put_tag(&mut buf, 4, 5);
        buf.extend_from_slice(&5.0f32.to_le_bytes());
        put_str_field(&mut buf, 6, "account");
        let fields = decode_fields(&buf).unwrap();
        assert_eq!(field_string(&fields, 6).as_deref(), Some("account"));
    }

    #[test]
    fn decoder_rejects_truncated_input() {
        let mut buf = Vec::new();
        put_str_field(&mut buf, 1, "hello");
        buf.truncate(buf.len() - 2);
        assert!(decode_fields(&buf).is_err());
    }

    #[test]
    fn qr_svg_renders() {
        let svg = challenge_qr_svg("https://s.team/q/1/1234567890").unwrap();
        assert!(svg.starts_with("<?xml") || svg.starts_with("<svg"));
        assert!(svg.contains("svg"));
    }
}
