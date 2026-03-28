//! AES-128-ECB Encryption
//!
//! Uses the `aes` and `cipher` crates for AES encryption,
//! and the `md-5` crate for MD5 hashing.

use aes::Aes128;
use cipher::{BlockEncrypt, KeyInit, generic_array::GenericArray};
use generic_array::typenum::U16;
use md5::{Digest, Md5};

type AesBlock = GenericArray<u8, U16>;

/// Encrypt data using AES-128-ECB with PKCS7 padding
pub fn encrypt_aes_ecb(data: &[u8], key: &[u8; 16]) -> Vec<u8> {
    let cipher = Aes128::new(key.into());

    // PKCS7 padding
    let pad_len = 16 - data.len() % 16;
    let mut padded = data.to_vec();
    padded.extend(std::iter::repeat(pad_len as u8).take(pad_len));

    let mut result = Vec::new();
    for chunk in padded.chunks(16) {
        let mut block = AesBlock::from_exact_iter(chunk.iter().copied()).unwrap();
        cipher.encrypt_block(&mut block);
        result.extend_from_slice(&block);
    }
    result
}

/// Compute MD5 hex string of data
pub fn md5_hex(data: &[u8]) -> String {
    let mut hasher = Md5::new();
    hasher.update(data);
    format!("{:x}", hasher.finalize())
}
