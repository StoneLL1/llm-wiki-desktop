//! Bounded-memory artifact IO shared by validation and transactional publication.
use sha2::{Digest, Sha256};
use std::io::{self, Read};

pub(super) fn hash_reader(reader: &mut impl Read) -> io::Result<(String, u64)> {
    let mut digest = Sha256::new();
    let mut size = 0;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
        size += count as u64;
    }
    Ok((format!("{:x}", digest.finalize()), size))
}

/// Verifies the bytes actually copied, including changes after preview. An
/// error at EOF prevents the transaction from publishing its temporary file.
pub(super) struct VerifiedReader<'a, R> {
    inner: &'a mut R,
    expected_hash: &'a str,
    expected_size: u64,
    size: u64,
    digest: Sha256,
}

impl<'a, R: Read> VerifiedReader<'a, R> {
    pub fn new(inner: &'a mut R, expected_hash: &'a str, expected_size: u64) -> Self {
        Self {
            inner,
            expected_hash,
            expected_size,
            size: 0,
            digest: Sha256::new(),
        }
    }
}

impl<R: Read> Read for VerifiedReader<'_, R> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if buffer.is_empty() {
            return Ok(0);
        }
        let count = self.inner.read(buffer)?;
        self.size += count as u64;
        self.digest.update(&buffer[..count]);
        if self.size > self.expected_size
            || (count == 0
                && (self.size != self.expected_size
                    || format!("{:x}", self.digest.clone().finalize()) != self.expected_hash))
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Import artifact changed after preview",
            ));
        }
        Ok(count)
    }
}
