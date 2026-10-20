//! Mémorisation facultative des identifiants Signal dans le trousseau du système.

use keyring::Entry;
use line2signal::signal::upload::Credentials;

const SERVICE: &str = "line2signal";
const ACCOUNT: &str = "signal";

fn entry() -> keyring::Result<Entry> {
    Entry::new(SERVICE, ACCOUNT)
}

pub fn load() -> Option<Credentials> {
    let stored = entry().and_then(|e| e.get_password()).ok()?;
    let (username, password) = stored.split_once('\n')?;
    Some(Credentials {
        username: username.to_string(),
        password: password.to_string(),
    })
}

pub fn save(creds: &Credentials) -> Result<(), String> {
    entry()
        .and_then(|e| {
            e.set_password(&format!(
                "{}\n{}",
                creds.username.trim(),
                creds.password.trim()
            ))
        })
        .map_err(|e| e.to_string())
}

pub fn forget() {
    // Rien à supprimer n'est pas une erreur.
    let _ = entry().and_then(|e| e.delete_credential());
}
