//! SHA-256 per FIPS 180-4 §6.2 — portable u32 implementation.
//!
//! # References
//!
//! - FIPS 180-4 §6.2: SHA-256 algorithm and message schedule.
//! - FIPS 180-4 §5.3.3: initial hash H(0) constants.
//! - FIPS 180-4 §4.2.2: K round constants (first 32 bits of the fractional
//!   parts of the cube roots of the first 64 primes).
//! - FIPS 180-4 §5.1.1: padding — append 0x80, zero-pad to (len mod 64) == 56,
//!   then append the 64-bit big-endian message length in bits.
//!
//! # Constant-time notes
//!
//! SHA-256 is a Merkle-Damgard construction and is length-attackable by
//! design: the padding embeds the message length, so "constant-time" here
//! only applies to the compression function's data dependence on secret
//! inputs. The u32 add/xor/rotate ops used in the schedule and round
//! function have no secret-dependent branches and no secret-dependent
//! table lookups (K and H(0) are compile-time constants), so the
//! compression function is constant-time with respect to secret data.
//!
//! # Streaming vs one-shot
//!
//! [`sha256`] is the one-shot convenience. [`Sha256Ctx`] is the streaming
//! form used by the HMAC-SHA256 / HKDF composition in `crate::kdf::hkdf`.
//! Both share the same compression core.

/// SHA-256 initial hash value H(0) (FIPS 180-4 §5.3.3).
///
/// First 32 bits of the fractional parts of the square roots of the first
/// eight primes (2, 3, 5, 7, 11, 13, 17, 19).
const H0: [u32; 8] = [
    0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
];

/// SHA-256 round constants K (FIPS 180-4 §4.2.2).
///
/// First 32 bits of the fractional parts of the cube roots of the first
/// 64 primes.
const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

/// Streaming SHA-256 context.
///
/// Feed message bytes with [`Sha256Ctx::update`] (any chunk boundaries) and
/// call [`Sha256Ctx::finalize`] to produce the 32-byte digest. Consuming
/// `self` on finalize prevents accidental re-use of a finalized context.
///
/// This form is what [`crate::kdf::hkdf::hmac_sha256`] uses to build the
/// two-pass HMAC without ever allocating a temporary message buffer.
#[derive(Clone)]
pub struct Sha256Ctx {
    /// Running hash state (H(0) .. H(N)).
    state: [u32; 8],
    /// Total message length in *bytes* processed so far (all blocks).
    /// FIPS 180-4 §5.1.1 embeds it as bits (`len * 8`) at finalize.
    len: u64,
    /// Partial block buffer for un-flushed tail bytes.
    buf: [u8; 64],
    /// Bytes currently populated in `buf` (0..64).
    buf_len: usize,
}

impl Default for Sha256Ctx {
    fn default() -> Self {
        Self::new()
    }
}

impl Sha256Ctx {
    /// Construct a fresh context initialised with FIPS 180-4 §5.3.3 H(0).
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: H0,
            len: 0,
            buf: [0u8; 64],
            buf_len: 0,
        }
    }

    /// Absorb `data` into the running hash.
    pub fn update(&mut self, data: &[u8]) {
        self.len = self.len.wrapping_add(data.len() as u64);
        let mut i = 0;

        // If there are stale bytes in the buffer, top it off first.
        if self.buf_len > 0 {
            let take = (64 - self.buf_len).min(data.len());
            self.buf[self.buf_len..self.buf_len + take].copy_from_slice(&data[..take]);
            self.buf_len += take;
            i += take;
            if self.buf_len == 64 {
                compress(&mut self.state, &self.buf);
                self.buf_len = 0;
            }
        }

        // Process full 64-byte blocks directly from the input.
        while i + 64 <= data.len() {
            let mut block = [0u8; 64];
            block.copy_from_slice(&data[i..i + 64]);
            compress(&mut self.state, &block);
            i += 64;
        }

        // Stash any remaining tail in the buffer.
        let tail = data.len() - i;
        if tail > 0 {
            self.buf[..tail].copy_from_slice(&data[i..]);
            self.buf_len = tail;
        }
    }

    /// Apply FIPS 180-4 §5.1.1 padding and emit the 32-byte digest.
    ///
    /// The context is consumed to prevent re-use of a finalized state.
    #[must_use]
    pub fn finalize(mut self) -> [u8; 32] {
        // Bit length of the message (FIPS 180-4 §5.1.1) — must be embedded as
        // a 64-bit big-endian integer in the last 8 bytes of the padded
        // message.
        let bit_len = self.len.wrapping_mul(8);

        // Append the mandatory 0x80 byte.
        self.buf[self.buf_len] = 0x80;
        self.buf_len += 1;

        // If there isn't room for the 8-byte length in the current block,
        // zero-pad, flush, and start a fresh block for the length.
        if self.buf_len > 56 {
            for byte in self.buf.iter_mut().skip(self.buf_len) {
                *byte = 0;
            }
            compress(&mut self.state, &self.buf);
            self.buf_len = 0;
        }

        // Zero-pad up to byte 56, then embed the 64-bit big-endian bit length.
        for byte in self.buf.iter_mut().take(56).skip(self.buf_len) {
            *byte = 0;
        }
        self.buf[56..64].copy_from_slice(&bit_len.to_be_bytes());
        compress(&mut self.state, &self.buf);

        // Emit the state as 8 big-endian u32s → 32 bytes.
        let mut out = [0u8; 32];
        for (i, word) in self.state.iter().enumerate() {
            out[i * 4..(i + 1) * 4].copy_from_slice(&word.to_be_bytes());
        }
        out
    }
}

/// One-shot SHA-256 of `msg`. Convenience over [`Sha256Ctx`].
#[must_use]
pub fn sha256(msg: &[u8]) -> [u8; 32] {
    let mut ctx = Sha256Ctx::new();
    ctx.update(msg);
    ctx.finalize()
}

/// SHA-256 compression function on one 64-byte block.
///
/// Message schedule + 64-round main loop per FIPS 180-4 §6.2.2. Operates
/// entirely on `u32` with no secret-dependent branches or table lookups.
fn compress(state: &mut [u32; 8], block: &[u8; 64]) {
    // Step 1 (FIPS 180-4 §6.2.2): parse block into 16 32-bit big-endian words.
    let mut w = [0u32; 64];
    for i in 0..16 {
        w[i] = u32::from_be_bytes([
            block[i * 4],
            block[i * 4 + 1],
            block[i * 4 + 2],
            block[i * 4 + 3],
        ]);
    }

    // Step 2 (§6.2.2): extend into the 64-word message schedule.
    for i in 16..64 {
        let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
        let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
        w[i] = w[i - 16]
            .wrapping_add(s0)
            .wrapping_add(w[i - 7])
            .wrapping_add(s1);
    }

    // Step 3 (§6.2.2): initialise working variables from H.
    let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = *state;

    // Step 4 (§6.2.2): main 64-round loop.
    for i in 0..64 {
        let sigma1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
        let ch = (e & f) ^ ((!e) & g);
        let t1 = h
            .wrapping_add(sigma1)
            .wrapping_add(ch)
            .wrapping_add(K[i])
            .wrapping_add(w[i]);
        let sigma0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
        let maj = (a & b) ^ (a & c) ^ (b & c);
        let t2 = sigma0.wrapping_add(maj);

        h = g;
        g = f;
        f = e;
        e = d.wrapping_add(t1);
        d = c;
        c = b;
        b = a;
        a = t1.wrapping_add(t2);
    }

    // Step 5 (§6.2.2): compute the i'th intermediate hash value.
    state[0] = state[0].wrapping_add(a);
    state[1] = state[1].wrapping_add(b);
    state[2] = state[2].wrapping_add(c);
    state[3] = state[3].wrapping_add(d);
    state[4] = state[4].wrapping_add(e);
    state[5] = state[5].wrapping_add(f);
    state[6] = state[6].wrapping_add(g);
    state[7] = state[7].wrapping_add(h);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(bytes: &[u8]) -> String {
        let mut s = String::with_capacity(bytes.len() * 2);
        for b in bytes {
            s.push_str(&format!("{:02x}", b));
        }
        s
    }

    // -------- FIPS 180-4 §D.1 --------
    //
    // Msg: "abc"
    // Digest: ba7816bf 8f01cfea 414140de 5dae2223
    //         b00361a3 96177a9c b410ff61 f20015ad
    #[test]
    fn fips_180_4_appendix_d_1_abc() {
        let d = sha256(b"abc");
        assert_eq!(
            hex(&d),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    // -------- FIPS 180-4 §D.2 --------
    //
    // Msg: "abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq" (448-bit,
    // spanning two 512-bit blocks after padding).
    // Digest: 248d6a61 d20638b8 e5c02693 0c3e6039
    //         a33ce459 64ff2167 f6ecedd4 19db06c1
    #[test]
    fn fips_180_4_appendix_d_2_two_blocks() {
        let d = sha256(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq");
        assert_eq!(
            hex(&d),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
    }

    // -------- FIPS 180-4 §D.3 --------
    //
    // Msg: one million 'a's.
    // Digest: cdc76e5c 9914fb92 81a1c7e2 84d73e67
    //         f1809a48 a497200e 046d39cc c7112cd0
    //
    // Streamed to prove the update()/finalize() chunking is robust across
    // non-block-aligned boundaries.
    #[test]
    fn fips_180_4_appendix_d_3_one_million_a() {
        let mut ctx = Sha256Ctx::new();
        // Odd chunk size that straddles block boundaries repeatedly.
        let chunk = [b'a'; 997];
        let iters = 1_000_000 / 997;
        let tail = 1_000_000 - iters * 997;
        for _ in 0..iters {
            ctx.update(&chunk);
        }
        ctx.update(&chunk[..tail]);
        assert_eq!(
            hex(&ctx.finalize()),
            "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
        );
    }

    // -------- Empty input --------
    //
    // The empty-string digest is a widely-cited pin (RFC 6234 §8.5, various
    // NIST test suites); catches accidental "assume ≥1 block" bugs in the
    // padding path.
    #[test]
    fn empty_input() {
        let d = sha256(b"");
        assert_eq!(
            hex(&d),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    // -------- Streaming equivalence --------
    //
    // Prove chunk boundaries don't leak into the digest. Feed a fixed 200-byte
    // message via chunk sizes that straddle every position around the 64-byte
    // block boundary (63, 64, 65) and confirm all match the one-shot digest.
    #[test]
    fn streaming_equivalence_across_block_boundary() {
        let mut msg = [0u8; 200];
        for (i, b) in msg.iter_mut().enumerate() {
            *b = i as u8;
        }
        let one_shot = sha256(&msg);

        for chunk_size in [1, 63, 64, 65, 128, 199, 200] {
            let mut ctx = Sha256Ctx::new();
            for chunk in msg.chunks(chunk_size) {
                ctx.update(chunk);
            }
            assert_eq!(
                ctx.finalize(),
                one_shot,
                "streaming digest with chunk_size={} diverged from one-shot",
                chunk_size
            );
        }
    }

    // -------- Padding-boundary cases (55 / 56 / 57 / 63 / 64 / 65 bytes) --------
    //
    // The padding path branches at buf_len > 56 (i.e., 55-byte inputs fit the
    // length in the same block after the 0x80; 56-byte inputs push the length
    // into a second block). Rather than pin made-up hex values, prove:
    //   1. Every boundary length streams identically to a one-shot digest
    //      through a variety of chunk sizes.
    //   2. Adjacent lengths produce distinct digests (i.e., the padding path
    //      isn't accidentally identifying length ±1).
    #[test]
    fn padding_boundary_streaming_and_distinctness() {
        let lengths = [55usize, 56, 57, 63, 64, 65];
        let mut digests: Vec<[u8; 32]> = Vec::new();

        for &n in &lengths {
            let msg = vec![b'a'; n];
            let one_shot = sha256(&msg);

            // Chunk the same message through several sizes that straddle
            // the 64-byte block boundary and confirm identical output.
            for chunk_size in [1, 32, 55, 56, 57, 63, 64, 65] {
                let mut ctx = Sha256Ctx::new();
                for chunk in msg.chunks(chunk_size) {
                    ctx.update(chunk);
                }
                assert_eq!(
                    ctx.finalize(),
                    one_shot,
                    "n={} chunk={} streaming diverged from one-shot",
                    n,
                    chunk_size
                );
            }

            digests.push(one_shot);
        }

        // Every input length must produce a distinct digest.
        for i in 0..digests.len() {
            for j in (i + 1)..digests.len() {
                assert_ne!(
                    digests[i], digests[j],
                    "sha256({} 'a's) collided with sha256({} 'a's)",
                    lengths[i], lengths[j]
                );
            }
        }
    }

    // -------- Widely-cited additional vector --------
    //
    // "The quick brown fox jumps over the lazy dog" — 43 bytes, single block
    // after padding. Pinned as a second single-block vector alongside "abc"
    // (D.1) so a regression that only touches the round-loop's tail — but
    // still passes the 3-byte D.1 vector — surfaces here.
    // Widely-cited single-block vector — see e.g. the SHA-2 Wikipedia entry
    // or `sha256sum` on any Unix. Pinned as a second single-block vector
    // alongside the 3-byte "abc" (D.1) so a regression that touches the
    // round-loop's tail — but still passes the 3-byte D.1 vector — surfaces
    // here. The paired one-byte-edited input ("cog" vs "dog") smokes the
    // avalanche property.
    #[test]
    fn fox_reference_vector() {
        let d = sha256(b"The quick brown fox jumps over the lazy dog");
        assert_eq!(
            hex(&d),
            "d7a8fbb307d7809469ca9abcb0082e4f8d5651e46d3cdb762d02d0bf37c9e592"
        );
        let d2 = sha256(b"The quick brown fox jumps over the lazy cog");
        assert_eq!(
            hex(&d2),
            "e4c4d8f3bf76b692de791a173e05321150f7a345b46484fe427f6acc7ecc81be"
        );
        assert_ne!(d, d2, "avalanche broken: one-byte edit produced same digest");
    }
}
