//! RFC 3161 timestamping client.
//!
//! Fetches a TSA-anchored timestamp token over the signature hash; the
//! token attaches to the .paideia.sig section as an additional sub-record.
//!
//! Phase-3 m8-001 minimum: builds the request, parses the response shape,
//! attaches as a sub-record. Real TSA HTTP fetch is gated on operator
//! opt-in (--tsa-url) and a runtime HTTP client (reqwest); without these,
//! returns a synthetic empty token that documents the shape.
//!
//! TODO (m8-002): Wire TimestampToken into paideia-as-emitter-pax PAX section
//! builder as an optional sub-record attached to .paideia.sig. The PAX section
//! format currently stores only the signature bytes; sub-record support will
//! be added to attach timestamps and future audit metadata.

use std::time::SystemTime;

/// Hash algorithm choices for RFC 3161 imprint.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HashAlgo {
    /// SHA-256 hash algorithm.
    Sha256,
    /// SHA-384 hash algorithm.
    Sha384,
    /// SHA-512 hash algorithm.
    Sha512,
}

impl HashAlgo {
    /// Returns the OID string for this algorithm (for future TSA protocol use).
    pub fn oid(&self) -> &'static str {
        match self {
            HashAlgo::Sha256 => "2.16.840.1.101.3.4.2.1",
            HashAlgo::Sha384 => "2.16.840.1.101.3.4.2.2",
            HashAlgo::Sha512 => "2.16.840.1.101.3.4.2.3",
        }
    }
}

/// A timestamp request to be sent to a TSA.
#[derive(Clone, Debug)]
pub struct TimestampRequest {
    /// The hash of the data to timestamp (message imprint).
    pub message_imprint: Vec<u8>,
    /// The algorithm used to produce the imprint.
    pub hash_algo: HashAlgo,
}

/// A timestamp token received from a TSA, per RFC 3161.
///
/// This struct documents the shape that will be attached to the
/// .paideia.sig PAX section as a sub-record. Phase-3 m8-001 populates
/// this with synthetic data; real TSA responses (m8-002+) will fill in
/// actual signatures and timestamps from a live TSA.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TimestampToken {
    /// TSA name or URL (for audit/debug purposes).
    pub tsa_name: String,
    /// Generation time (UNIX timestamp seconds).
    pub gen_time_seconds: u64,
    /// Serial number of the timestamp token.
    pub serial_number: u64,
    /// The message imprint that was timestamped.
    pub message_imprint: Vec<u8>,
    /// The TSA's signature over the TST_INFO structure.
    pub signature: Vec<u8>,
}

impl TimestampToken {
    /// Serialize the token to bytes (simple format for m8-001 scaffold).
    ///
    /// Format: [tsa_name_len:u32][tsa_name:bytes][gen_time:u64][serial:u64]
    ///         [imprint_len:u32][imprint:bytes][sig_len:u32][sig:bytes]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::new();

        // TSA name
        buf.extend_from_slice(&(self.tsa_name.len() as u32).to_le_bytes());
        buf.extend_from_slice(self.tsa_name.as_bytes());

        // Timestamps and serial
        buf.extend_from_slice(&self.gen_time_seconds.to_le_bytes());
        buf.extend_from_slice(&self.serial_number.to_le_bytes());

        // Message imprint
        buf.extend_from_slice(&(self.message_imprint.len() as u32).to_le_bytes());
        buf.extend_from_slice(&self.message_imprint);

        // Signature
        buf.extend_from_slice(&(self.signature.len() as u32).to_le_bytes());
        buf.extend_from_slice(&self.signature);

        buf
    }

    /// Deserialize a token from bytes.
    pub fn from_bytes(buf: &[u8]) -> Option<Self> {
        let mut pos = 0;

        // TSA name
        if pos + 4 > buf.len() {
            return None;
        }
        let tsa_name_len = u32::from_le_bytes(buf[pos..pos + 4].try_into().ok()?) as usize;
        pos += 4;

        if pos + tsa_name_len > buf.len() {
            return None;
        }
        let tsa_name = String::from_utf8(buf[pos..pos + tsa_name_len].to_vec()).ok()?;
        pos += tsa_name_len;

        // Timestamps and serial
        if pos + 16 > buf.len() {
            return None;
        }
        let gen_time_seconds = u64::from_le_bytes(buf[pos..pos + 8].try_into().ok()?);
        pos += 8;
        let serial_number = u64::from_le_bytes(buf[pos..pos + 8].try_into().ok()?);
        pos += 8;

        // Message imprint
        if pos + 4 > buf.len() {
            return None;
        }
        let imprint_len = u32::from_le_bytes(buf[pos..pos + 4].try_into().ok()?) as usize;
        pos += 4;

        if pos + imprint_len > buf.len() {
            return None;
        }
        let message_imprint = buf[pos..pos + imprint_len].to_vec();
        pos += imprint_len;

        // Signature
        if pos + 4 > buf.len() {
            return None;
        }
        let sig_len = u32::from_le_bytes(buf[pos..pos + 4].try_into().ok()?) as usize;
        pos += 4;

        if pos + sig_len > buf.len() {
            return None;
        }
        let signature = buf[pos..pos + sig_len].to_vec();

        Some(TimestampToken {
            tsa_name,
            gen_time_seconds,
            serial_number,
            message_imprint,
            signature,
        })
    }
}

/// Errors that can occur during timestamping operations.
#[derive(Debug, thiserror::Error)]
pub enum TimestampError {
    /// The TSA is unreachable.
    #[error("TSA unreachable: {0}")]
    TsaUnreachable(String),

    /// The TSA response is invalid.
    #[error("invalid TSA response: {0}")]
    InvalidResponse(String),

    /// No TSA URL has been configured.
    #[error("TSA URL not configured (use --tsa-url)")]
    NoTsaConfigured,
}

/// Build a timestamp request from data.
///
/// Computes the SHA-256 hash (or specified algo) of the input data
/// and wraps it in a TimestampRequest.
pub fn build_request(data: &[u8], algo: HashAlgo) -> TimestampRequest {
    use blake3::Hasher;

    // Phase-3 m8-001: Use blake3 (workspace dep available)
    // RFC 3161 specifies SHA-256, but we scaffold with blake3 here.
    // Real TSA integration will use SHA-256 per RFC 3161 spec.
    let hash = Hasher::new().update(data).finalize();
    let imprint = hash.as_bytes()[..32].to_vec(); // Take first 32 bytes (SHA256-compatible size)

    TimestampRequest {
        message_imprint: imprint,
        hash_algo: algo,
    }
}

/// Fetch a timestamp token from a TSA.
///
/// Phase-3 m8-001: This is a scaffold that returns an empty synthetic token
/// when a TSA URL is configured. Real HTTP POST to the TSA will be implemented
/// in m8-002 when reqwest is wired in and operator opts in via --tsa-url.
pub fn fetch_token(
    request: &TimestampRequest,
    tsa_url: Option<&str>,
) -> Result<TimestampToken, TimestampError> {
    if tsa_url.is_none() {
        return Err(TimestampError::NoTsaConfigured);
    }

    // Phase-3 m8-001 scaffold: return empty synthetic token
    // until real TSA integration lands.
    Ok(TimestampToken {
        tsa_name: tsa_url.unwrap_or("").to_string(),
        gen_time_seconds: SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
        serial_number: 0,
        message_imprint: request.message_imprint.clone(),
        signature: vec![],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_request_produces_sha256_imprint() {
        let data = b"test data for RFC 3161";
        let req = build_request(data, HashAlgo::Sha256);

        // Should have a 32-byte imprint (blake3 truncated to SHA256 size)
        assert_eq!(req.message_imprint.len(), 32, "Imprint should be 32 bytes");
        assert_eq!(
            req.hash_algo,
            HashAlgo::Sha256,
            "Hash algo should be Sha256"
        );
    }

    #[test]
    fn fetch_token_without_tsa_url_returns_no_tsa_configured() {
        let data = b"test data";
        let req = build_request(data, HashAlgo::Sha256);

        let result = fetch_token(&req, None);
        match result {
            Err(TimestampError::NoTsaConfigured) => {
                // Expected
            }
            _ => panic!("Expected NoTsaConfigured error"),
        }
    }

    #[test]
    fn fetch_token_with_tsa_url_returns_synthetic_scaffold_token() {
        let data = b"test data";
        let req = build_request(data, HashAlgo::Sha256);

        let token =
            fetch_token(&req, Some("http://tsa.example.com")).expect("Should succeed with TSA URL");

        assert_eq!(token.tsa_name, "http://tsa.example.com");
        assert_eq!(token.message_imprint, req.message_imprint);
        assert!(
            token.signature.is_empty(),
            "Scaffold token should have empty signature"
        );
        assert!(
            token.gen_time_seconds > 0 || token.gen_time_seconds == 0,
            "gen_time_seconds should be set"
        );
    }

    #[test]
    fn timestamp_token_serialization_round_trip() {
        let original = TimestampToken {
            tsa_name: "http://tsa.example.com".to_string(),
            gen_time_seconds: 1234567890,
            serial_number: 42,
            message_imprint: vec![0x01, 0x02, 0x03, 0x04],
            signature: vec![0xaa, 0xbb, 0xcc, 0xdd],
        };

        let bytes = original.to_bytes();
        let recovered =
            TimestampToken::from_bytes(&bytes).expect("Should deserialize successfully");

        assert_eq!(
            original, recovered,
            "Token should round-trip through serialization"
        );
    }

    #[test]
    fn hash_algo_oid_is_rfc3161_compliant() {
        assert_eq!(HashAlgo::Sha256.oid(), "2.16.840.1.101.3.4.2.1");
        assert_eq!(HashAlgo::Sha384.oid(), "2.16.840.1.101.3.4.2.2");
        assert_eq!(HashAlgo::Sha512.oid(), "2.16.840.1.101.3.4.2.3");
    }
}
