#[derive(Debug, Clone)]
pub struct Fnv1a(u64);

impl Fnv1a {
    pub fn new() -> Self {
        Self(0xcbf29ce484222325)
    }

    pub fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 ^= u64::from(*byte);
            self.0 = self.0.wrapping_mul(0x100000001b3);
        }
    }

    pub fn finish(&self) -> u64 {
        self.0
    }
}

impl Default for Fnv1a {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hash_of(bytes: &[u8]) -> u64 {
        let mut hasher = Fnv1a::new();
        hasher.write(bytes);
        hasher.finish()
    }

    #[test]
    fn the_hash_of_empty_input_is_the_fnv_offset_basis() {
        assert_eq!(hash_of(b""), 0xcbf29ce484222325);
    }

    #[test]
    fn known_inputs_hash_to_their_published_fnv1a_values() {
        assert_eq!(hash_of(b"a"), 0xaf63dc4c8601ec8c);
        assert_eq!(hash_of(b"foobar"), 0x85944171f73967e8);
    }

    #[test]
    fn writing_in_pieces_equals_writing_at_once() {
        let mut pieces = Fnv1a::new();
        pieces.write(b"foo");
        pieces.write(b"bar");
        assert_eq!(pieces.finish(), hash_of(b"foobar"));
    }
}
