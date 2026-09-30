// The receiver's own TLS identity. With the network switch on, the receiver
// speaks only TLS; the certificate the other devices check is a private CA made
// here, and the server certificate it signs. No custom crypto: rcgen builds the
// certificates and rustls does the handshake.
//
// The CA private key is dropped right after the server certificate is signed
// and is never stored anywhere, so a stolen settings file can never mint a new
// certificate that other devices would trust. Only the server key is stored,
// sealed with the same per-user protection as the rest of the stored secrets.

use crate::{dec_str, enc_str, Settings, ENC_PREFIX};
use rcgen::{
    BasicConstraints, CertificateParams, DistinguishedName, DnType, ExtendedKeyUsagePurpose, IsCa, Issuer, KeyPair,
    KeyUsagePurpose, SanType,
};
use sha2::{Digest, Sha256};
use std::net::IpAddr;
use std::str::FromStr;
use std::sync::Arc;
use tokio_rustls::rustls::pki_types::{pem::PemObject, CertificateDer, PrivateKeyDer};
use tokio_rustls::rustls::ServerConfig;

pub(crate) const REMOTE_TLS_KEY: &str = "remoteTlsKey";
pub(crate) const REMOTE_TLS_CERT: &str = "remoteTlsCert";
pub(crate) const REMOTE_TLS_CA: &str = "remoteTlsCa";
// Machine values: they never leave this machine (no export, no import, never
// shown to a window) and they survive both resets.
pub(crate) const REMOTE_TLS_KEYS: [&str; 3] = [REMOTE_TLS_KEY, REMOTE_TLS_CERT, REMOTE_TLS_CA];

pub(crate) const ERR_KEY_NOT_PROTECTED: &str = "the certificate key could not be protected";
pub(crate) const ERR_SAVE_FAILED: &str = "save-failed";

const VALIDITY_YEARS: i32 = 10;

pub(crate) struct TlsIdentity {
    pub cert_pem: String,
    pub key_pem: String,
    pub ca_pem: String,
}

// Why a fresh identity had to be made, for the one log line.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub(crate) enum IdentitySource {
    Stored,
    Created,
    KeyUnreadable,
}

fn names_for(dns_host: Option<&str>, computer_name: Option<&str>) -> Vec<String> {
    let mut names: Vec<String> = vec!["localhost".to_string(), "127.0.0.1".to_string()];
    let mut add = |n: &str| {
        let n = n.trim().to_lowercase();
        if n.is_empty() {
            return;
        }
        for candidate in [n.clone(), format!("{n}.local")] {
            if !names.contains(&candidate) {
                names.push(candidate);
            }
        }
    };
    if let Some(h) = dns_host {
        add(h);
    }
    if let Some(c) = computer_name {
        add(c);
    }
    names
}

// localhost, 127.0.0.1, the DNS host name and COMPUTERNAME (lower-cased, each
// also with .local): the names another device is likely to type.
pub(crate) fn host_names() -> Vec<String> {
    let computer = std::env::var("COMPUTERNAME").ok();
    names_for(dns_host_name().as_deref(), computer.as_deref())
}

#[cfg(windows)]
fn dns_host_name() -> Option<String> {
    use windows::core::PWSTR;
    use windows::Win32::System::SystemInformation::{ComputerNameDnsHostname, GetComputerNameExW};
    let mut len: u32 = 0;
    // The first call only reports the needed length.
    let _ = unsafe { GetComputerNameExW(ComputerNameDnsHostname, PWSTR::null(), &mut len) };
    if len == 0 || len > 4096 {
        return None;
    }
    let mut buf = vec![0u16; len as usize + 1];
    let mut size = buf.len() as u32;
    unsafe { GetComputerNameExW(ComputerNameDnsHostname, PWSTR(buf.as_mut_ptr()), &mut size) }.ok()?;
    let s = String::from_utf16_lossy(&buf[..size as usize]);
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

#[cfg(not(windows))]
fn dns_host_name() -> Option<String> {
    None
}

fn year_now() -> i32 {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    1970 + (secs / 31_556_952) as i32
}

fn generate(names: &[String]) -> Result<TlsIdentity, String> {
    let year = year_now();
    // Starts a year back so a device with a slightly wrong clock still accepts it.
    let not_before = rcgen::date_time_ymd(year - 1, 1, 1);
    let not_after = rcgen::date_time_ymd(year + VALIDITY_YEARS, 12, 31);

    let ca_key = KeyPair::generate().map_err(|e| e.to_string())?;
    let mut ca_params = CertificateParams::default();
    let mut ca_dn = DistinguishedName::new();
    ca_dn.push(DnType::CommonName, "Remote access CA");
    ca_params.distinguished_name = ca_dn;
    ca_params.is_ca = IsCa::Ca(BasicConstraints::Constrained(0));
    ca_params.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
    ca_params.not_before = not_before;
    ca_params.not_after = not_after;
    let ca_cert = ca_params.self_signed(&ca_key).map_err(|e| e.to_string())?;

    let server_key = KeyPair::generate().map_err(|e| e.to_string())?;
    let mut server_params = CertificateParams::default();
    let mut server_dn = DistinguishedName::new();
    server_dn.push(DnType::CommonName, "Remote access receiver");
    server_params.distinguished_name = server_dn;
    server_params.is_ca = IsCa::ExplicitNoCa;
    server_params.key_usages = vec![KeyUsagePurpose::DigitalSignature];
    server_params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
    server_params.not_before = not_before;
    server_params.not_after = not_after;
    server_params.subject_alt_names = names
        .iter()
        .map(|n| match IpAddr::from_str(n) {
            Ok(ip) => Ok(SanType::IpAddress(ip)),
            Err(_) => n.clone().try_into().map(SanType::DnsName).map_err(|e: rcgen::Error| e.to_string()),
        })
        .collect::<Result<Vec<_>, String>>()?;
    let issuer = Issuer::from_params(&ca_params, &ca_key);
    let server_cert = server_params.signed_by(&server_key, &issuer).map_err(|e| e.to_string())?;
    // ca_key goes out of scope here and is never written anywhere.
    Ok(TlsIdentity { cert_pem: server_cert.pem(), key_pem: server_key.serialize_pem(), ca_pem: ca_cert.pem() })
}

pub(crate) fn build_server_config(id: &TlsIdentity) -> Result<Arc<ServerConfig>, String> {
    let certs = CertificateDer::pem_slice_iter(id.cert_pem.as_bytes())
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    if certs.is_empty() {
        return Err("no certificate".to_string());
    }
    let key = PrivateKeyDer::from_pem_slice(id.key_pem.as_bytes()).map_err(|e| e.to_string())?;
    let provider = Arc::new(tokio_rustls::rustls::crypto::ring::default_provider());
    let mut config = ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|e| e.to_string())?
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .map_err(|e| e.to_string())?;
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    Ok(Arc::new(config))
}

fn stored(settings: &Settings, key: &str) -> Option<String> {
    settings.ui_texts.get(key).filter(|v| !v.is_empty()).cloned()
}

// The identity the settings already hold, when it is complete, the key is
// sealed and readable here, and key and certificate fit together.
fn load_stored(settings: &Settings) -> Result<TlsIdentity, IdentitySource> {
    let (Some(sealed), Some(cert), Some(ca)) = (stored(settings, REMOTE_TLS_KEY), stored(settings, REMOTE_TLS_CERT), stored(settings, REMOTE_TLS_CA))
    else {
        return Err(IdentitySource::Created);
    };
    if !sealed.starts_with(ENC_PREFIX) {
        return Err(IdentitySource::KeyUnreadable);
    }
    let key_pem = dec_str(&sealed);
    if key_pem.is_empty() {
        return Err(IdentitySource::KeyUnreadable);
    }
    let id = TlsIdentity { cert_pem: cert, key_pem, ca_pem: ca };
    build_server_config(&id).map(|_| id).map_err(|_| IdentitySource::KeyUnreadable)
}

// Makes a fresh identity and stores it: the sealed key, the server certificate
// and the CA certificate. Refuses, storing nothing, when the key did not come
// back sealed; a failed write restores what was there.
fn create_and_store(settings: &mut Settings, names: &[String], mut save: impl FnMut(&Settings) -> bool) -> Result<TlsIdentity, String> {
    let id = generate(names)?;
    let sealed = enc_str(&id.key_pem);
    if !sealed.starts_with(ENC_PREFIX) {
        return Err(ERR_KEY_NOT_PROTECTED.to_string());
    }
    let old: Vec<(&str, Option<String>)> = REMOTE_TLS_KEYS.iter().map(|k| (*k, settings.ui_texts.get(*k).cloned())).collect();
    settings.ui_texts.insert(REMOTE_TLS_KEY.to_string(), sealed);
    settings.ui_texts.insert(REMOTE_TLS_CERT.to_string(), id.cert_pem.clone());
    settings.ui_texts.insert(REMOTE_TLS_CA.to_string(), id.ca_pem.clone());
    if !save(settings) {
        for (k, v) in old {
            match v {
                Some(v) => {
                    settings.ui_texts.insert(k.to_string(), v);
                }
                None => {
                    settings.ui_texts.remove(k);
                }
            }
        }
        return Err(ERR_SAVE_FAILED.to_string());
    }
    Ok(id)
}

// Called before the listener is spawned. Returns the identity and where it came
// from; a listener must not start when this errs.
pub(crate) fn ensure_tls_identity(
    settings: &mut Settings,
    names: &[String],
    save: impl FnMut(&Settings) -> bool,
) -> Result<(TlsIdentity, IdentitySource), String> {
    match load_stored(settings) {
        Ok(id) => Ok((id, IdentitySource::Stored)),
        Err(why) => create_and_store(settings, names, save).map(|id| (id, why)),
    }
}

// A fresh CA and server certificate: every device that trusted the old one has
// to be set up again. The old identity stays in place when the write fails.
pub(crate) fn renew_tls_identity(settings: &mut Settings, names: &[String], save: impl FnMut(&Settings) -> bool) -> Result<TlsIdentity, String> {
    create_and_store(settings, names, save)
}

// Hex pairs with colons, e.g. "AB:CD:...", of the SHA-256 of the CA certificate.
pub(crate) fn fingerprint(ca_pem: &str) -> Option<String> {
    let der = CertificateDer::from_pem_slice(ca_pem.as_bytes()).ok()?;
    let digest = Sha256::digest(der.as_ref());
    Some(digest.iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(":"))
}

pub(crate) fn stored_ca(settings: &Settings) -> Option<String> {
    stored(settings, REMOTE_TLS_CA)
}

// What a window may see: everything except the private key.
pub(crate) fn strip_private_key(settings: &mut Settings) {
    settings.ui_texts.remove(REMOTE_TLS_KEY);
}

// Removes all three machine values from a map about to leave this machine or
// come in from another one.
pub(crate) fn strip_tls_keys(texts: &mut std::collections::HashMap<String, String>) {
    for k in REMOTE_TLS_KEYS {
        texts.remove(k);
    }
}

// The three values of `from`, to put back after settings were replaced.
pub(crate) fn tls_values(settings: &Settings) -> Vec<(String, String)> {
    REMOTE_TLS_KEYS.iter().filter_map(|k| settings.ui_texts.get(*k).map(|v| (k.to_string(), v.clone()))).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names() -> Vec<String> {
        names_for(Some("Test-PC"), Some("TESTPC"))
    }

    #[test]
    fn names_are_lowercase_and_deduplicated() {
        let n = names_for(Some("Box"), Some("BOX"));
        assert_eq!(n, vec!["localhost", "127.0.0.1", "box", "box.local"]);
        let n = names();
        assert!(n.contains(&"test-pc.local".to_string()) && n.contains(&"testpc".to_string()));
    }

    #[test]
    fn a_new_identity_is_stored_sealed_and_the_ca_key_is_nowhere() {
        let mut s = Settings::default();
        let mut saves = 0;
        let (id, source) = ensure_tls_identity(&mut s, &names(), |_| {
            saves += 1;
            true
        })
        .unwrap();
        assert_eq!(saves, 1);
        assert_eq!(source, IdentitySource::Created);
        let sealed = s.ui_texts.get(REMOTE_TLS_KEY).unwrap();
        assert!(sealed.starts_with(ENC_PREFIX));
        assert!(!sealed.contains("PRIVATE KEY"));
        assert_eq!(dec_str(sealed), id.key_pem);
        // Exactly one private key exists in the stored values, and it is the server's.
        let all: String = REMOTE_TLS_KEYS.iter().map(|k| s.ui_texts.get(*k).cloned().unwrap_or_default()).collect();
        assert_eq!(all.matches("PRIVATE KEY").count(), 0);
        assert!(!id.ca_pem.contains("PRIVATE KEY") && !id.cert_pem.contains("PRIVATE KEY"));
        assert!(id.key_pem.contains("PRIVATE KEY"));
        // A second call reuses it and writes nothing.
        let (again, source2) = ensure_tls_identity(&mut s, &names(), |_| panic!("no write expected")).unwrap();
        assert_eq!(source2, IdentitySource::Stored);
        assert_eq!(again.cert_pem, id.cert_pem);
        assert!(build_server_config(&id).is_ok());
    }

    #[test]
    fn a_failed_write_keeps_the_old_identity_and_starts_nothing() {
        let mut s = Settings::default();
        assert_eq!(ensure_tls_identity(&mut s, &names(), |_| false).err().as_deref(), Some(ERR_SAVE_FAILED));
        assert!(s.ui_texts.get(REMOTE_TLS_KEY).is_none());
        let (id, _) = ensure_tls_identity(&mut s, &names(), |_| true).unwrap();
        let before = s.ui_texts.get(REMOTE_TLS_CA).cloned();
        assert!(renew_tls_identity(&mut s, &names(), |_| false).is_err());
        assert_eq!(s.ui_texts.get(REMOTE_TLS_CA).cloned(), before);
        assert_eq!(load_stored(&s).ok().map(|i| i.cert_pem), Some(id.cert_pem));
    }

    #[test]
    fn an_unreadable_key_makes_a_new_identity() {
        let mut s = Settings::default();
        let (old, _) = ensure_tls_identity(&mut s, &names(), |_| true).unwrap();
        // A sealed value from another account decrypts to an empty string.
        s.ui_texts.insert(REMOTE_TLS_KEY.into(), format!("{ENC_PREFIX}AAAA"));
        let (fresh, source) = ensure_tls_identity(&mut s, &names(), |_| true).unwrap();
        assert_eq!(source, IdentitySource::KeyUnreadable);
        assert_ne!(fresh.ca_pem, old.ca_pem);
        // A key stored unsealed is never trusted either.
        s.ui_texts.insert(REMOTE_TLS_KEY.into(), old.key_pem.clone());
        let (_, source) = ensure_tls_identity(&mut s, &names(), |_| true).unwrap();
        assert_eq!(source, IdentitySource::KeyUnreadable);
        assert!(s.ui_texts.get(REMOTE_TLS_KEY).unwrap().starts_with(ENC_PREFIX));
    }

    #[test]
    fn renewing_replaces_the_ca_and_changes_the_fingerprint() {
        let mut s = Settings::default();
        let (old, _) = ensure_tls_identity(&mut s, &names(), |_| true).unwrap();
        let fp_old = fingerprint(&old.ca_pem).unwrap();
        let new = renew_tls_identity(&mut s, &names(), |_| true).unwrap();
        let fp_new = fingerprint(&new.ca_pem).unwrap();
        assert_ne!(fp_old, fp_new);
        assert_eq!(fp_new.len(), 32 * 3 - 1);
        assert_eq!(fingerprint(&stored_ca(&s).unwrap()), Some(fp_new));
    }

    #[test]
    fn the_private_key_never_reaches_a_window_copy() {
        let mut s = Settings::default();
        ensure_tls_identity(&mut s, &names(), |_| true).unwrap();
        strip_private_key(&mut s);
        assert!(s.ui_texts.get(REMOTE_TLS_KEY).is_none());
        assert!(s.ui_texts.get(REMOTE_TLS_CA).is_some());
        strip_tls_keys(&mut s.ui_texts);
        assert!(REMOTE_TLS_KEYS.iter().all(|k| !s.ui_texts.contains_key(*k)));
    }
}
