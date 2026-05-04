//! Pure-Rust SHA-256 used as Andromeda's canonical digest backend.
//!
//! Andromeda needs a deterministic, collision-resistant fingerprint that can
//! be computed without runtime serialization, ad-hoc multiplicative mixers, or
//! external dependencies.  This module implements FIPS-180-4 SHA-256 in safe,
//! `forbid(unsafe_code)`-compatible Rust and exposes a [`Sha256`] streaming
//! hasher used by the catalog (`StableHashSink`, `ObjectShapeHashSink`,
//! `PolicyVersion` digests) and by the protocol layer (StructuredObject
//! descriptor hashes).  Centralising the digest backend guarantees that
//! contract hashes, object-shape hashes, descriptor hashes, and policy
//! versions all derive from the same proven one-way function.

const ROUND_CONSTANTS: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

const INITIAL_STATE: [u32; 8] = [
    0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
];

/// Streaming SHA-256 hasher.  Always emits a 32-byte digest from `finalize`.
#[derive(Clone)]
pub struct Sha256 {
    state: [u32; 8],
    buffer: [u8; 64],
    buffer_len: usize,
    length_bits: u64,
}

impl Sha256 {
    pub fn new() -> Self {
        Self {
            state: INITIAL_STATE,
            buffer: [0u8; 64],
            buffer_len: 0,
            length_bits: 0,
        }
    }

    pub fn update(&mut self, mut data: &[u8]) {
        // Wrapping is fine: SHA-256 specifies behavior for inputs up to 2^64-1
        // bits.  Andromeda digests never approach this limit.
        self.length_bits = self
            .length_bits
            .wrapping_add((data.len() as u64).wrapping_mul(8));

        if self.buffer_len > 0 {
            let take = (64 - self.buffer_len).min(data.len());
            self.buffer[self.buffer_len..self.buffer_len + take].copy_from_slice(&data[..take]);
            self.buffer_len += take;
            data = &data[take..];
            if self.buffer_len == 64 {
                let block = self.buffer;
                self.compress_block(&block);
                self.buffer_len = 0;
            }
        }

        while data.len() >= 64 {
            let mut block = [0u8; 64];
            block.copy_from_slice(&data[..64]);
            self.compress_block(&block);
            data = &data[64..];
        }

        if !data.is_empty() {
            self.buffer[..data.len()].copy_from_slice(data);
            self.buffer_len = data.len();
        }
    }

    pub fn finalize(mut self) -> [u8; 32] {
        let length_bits = self.length_bits;

        // Append the 0x80 byte.
        self.buffer[self.buffer_len] = 0x80;
        self.buffer_len += 1;

        // If we cannot fit the 8-byte length in this block, pad and flush.
        if self.buffer_len > 56 {
            for byte in &mut self.buffer[self.buffer_len..] {
                *byte = 0;
            }
            let block = self.buffer;
            self.compress_block(&block);
            self.buffer_len = 0;
        }

        for byte in &mut self.buffer[self.buffer_len..56] {
            *byte = 0;
        }
        self.buffer[56..64].copy_from_slice(&length_bits.to_be_bytes());
        let block = self.buffer;
        self.compress_block(&block);

        let mut output = [0u8; 32];
        for (word_index, word) in self.state.iter().enumerate() {
            output[word_index * 4..word_index * 4 + 4].copy_from_slice(&word.to_be_bytes());
        }
        output
    }

    fn compress_block(&mut self, block: &[u8; 64]) {
        let mut schedule = [0u32; 64];
        for (word_index, slot) in schedule.iter_mut().enumerate().take(16) {
            let offset = word_index * 4;
            *slot = u32::from_be_bytes([
                block[offset],
                block[offset + 1],
                block[offset + 2],
                block[offset + 3],
            ]);
        }

        for word_index in 16..64 {
            let prev15 = schedule[word_index - 15];
            let prev2 = schedule[word_index - 2];
            let s0 = prev15.rotate_right(7) ^ prev15.rotate_right(18) ^ (prev15 >> 3);
            let s1 = prev2.rotate_right(17) ^ prev2.rotate_right(19) ^ (prev2 >> 10);
            schedule[word_index] = schedule[word_index - 16]
                .wrapping_add(s0)
                .wrapping_add(schedule[word_index - 7])
                .wrapping_add(s1);
        }

        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = self.state;

        for round in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let choose = (e & f) ^ ((!e) & g);
            let temp1 = h
                .wrapping_add(s1)
                .wrapping_add(choose)
                .wrapping_add(ROUND_CONSTANTS[round])
                .wrapping_add(schedule[round]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let majority = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(majority);

            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        self.state[0] = self.state[0].wrapping_add(a);
        self.state[1] = self.state[1].wrapping_add(b);
        self.state[2] = self.state[2].wrapping_add(c);
        self.state[3] = self.state[3].wrapping_add(d);
        self.state[4] = self.state[4].wrapping_add(e);
        self.state[5] = self.state[5].wrapping_add(f);
        self.state[6] = self.state[6].wrapping_add(g);
        self.state[7] = self.state[7].wrapping_add(h);
    }
}

impl Default for Sha256 {
    fn default() -> Self {
        Self::new()
    }
}

/// Convenience helper: digest a single byte slice in one call.
pub fn sha256(bytes: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher.finalize()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(bytes: &[u8; 32]) -> String {
        let mut output = String::with_capacity(64);
        for byte in bytes {
            output.push_str(&format!("{byte:02x}"));
        }
        output
    }

    #[test]
    fn sha256_matches_fips_test_vector_empty() {
        assert_eq!(
            hex(&sha256(b"")),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn sha256_matches_fips_test_vector_abc() {
        assert_eq!(
            hex(&sha256(b"abc")),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn sha256_matches_fips_test_vector_two_block() {
        let message = b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq";
        assert_eq!(
            hex(&sha256(message)),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
    }

    #[test]
    fn sha256_streaming_matches_one_shot() {
        let message: Vec<u8> = (0u8..200).collect();
        let one_shot = sha256(&message);

        let mut streaming = Sha256::new();
        streaming.update(&message[..1]);
        streaming.update(&message[1..73]);
        streaming.update(&message[73..128]);
        streaming.update(&message[128..]);
        let streamed = streaming.finalize();

        assert_eq!(one_shot, streamed);
    }
}
