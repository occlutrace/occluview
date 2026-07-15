use crate::{HpsError, ReadError};
use blowfish::cipher::{Block, BlockCipherDecrypt, KeyInit};
use blowfish::Blowfish;
use md5::{Digest, Md5};
use std::collections::BTreeMap;
use zeroize::{Zeroize, Zeroizing};

use super::key::{validate_key_len, HpsKeyProvider};

pub(super) fn derive_encryption_key<P: HpsKeyProvider + ?Sized>(
    provider: &P,
    properties: &BTreeMap<String, String>,
) -> Result<Zeroizing<Vec<u8>>, ReadError<P::Error>> {
    let base_key = provider
        .base_key()
        .map_err(ReadError::KeyProvider)?
        .ok_or(HpsError::KeyMissing)?;
    let mut key = Zeroizing::new(base_key.as_bytes().to_vec());
    let package_hash = compute_package_lock_hash(properties);

    match properties.get("EKID").filter(|value| !value.is_empty()) {
        None => {
            if let Some(package_hash) = package_hash {
                key.zeroize();
                *key = package_hash.into_bytes();
            }
        }
        Some(value) if value == "1" => {
            if let Some(package_hash) = package_hash {
                key.extend_from_slice(package_hash.as_bytes());
            }
        }
        Some(_) => {}
    }

    validate_key_len(key.len())?;
    Ok(key)
}

pub(super) fn decrypt_hps_data(
    encrypted: &[u8],
    key: &[u8],
    original_size: Option<usize>,
    use_scrambled_key: bool,
) -> Result<Zeroizing<Vec<u8>>, HpsError> {
    let effective_key = if use_scrambled_key {
        scramble_key(key)
    } else {
        Zeroizing::new(key.to_vec())
    };
    let mut decrypted = blowfish_ecb_decrypt(encrypted, &effective_key)?;
    if let Some(original_size) = original_size {
        if original_size < decrypted.len() {
            decrypted.truncate(original_size);
        }
    }
    Ok(decrypted)
}

pub(super) fn hps_adler32_check_value(bytes: &[u8]) -> u32 {
    adler2::adler32_slice(bytes).swap_bytes()
}

fn compute_package_lock_hash(properties: &BTreeMap<String, String>) -> Option<String> {
    let value = properties.get("PackageLockList")?;
    if value.is_empty() {
        return None;
    }
    let mut items: Vec<&str> = value.split(';').filter(|item| !item.is_empty()).collect();
    if items.is_empty() {
        return None;
    }
    items.sort_unstable();
    items.dedup();

    let mut canonical = String::new();
    for item in items {
        canonical.push_str(item);
        canonical.push(';');
    }

    let digest = Md5::digest(canonical.as_bytes());
    Some(to_upper_hex(&digest))
}

fn to_upper_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

fn scramble_key(key: &[u8]) -> Zeroizing<Vec<u8>> {
    let mut out = Zeroizing::new(Vec::with_capacity(key.len()));
    out.extend(key.iter().rev().map(|byte| byte ^ 123));
    out
}

fn blowfish_ecb_decrypt(encrypted: &[u8], key: &[u8]) -> Result<Zeroizing<Vec<u8>>, HpsError> {
    if encrypted.is_empty() {
        return Ok(Zeroizing::new(Vec::new()));
    }
    validate_key_len(key.len())?;

    let mut padded = Zeroizing::new(encrypted.to_vec());
    let remainder = padded.len() % 8;
    if remainder != 0 {
        let padded_len = padded.len() + (8 - remainder);
        padded.resize(padded_len, 0);
    }

    swap_32_bit_words_in_blocks(&mut padded);
    let cipher: Blowfish = Blowfish::new_from_slice(key).map_err(|_| HpsError::InvalidKey {
        reason: "CE encryption key must be 4..56 bytes for Blowfish".to_string(),
    })?;
    for chunk in padded.chunks_exact_mut(8) {
        let mut block = Block::<Blowfish>::default();
        block.copy_from_slice(chunk);
        cipher.decrypt_block(&mut block);
        chunk.copy_from_slice(&block);
        block.fill(0);
    }
    swap_32_bit_words_in_blocks(&mut padded);
    Ok(padded)
}

fn swap_32_bit_words_in_blocks(bytes: &mut [u8]) {
    for block in bytes.chunks_exact_mut(8) {
        block[..4].reverse();
        block[4..8].reverse();
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;
    use crate::key::{HpsSecretKey, NoHpsKeyProvider};

    struct StaticProvider(Vec<u8>);

    impl HpsKeyProvider for StaticProvider {
        type Error = HpsError;

        fn base_key(&self) -> Result<Option<HpsSecretKey>, Self::Error> {
            HpsSecretKey::from_bytes(self.0.clone()).map(Some)
        }
    }

    #[test]
    fn package_lock_hash_derivation_is_order_independent() {
        let mut props = BTreeMap::new();
        props.insert("PackageLockList".to_string(), "b;a;b".to_string());
        assert_eq!(
            compute_package_lock_hash(&props).as_deref(),
            compute_package_lock_hash(&BTreeMap::from([(
                "PackageLockList".to_string(),
                "a;b".to_string()
            )]))
            .as_deref()
        );
    }

    #[test]
    fn provider_missing_key_returns_deferred() {
        let provider = NoHpsKeyProvider;
        let err = derive_encryption_key(&provider, &BTreeMap::new()).expect_err("missing key");
        assert!(matches!(err, ReadError::Parser(HpsError::KeyMissing)));
    }

    #[test]
    fn provider_rejects_bad_key_length() {
        let provider = StaticProvider(vec![1, 2, 3]);
        let err = derive_encryption_key(&provider, &BTreeMap::new()).expect_err("bad key");
        assert!(matches!(
            err,
            ReadError::KeyProvider(HpsError::InvalidKey { .. })
        ));
    }
}
