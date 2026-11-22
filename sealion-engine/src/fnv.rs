pub(crate) const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
pub(crate) const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

pub(crate) fn fnv1a(hash: &mut u64, bytes: &[u8]) {
    for &byte in bytes {
        *hash ^= u64::from(byte);
        *hash = hash.wrapping_mul(FNV_PRIME);
    }
}

pub(crate) fn source_checksum(source: &str) -> u64 {
    let mut hash = FNV_OFFSET;
    fnv1a(&mut hash, source.as_bytes());
    hash
}
