use blake3::Hasher;

pub(crate) struct CanonicalHasher {
    hasher: Hasher,
}

impl CanonicalHasher {
    pub(crate) fn new(context: &'static str) -> Self {
        Self {
            hasher: Hasher::new_derive_key(context),
        }
    }

    pub(crate) fn field(&mut self, bytes: impl AsRef<[u8]>) {
        let bytes = bytes.as_ref();
        self.hasher.update(&(bytes.len() as u64).to_le_bytes());
        self.hasher.update(bytes);
    }

    pub(crate) fn finish(self) -> [u8; 32] {
        *self.hasher.finalize().as_bytes()
    }
}

pub(crate) fn push_field(out: &mut Vec<u8>, bytes: impl AsRef<[u8]>) {
    let bytes = bytes.as_ref();
    out.extend_from_slice(&(bytes.len() as u64).to_le_bytes());
    out.extend_from_slice(bytes);
}
