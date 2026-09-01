//! SHA-512 per FIPS 180-4 §6.4 — portable u64 implementation.
//!
//! Mirrors the shape of [`crate::hash::sha256`] but with 512-bit state,
//! 128-byte blocks, 80 rounds, and 64-bit words. Introduced in #1341 for
//! Ed25519 (RFC 8032 §5.1 depends on SHA-512).
//!
//! # References
//!
//! - FIPS 180-4 §6.4: SHA-512 algorithm and message schedule.
//! - FIPS 180-4 §5.3.5: initial hash H(0) constants for SHA-512.
//! - FIPS 180-4 §4.2.3: K round constants (first 64 bits of the fractional
//!   parts of the cube roots of the first 80 primes).
//! - FIPS 180-4 §5.1.2: SHA-512 padding — append 0x80, zero-pad to
//!   (len mod 128) == 112, then append the 128-bit big-endian message
//!   length in bits (we only support len < 2^64 bits so the upper 64
//!   bits of the length are always zero).

/// SHA-512 initial hash value H(0) (FIPS 180-4 §5.3.5).
const H0: [u64; 8] = [
    0x6a09e667f3bcc908,
    0xbb67ae8584caa73b,
    0x3c6ef372fe94f82b,
    0xa54ff53a5f1d36f1,
    0x510e527fade682d1,
    0x9b05688c2b3e6c1f,
    0x1f83d9abfb41bd6b,
    0x5be0cd19137e2179,
];

/// SHA-512 round constants K (FIPS 180-4 §4.2.3).
const K: [u64; 80] = [
    0x428a2f98d728ae22, 0x7137449123ef65cd, 0xb5c0fbcfec4d3b2f, 0xe9b5dba58189dbbc,
    0x3956c25bf348b538, 0x59f111f1b605d019, 0x923f82a4af194f9b, 0xab1c5ed5da6d8118,
    0xd807aa98a3030242, 0x12835b0145706fbe, 0x243185be4ee4b28c, 0x550c7dc3d5ffb4e2,
    0x72be5d74f27b896f, 0x80deb1fe3b1696b1, 0x9bdc06a725c71235, 0xc19bf174cf692694,
    0xe49b69c19ef14ad2, 0xefbe4786384f25e3, 0x0fc19dc68b8cd5b5, 0x240ca1cc77ac9c65,
    0x2de92c6f592b0275, 0x4a7484aa6ea6e483, 0x5cb0a9dcbd41fbd4, 0x76f988da831153b5,
    0x983e5152ee66dfab, 0xa831c66d2db43210, 0xb00327c898fb213f, 0xbf597fc7beef0ee4,
    0xc6e00bf33da88fc2, 0xd5a79147930aa725, 0x06ca6351e003826f, 0x142929670a0e6e70,
    0x27b70a8546d22ffc, 0x2e1b21385c26c926, 0x4d2c6dfc5ac42aed, 0x53380d139d95b3df,
    0x650a73548baf63de, 0x766a0abb3c77b2a8, 0x81c2c92e47edaee6, 0x92722c851482353b,
    0xa2bfe8a14cf10364, 0xa81a664bbc423001, 0xc24b8b70d0f89791, 0xc76c51a30654be30,
    0xd192e819d6ef5218, 0xd69906245565a910, 0xf40e35855771202a, 0x106aa07032bbd1b8,
    0x19a4c116b8d2d0c8, 0x1e376c085141ab53, 0x2748774cdf8eeb99, 0x34b0bcb5e19b48a8,
    0x391c0cb3c5c95a63, 0x4ed8aa4ae3418acb, 0x5b9cca4f7763e373, 0x682e6ff3d6b2b8a3,
    0x748f82ee5defb2fc, 0x78a5636f43172f60, 0x84c87814a1f0ab72, 0x8cc702081a6439ec,
    0x90befffa23631e28, 0xa4506cebde82bde9, 0xbef9a3f7b2c67915, 0xc67178f2e372532b,
    0xca273eceea26619c, 0xd186b8c721c0c207, 0xeada7dd6cde0eb1e, 0xf57d4f7fee6ed178,
    0x06f067aa72176fba, 0x0a637dc5a2c898a6, 0x113f9804bef90dae, 0x1b710b35131c471b,
    0x28db77f523047d84, 0x32caab7b40c72493, 0x3c9ebe0a15c9bebc, 0x431d67c49c100d4c,
    0x4cc5d4becb3e42b6, 0x597f299cfc657e2a, 0x5fcb6fab3ad6faec, 0x6c44198c4a475817,
];

/// Streaming SHA-512 context.
#[derive(Clone)]
pub struct Sha512Ctx {
    state: [u64; 8],
    /// Total message length in bytes (we do not support > 2^64-1 bytes).
    len: u64,
    buf: [u8; 128],
    buf_len: usize,
}

impl Default for Sha512Ctx {
    fn default() -> Self {
        Self::new()
    }
}

impl Sha512Ctx {
    /// Construct a fresh context initialised with FIPS 180-4 §5.3.5 H(0).
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: H0,
            len: 0,
            buf: [0u8; 128],
            buf_len: 0,
        }
    }

    /// Absorb `data` into the running hash.
    pub fn update(&mut self, data: &[u8]) {
        self.len = self.len.wrapping_add(data.len() as u64);
        let mut i = 0;

        if self.buf_len > 0 {
            let take = (128 - self.buf_len).min(data.len());
            self.buf[self.buf_len..self.buf_len + take].copy_from_slice(&data[..take]);
            self.buf_len += take;
            i += take;
            if self.buf_len == 128 {
                compress(&mut self.state, &self.buf);
                self.buf_len = 0;
            }
        }

        while i + 128 <= data.len() {
            let mut block = [0u8; 128];
            block.copy_from_slice(&data[i..i + 128]);
            compress(&mut self.state, &block);
            i += 128;
        }

        let tail = data.len() - i;
        if tail > 0 {
            self.buf[..tail].copy_from_slice(&data[i..]);
            self.buf_len = tail;
        }
    }

    /// Apply FIPS 180-4 §5.1.2 padding and emit the 64-byte digest.
    #[must_use]
    pub fn finalize(mut self) -> [u8; 64] {
        let bit_len = self.len.wrapping_mul(8);

        self.buf[self.buf_len] = 0x80;
        self.buf_len += 1;

        // SHA-512 length field is 128 bits at offset 112..128; the upper 64
        // bits are always zero for messages < 2^64 bytes, which is all we
        // support. Need 16 tail bytes for the length; if the current block
        // has < 16 bytes free, zero-pad + flush + start a fresh block.
        if self.buf_len > 112 {
            for byte in self.buf.iter_mut().skip(self.buf_len) {
                *byte = 0;
            }
            compress(&mut self.state, &self.buf);
            self.buf_len = 0;
        }

        for byte in self.buf.iter_mut().take(112).skip(self.buf_len) {
            *byte = 0;
        }
        // Upper 64 bits of the 128-bit length are always zero (see above).
        for byte in self.buf.iter_mut().take(120).skip(112) {
            *byte = 0;
        }
        self.buf[120..128].copy_from_slice(&bit_len.to_be_bytes());
        compress(&mut self.state, &self.buf);

        let mut out = [0u8; 64];
        for (i, word) in self.state.iter().enumerate() {
            out[i * 8..(i + 1) * 8].copy_from_slice(&word.to_be_bytes());
        }
        out
    }
}

/// One-shot SHA-512 of `msg`.
#[must_use]
pub fn sha512(msg: &[u8]) -> [u8; 64] {
    let mut ctx = Sha512Ctx::new();
    ctx.update(msg);
    ctx.finalize()
}

/// SHA-512 compression function on one 128-byte block (FIPS 180-4 §6.4.2).
fn compress(state: &mut [u64; 8], block: &[u8; 128]) {
    // Parse block into 16 64-bit big-endian words.
    let mut w = [0u64; 80];
    for i in 0..16 {
        w[i] = u64::from_be_bytes([
            block[i * 8],
            block[i * 8 + 1],
            block[i * 8 + 2],
            block[i * 8 + 3],
            block[i * 8 + 4],
            block[i * 8 + 5],
            block[i * 8 + 6],
            block[i * 8 + 7],
        ]);
    }

    // Extend into the 80-word message schedule.
    for i in 16..80 {
        let s0 = w[i - 15].rotate_right(1) ^ w[i - 15].rotate_right(8) ^ (w[i - 15] >> 7);
        let s1 = w[i - 2].rotate_right(19) ^ w[i - 2].rotate_right(61) ^ (w[i - 2] >> 6);
        w[i] = w[i - 16]
            .wrapping_add(s0)
            .wrapping_add(w[i - 7])
            .wrapping_add(s1);
    }

    let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = *state;

    for i in 0..80 {
        let s1 = e.rotate_right(14) ^ e.rotate_right(18) ^ e.rotate_right(41);
        let ch = (e & f) ^ ((!e) & g);
        let t1 = h
            .wrapping_add(s1)
            .wrapping_add(ch)
            .wrapping_add(K[i])
            .wrapping_add(w[i]);
        let s0 = a.rotate_right(28) ^ a.rotate_right(34) ^ a.rotate_right(39);
        let maj = (a & b) ^ (a & c) ^ (b & c);
        let t2 = s0.wrapping_add(maj);

        h = g;
        g = f;
        f = e;
        e = d.wrapping_add(t1);
        d = c;
        c = b;
        b = a;
        a = t1.wrapping_add(t2);
    }

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

    /// FIPS 180-4 §D.4 "abc" — 3-byte one-block input.
    #[test]
    fn sha512_fips_d4_abc() {
        let got = sha512(b"abc");
        assert_eq!(
            hex(&got),
            "ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a\
             2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f"
        );
    }

    /// FIPS 180-4 §D.5 — 112-byte two-block input.
    #[test]
    fn sha512_fips_d5_two_block() {
        let msg = b"abcdefghbcdefghicdefghijdefghijkefghijklfghijklmghijklmnhijklmno\
                    ijklmnopjklmnopqklmnopqrlmnopqrsmnopqrstnopqrstu";
        let got = sha512(msg);
        assert_eq!(
            hex(&got),
            "8e959b75dae313da8cf4f72814fc143f8f7779c6eb9f7fa17299aeadb6889018\
             501d289e4900f7e4331b99dec4b5433ac7d329eeb6dd26545e96e55b874be909"
        );
    }

    #[test]
    fn sha512_empty() {
        let got = sha512(b"");
        // NIST empty-string vector.
        assert_eq!(
            hex(&got),
            "cf83e1357eefb8bdf1542850d66d8007d620e4050b5715dc83f4a921d36ce9ce\
             47d0d13c5d85f2b0ff8318d2877eec2f63b931bd47417a81a538327af927da3e"
        );
    }

    #[test]
    fn sha512_streaming_equivalence() {
        let msg: Vec<u8> = (0u8..=200u8).collect();
        let one_shot = sha512(&msg);

        for chunk in &[1usize, 32, 55, 63, 64, 65, 111, 112, 113, 127, 128, 129, 200] {
            let mut ctx = Sha512Ctx::new();
            for slice in msg.chunks(*chunk) {
                ctx.update(slice);
            }
            let got = ctx.finalize();
            assert_eq!(got, one_shot, "chunk size {} diverges", chunk);
        }
    }

    /// FIPS 180-4 §D.6 — one-million 'a' bytes. Marked #[ignore] to keep
    /// the default suite fast; run with `--ignored` for the full check.
    #[test]
    #[ignore]
    fn sha512_fips_d6_one_million_a() {
        let mut ctx = Sha512Ctx::new();
        let block = [b'a'; 1000];
        for _ in 0..1000 {
            ctx.update(&block);
        }
        let got = ctx.finalize();
        assert_eq!(
            hex(&got),
            "e718483d0ce769644e2e42c7bc15b4638e1f98b13b2044285632a803afa973eb\
             de0ff244877ea60a4cb0432ce577c31beb009c5c2c49aa2e4eadb217ad8cc09b"
        );
    }
}
