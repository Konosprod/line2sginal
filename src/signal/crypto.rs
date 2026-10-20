//! Chiffrement des packs, identique à `encryptAttachment` de Signal Desktop :
//! `iv (16) || AES-256-CBC-PKCS7 || HMAC-SHA256(iv || chiffré)`.

use aes::Aes256;
use anyhow::{bail, Result};
use cbc::cipher::block_padding::Pkcs7;
use cbc::cipher::{BlockDecryptMut, BlockEncryptMut, KeyIvInit};
use hkdf::Hkdf;
use hmac::{Hmac, Mac};
use sha2::Sha256;

pub struct Keys {
    aes: [u8; 32],
    mac: [u8; 32],
}

/// Dérive les clés AES et HMAC de la clé du pack (HKDF, info « Sticker Pack »).
pub fn derive_keys(pack_key: &[u8; 32]) -> Keys {
    let mut okm = [0u8; 64];
    Hkdf::<Sha256>::new(Some(&[0u8; 32]), pack_key)
        .expand(b"Sticker Pack", &mut okm)
        .expect("64 octets est une longueur HKDF valide");
    let mut keys = Keys {
        aes: [0; 32],
        mac: [0; 32],
    };
    keys.aes.copy_from_slice(&okm[..32]);
    keys.mac.copy_from_slice(&okm[32..]);
    keys
}

fn hmac(keys: &Keys, data: &[u8]) -> Hmac<Sha256> {
    let mut mac = Hmac::<Sha256>::new_from_slice(&keys.mac).expect("toute taille de clé convient");
    mac.update(data);
    mac
}

pub fn encrypt(keys: &Keys, plaintext: &[u8], iv: [u8; 16]) -> Vec<u8> {
    let ciphertext = cbc::Encryptor::<Aes256>::new(&keys.aes.into(), &iv.into())
        .encrypt_padded_vec_mut::<Pkcs7>(plaintext);
    let mut out = iv.to_vec();
    out.extend_from_slice(&ciphertext);
    let tag = hmac(keys, &out).finalize().into_bytes();
    out.extend_from_slice(&tag);
    out
}

pub fn decrypt(keys: &Keys, blob: &[u8]) -> Result<Vec<u8>> {
    if blob.len() < 16 + 16 + 32 {
        bail!("données chiffrées trop courtes");
    }
    let (body, tag) = blob.split_at(blob.len() - 32);
    if hmac(keys, body).verify_slice(tag).is_err() {
        bail!("HMAC invalide");
    }
    let (iv, ciphertext) = body.split_at(16);
    let iv: [u8; 16] = iv.try_into().expect("16 octets");
    cbc::Decryptor::<Aes256>::new(&keys.aes.into(), &iv.into())
        .decrypt_padded_vec_mut::<Pkcs7>(ciphertext)
        .map_err(|_| anyhow::anyhow!("remplissage invalide"))
}

pub fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let keys = derive_keys(&[7; 32]);
        let blob = encrypt(&keys, b"bonjour signal", [1; 16]);
        assert_eq!(blob.len(), 16 + 16 + 32);
        assert_eq!(&blob[..16], &[1; 16]);
        assert_eq!(decrypt(&keys, &blob).unwrap(), b"bonjour signal");
    }

    #[test]
    fn tampering_is_detected() {
        let keys = derive_keys(&[7; 32]);
        let mut blob = encrypt(&keys, b"bonjour signal", [1; 16]);
        blob[20] ^= 1;
        assert!(decrypt(&keys, &blob).is_err());
        assert!(decrypt(&derive_keys(&[8; 32]), &encrypt(&keys, b"x", [0; 16])).is_err());
    }

    #[test]
    fn hex() {
        assert_eq!(to_hex(&[0, 15, 255]), "000fff");
    }
}
