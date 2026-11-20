use aes::cipher::{BlockDecrypt, KeyInit, generic_array::GenericArray};
use aes::{Aes128, Aes192, Aes256};

#[derive(Debug)]
pub enum CryptoError {
    BadKeyLength(usize),
    InvalidBlockLength(usize),
    BadPadding,
    DecompressFailed(String),
}

impl std::fmt::Display for CryptoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CryptoError::BadKeyLength(n) => write!(f, "invalid AES key length: {n}"),
            CryptoError::InvalidBlockLength(n) => write!(f, "ciphertext length {n} not a multiple of 16"),
            CryptoError::BadPadding => write!(f, "invalid PKCS7 padding"),
            CryptoError::DecompressFailed(e) => write!(f, "Oodle decompression failed: {e}"),
        }
    }
}

impl std::error::Error for CryptoError {}

/// AES-ECB decryption with PKCS7 unpadding (matching C# `Aes.DecryptEcb(..., PaddingMode.PKCS7)`).
pub fn decrypt_aes_ecb_pkcs7(encrypted: &[u8], key: &[u8]) -> Result<Vec<u8>, CryptoError> {
    if encrypted.is_empty() || encrypted.len() % 16 != 0 {
        return Err(CryptoError::InvalidBlockLength(encrypted.len()));
    }
    let mut out = encrypted.to_vec();
    match key.len() {
        16 => {
            let cipher = Aes128::new_from_slice(key).map_err(|_| CryptoError::BadKeyLength(key.len()))?;
            for chunk in out.chunks_exact_mut(16) {
                cipher.decrypt_block(GenericArray::from_mut_slice(chunk));
            }
        }
        24 => {
            let cipher = Aes192::new_from_slice(key).map_err(|_| CryptoError::BadKeyLength(key.len()))?;
            for chunk in out.chunks_exact_mut(16) {
                cipher.decrypt_block(GenericArray::from_mut_slice(chunk));
            }
        }
        32 => {
            let cipher = Aes256::new_from_slice(key).map_err(|_| CryptoError::BadKeyLength(key.len()))?;
            for chunk in out.chunks_exact_mut(16) {
                cipher.decrypt_block(GenericArray::from_mut_slice(chunk));
            }
        }
        n => return Err(CryptoError::BadKeyLength(n)),
    }

    let pad = *out.last().ok_or(CryptoError::BadPadding)? as usize;
    if pad == 0 || pad > 16 || pad > out.len() {
        return Err(CryptoError::BadPadding);
    }
    if !out[out.len() - pad..].iter().all(|&b| b as usize == pad) {
        return Err(CryptoError::BadPadding);
    }
    out.truncate(out.len() - pad);
    Ok(out)
}

/// Oodle (Kraken / Mermaid / Selkie / Leviathan) decompression using `oozextract`.
pub fn decompress_oodle(compressed: &[u8], decompressed_size: usize) -> Result<Vec<u8>, CryptoError> {
    let mut out = vec![0u8; decompressed_size];
    let mut extractor = oozextract::Extractor::new();
    let written = extractor
        .read_from_slice(compressed, &mut out)
        .map_err(|e| CryptoError::DecompressFailed(format!("{e:?}")))?;
    out.truncate(written);
    Ok(out)
}
