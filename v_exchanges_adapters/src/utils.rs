use std::time::SystemTime;

use hmac::{Hmac, Mac as _};
use secrecy::{ExposeSecret as _, SecretString};
use sha2::Sha256;

#[deprecated(note = "it doesn't even make sense in the first place")]
pub fn hmac_sign_key(pubkey: &str, secret: &SecretString) -> String {
	let time = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).expect("always after the epoch");
	let expires = time.as_millis() as u64 + 1000;

	let mut hmac = Hmac::<Sha256>::new_from_slice(secret.expose_secret().as_bytes()).expect("hmac accepts key of any length");

	hmac.update(format!("GET/realtime{expires}").as_bytes());
	hex::encode(hmac.finalize().into_bytes())
}
