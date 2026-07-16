use std::io::{self, Read};

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

    pub(crate) fn field_reader(&mut self, length: u64, mut reader: impl Read) -> io::Result<()> {
        self.hasher.update(&length.to_le_bytes());
        let mut remaining = length;
        let mut buffer = [0_u8; 16 * 1024];
        while remaining > 0 {
            let wanted = usize::try_from(remaining.min(buffer.len() as u64))
                .expect("fixed buffer length fits usize");
            let read = reader.read(&mut buffer[..wanted])?;
            if read == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "content changed while it was fingerprinted",
                ));
            }
            self.hasher.update(&buffer[..read]);
            remaining -= u64::try_from(read).expect("fixed buffer length fits u64");
        }
        let mut extra = [0_u8; 1];
        if reader.read(&mut extra)? != 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "content changed while it was fingerprinted",
            ));
        }
        Ok(())
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
