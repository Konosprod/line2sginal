//! Envoi direct d'un pack sur les serveurs Signal, comme `putStickers` de Signal Desktop.

use std::time::Duration;

use anyhow::{bail, Context, Result};
use prost::Message;
use reqwest::blocking::multipart::{Form, Part};
use reqwest::blocking::Client;
use reqwest::{Certificate, StatusCode};
use serde::Deserialize;

use super::crypto::{derive_keys, encrypt, to_hex};
use super::{manifest, Pack};

const CHAT_URL: &str = "https://chat.signal.org";
const CDN_URL: &str = "https://cdn.signal.org";
/// Les serveurs Signal présentent un certificat signé par la CA de Signal.
const SIGNAL_CA: &[u8] = include_bytes!("../../assets/signal-ca.pem");

/// Identifiants du compte, tels que stockés par Signal Desktop.
#[derive(Debug, Clone, Default)]
pub struct Credentials {
    /// `uuid_id`, de la forme `<aci>.<numéro d'appareil>`.
    pub username: String,
    pub password: String,
}

#[derive(Deserialize)]
struct UploadForm {
    #[serde(rename = "packId")]
    pack_id: String,
    manifest: UploadAttributes,
    stickers: Vec<UploadAttributes>,
}

#[derive(Deserialize)]
struct UploadAttributes {
    key: String,
    credential: String,
    acl: String,
    algorithm: String,
    date: String,
    policy: String,
    signature: String,
}

/// Envoie le pack et renvoie son lien `https://signal.art/addstickers/…`.
///
/// `progress` reçoit le nombre de stickers envoyés et le total.
pub fn upload(pack: &Pack, creds: &Credentials, progress: &dyn Fn(usize, usize)) -> Result<String> {
    let problems = pack.validate();
    if !problems.is_empty() {
        bail!(problems.join(" "));
    }
    let client = Client::builder()
        .tls_built_in_root_certs(false)
        .add_root_certificate(Certificate::from_pem(SIGNAL_CA)?)
        .user_agent(concat!("line2signal/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(120))
        .build()?;

    let count = pack.stickers.len();
    let resp = client
        .get(format!("{CHAT_URL}/v1/sticker/pack/form/{count}"))
        .basic_auth(creds.username.trim(), Some(creds.password.trim()))
        .send()
        .context("connexion à Signal impossible")?;
    match resp.status() {
        s if s.is_success() => {}
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
            bail!(
                "Signal a refusé les identifiants (HTTP {}).",
                resp.status().as_u16()
            )
        }
        StatusCode::PAYLOAD_TOO_LARGE | StatusCode::TOO_MANY_REQUESTS => {
            bail!(
                "Trop d'envois récents, réessaie plus tard (HTTP {}).",
                resp.status().as_u16()
            )
        }
        s => bail!("Réponse inattendue de Signal : HTTP {}.", s.as_u16()),
    }
    let form: UploadForm = resp.json().context("réponse de Signal illisible")?;
    if form.stickers.len() != count {
        bail!(
            "Signal a fourni {} emplacements pour {count} stickers.",
            form.stickers.len()
        );
    }

    let pack_key: [u8; 32] = rand::random();
    let keys = derive_keys(&pack_key);

    let manifest = manifest::build(pack).encode_to_vec();
    post_to_cdn(
        &client,
        &form.manifest,
        encrypt(&keys, &manifest, rand::random()),
    )
    .context("envoi du manifest")?;
    progress(0, count);
    for (i, (attrs, sticker)) in form.stickers.iter().zip(&pack.stickers).enumerate() {
        post_to_cdn(
            &client,
            attrs,
            encrypt(&keys, &sticker.data, rand::random()),
        )
        .with_context(|| format!("envoi du sticker n°{}", i + 1))?;
        progress(i + 1, count);
    }

    Ok(format!(
        "https://signal.art/addstickers/#pack_id={}&pack_key={}",
        form.pack_id,
        to_hex(&pack_key)
    ))
}

/// Formulaire S3 : les champs doivent précéder le fichier, dans cet ordre.
fn post_to_cdn(client: &Client, attrs: &UploadAttributes, body: Vec<u8>) -> Result<()> {
    let form = Form::new()
        .text("key", attrs.key.clone())
        .text("x-amz-credential", attrs.credential.clone())
        .text("acl", attrs.acl.clone())
        .text("x-amz-algorithm", attrs.algorithm.clone())
        .text("x-amz-date", attrs.date.clone())
        .text("policy", attrs.policy.clone())
        .text("x-amz-signature", attrs.signature.clone())
        .text("Content-Type", "application/octet-stream")
        .part(
            "file",
            Part::bytes(body).mime_str("application/octet-stream")?,
        );
    client
        .post(format!("{CDN_URL}/"))
        .multipart(form)
        .send()?
        .error_for_status()?;
    Ok(())
}
