// The receiver's own TLS identity. With the network switch on, the receiver
// speaks only TLS; the certificate the other devices check is a private CA made
// here, and the server certificate it signs. No custom crypto: rcgen builds the
// certificates and rustls does the handshake.
//
// The CA private key is dropped right after the server certificate is signed
// and is never stored anywhere, so a stolen settings file can never mint a new
// certificate that other devices would trust. Only the server key is stored,
// sealed with the same per-user protection as the rest of the stored secrets.

use crate::backups::{civil_from_days, days_from_civil};
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

// Expert switch: the certificates are valid for 825 days instead of ten years,
// the longest an Apple device accepts. Only `set_short_lived_core` writes it.
pub(crate) const SHORT_LIVED_KEY: &str = "remoteTlsShortLived";
// notBefore to notAfter spans 823 days: two days inside the 825 a device accepts,
// since validity counts inclusively and a device may add a second.
const SHORT_LIVED_SPAN_DAYS: i64 = 823;

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
    // The stored certificate (or its CA) is past its end date.
    Expired,
}

fn short_lived(settings: &Settings) -> bool {
    settings.ui_flags.get(SHORT_LIVED_KEY) == Some(&true)
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
    let host = dns_host_name();
    let mut names = names_for(host.as_deref(), computer.as_deref());
    // The machine's own addresses too, so https://192.168.x.y works. They are
    // read when the certificate is made, so a renewal picks up new ones.
    for ip in local_ipv4_names(host.as_deref().or(computer.as_deref())) {
        if !names.contains(&ip) {
            names.push(ip);
        }
    }
    names
}

// Non-loopback, non-link-local IPv4 addresses this machine's own name resolves to.
fn local_ipv4_names(host: Option<&str>) -> Vec<String> {
    use std::net::ToSocketAddrs;
    let Some(host) = host else { return Vec::new() };
    let Ok(addrs) = (host, 0u16).to_socket_addrs() else { return Vec::new() };
    let mut out = Vec::new();
    for a in addrs {
        if let IpAddr::V4(v4) = a.ip() {
            if !v4.is_loopback() && !v4.is_unspecified() && !v4.is_link_local() && !v4.is_multicast() {
                let text = v4.to_string();
                if !out.contains(&text) {
                    out.push(text);
                }
            }
        }
    }
    out
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

// Signed, because certificate dates are compared and shifted in both directions.
fn now_secs() -> i64 {
    crate::copy::now_secs() as i64
}

type Ymd = (i32, u8, u8);

// The validity window. Default: from the start of last year to the end of the
// year ten years ahead. Short-lived: from yesterday, 823 days in all, so no
// device sees a longer window than it allows.
fn validity(short: bool) -> (Ymd, Ymd) {
    let now = now_secs();
    if short {
        let today = now.div_euclid(86_400);
        (civil_from_days(today - 1), civil_from_days(today - 1 + SHORT_LIVED_SPAN_DAYS))
    } else {
        let year = 1970 + (now / 31_556_952) as i32;
        // Starts a year back so a device with a slightly wrong clock still accepts it.
        ((year - 1, 1, 1), (year + VALIDITY_YEARS, 12, 31))
    }
}

fn generate(names: &[String], short: bool) -> Result<TlsIdentity, String> {
    let (nb, na) = validity(short);
    generate_span(names, nb, na)
}

fn generate_span(names: &[String], nb: Ymd, na: Ymd) -> Result<TlsIdentity, String> {
    let not_before = rcgen::date_time_ymd(nb.0, nb.1, nb.2);
    let not_after = rcgen::date_time_ymd(na.0, na.1, na.2);

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
    // A name that cannot be a certificate name (non-ASCII, say) is skipped
    // instead of failing the whole identity.
    server_params.subject_alt_names = names
        .iter()
        .filter_map(|n| match IpAddr::from_str(n) {
            Ok(ip) => Some(SanType::IpAddress(ip)),
            Err(_) => n.clone().try_into().map(SanType::DnsName).ok(),
        })
        .collect();
    let issuer = Issuer::from_params(&ca_params, &ca_key);
    let server_cert = server_params.signed_by(&server_key, &issuer).map_err(|e| e.to_string())?;
    // ca_key goes out of scope here and is never written anywhere.
    Ok(TlsIdentity { cert_pem: server_cert.pem(), key_pem: server_key.serialize_pem(), ca_pem: ca_cert.pem() })
}

// One DER element at the start of `b`: (tag, content, rest).
fn der_next(b: &[u8]) -> Option<(u8, &[u8], &[u8])> {
    let tag = *b.first()?;
    let first = *b.get(1)?;
    let (len, head) = if first < 0x80 {
        (first as usize, 2)
    } else {
        let n = (first & 0x7f) as usize;
        if n == 0 || n > 4 {
            return None;
        }
        let mut len = 0usize;
        for i in 0..n {
            len = (len << 8) | *b.get(2 + i)? as usize;
        }
        (len, 2 + n)
    };
    let end = head.checked_add(len)?;
    if end > b.len() {
        return None;
    }
    Some((tag, &b[head..end], &b[end..]))
}

// One DER time (UTCTime or GeneralizedTime) as seconds since 1970.
fn der_time(tag: u8, text: &[u8]) -> Option<i64> {
    let digits = std::str::from_utf8(text).ok()?.strip_suffix('Z')?;
    if !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let (year, tail) = match tag {
        0x17 if digits.len() == 12 => {
            let yy: i64 = digits[..2].parse().ok()?;
            (if yy >= 50 { 1900 + yy } else { 2000 + yy }, &digits[2..])
        }
        0x18 if digits.len() == 14 => (digits[..4].parse().ok()?, &digits[4..]),
        _ => return None,
    };
    let part = |i: usize| -> Option<i64> { tail[i..i + 2].parse().ok() };
    let (mo, d, h, mi, se) = (part(0)?, part(2)?, part(4)?, part(6)?, part(8)?);
    if !(1..=12).contains(&mo) || !(1..=31).contains(&d) || h > 23 || mi > 59 || se > 60 {
        return None;
    }
    Some(days_from_civil(year, mo, d) * 86_400 + h * 3600 + mi * 60 + se)
}

// (notBefore, notAfter) of a certificate, as seconds since 1970, read straight
// from the DER. The certificate, its TBS part and the validity must all be
// SEQUENCEs, otherwise nothing is read.
fn cert_validity(der: &[u8]) -> Option<(i64, i64)> {
    let (tag, cert, _) = der_next(der)?;
    if tag != 0x30 {
        return None;
    }
    let (tag, tbs, _) = der_next(cert)?;
    if tag != 0x30 {
        return None;
    }
    let mut rest = tbs;
    let (tag, _, after) = der_next(rest)?;
    if tag == 0xa0 {
        rest = after; // the optional version
    }
    // serial, signature algorithm, issuer
    for _ in 0..3 {
        rest = der_next(rest)?.2;
    }
    let (tag, validity, _) = der_next(rest)?;
    if tag != 0x30 {
        return None;
    }
    let (nb_tag, nb_text, after_not_before) = der_next(validity)?;
    let (na_tag, na_text, _) = der_next(after_not_before)?;
    Some((der_time(nb_tag, nb_text)?, der_time(na_tag, na_text)?))
}

#[cfg(test)]
fn cert_not_after(der: &[u8]) -> Option<i64> {
    cert_validity(der).map(|v| v.1)
}

// True when the PEM certificate is past its end date at `now`, or starts more
// than a day later (made under a clock that ran ahead: every client would reject
// it as not yet valid). A certificate whose dates cannot be read counts as
// expired: it is replaced, never trusted blind.
fn pem_expired(pem: &str, now: i64) -> bool {
    match CertificateDer::from_pem_slice(pem.as_bytes()) {
        Ok(der) => cert_validity(der.as_ref()).map_or(true, |(nb, na)| na <= now || nb > now + 86_400),
        Err(_) => true,
    }
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
    build_server_config(&id).map_err(|_| IdentitySource::KeyUnreadable)?;
    let now = now_secs();
    if pem_expired(&id.cert_pem, now) || pem_expired(&id.ca_pem, now) {
        return Err(IdentitySource::Expired);
    }
    Ok(id)
}

// Makes a fresh identity and stores it: the sealed key, the server certificate
// and the CA certificate. Refuses, storing nothing, when the key did not come
// back sealed; a failed write restores what was there.
fn create_and_store(settings: &mut Settings, names: &[String], mut save: impl FnMut(&Settings) -> bool) -> Result<TlsIdentity, String> {
    let id = generate(names, short_lived(settings))?;
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

// The expert switch for the certificate lifetime. The flag and the new
// identity (made with the new lifetime) are saved together in one write; when
// anything fails, flag and identity are both as they were. Nothing else sets
// this flag, so it never changes without the certificate changing with it.
pub(crate) fn set_short_lived_core(
    settings: &mut Settings,
    enabled: bool,
    names: &[String],
    save: impl FnMut(&Settings) -> bool,
) -> Result<TlsIdentity, String> {
    let old = settings.ui_flags.get(SHORT_LIVED_KEY).copied();
    settings.ui_flags.insert(SHORT_LIVED_KEY.to_string(), enabled);
    let result = create_and_store(settings, names, save);
    if result.is_err() {
        match old {
            Some(v) => settings.ui_flags.insert(SHORT_LIVED_KEY.to_string(), v),
            None => settings.ui_flags.remove(SHORT_LIVED_KEY),
        };
    }
    result
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

// What a window may see: none of the receiver's TLS values. The fingerprint of
// the CA has its own command.
pub(crate) fn strip_private_key(settings: &mut Settings) {
    strip_tls_keys(&mut settings.ui_texts);
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

    fn contains_bytes(hay: &[u8], needle: &[u8]) -> bool {
        hay.windows(needle.len()).any(|w| w == needle)
    }

    #[test]
    fn an_unusable_name_is_skipped_and_addresses_become_ip_names() {
        let names = vec!["localhost".to_string(), "127.0.0.1".to_string(), "bücher-pc".to_string(), "192.168.7.42".to_string()];
        let id = generate(&names, false).expect("a non-ASCII name must not fail the identity");
        assert!(build_server_config(&id).is_ok());
        let der = CertificateDer::from_pem_slice(id.cert_pem.as_bytes()).unwrap();
        assert!(contains_bytes(der.as_ref(), &[192, 168, 7, 42]), "the address must be a name of the certificate");
        assert!(contains_bytes(der.as_ref(), b"localhost"));
        assert!(!contains_bytes(der.as_ref(), b"cher-pc"));
    }

    #[test]
    fn local_addresses_never_include_loopback_or_link_local() {
        for ip in local_ipv4_names(Some("localhost")) {
            let v4: std::net::Ipv4Addr = ip.parse().unwrap();
            assert!(!v4.is_loopback() && !v4.is_link_local());
        }
        assert!(local_ipv4_names(None).is_empty());
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
        assert!(!s.ui_texts.contains_key(REMOTE_TLS_KEY));
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
        assert!(REMOTE_TLS_KEYS.iter().all(|k| !s.ui_texts.contains_key(*k)), "key, certificate and CA all stay in the backend");
    }

    fn leaf_not_after(id: &TlsIdentity) -> i64 {
        cert_not_after(CertificateDer::from_pem_slice(id.cert_pem.as_bytes()).unwrap().as_ref()).expect("the end date must be readable")
    }
    fn leaf_not_before(id: &TlsIdentity) -> i64 {
        cert_validity(CertificateDer::from_pem_slice(id.cert_pem.as_bytes()).unwrap().as_ref()).expect("the dates must be readable").0
    }
    fn ca_not_before(id: &TlsIdentity) -> i64 {
        cert_validity(CertificateDer::from_pem_slice(id.ca_pem.as_bytes()).unwrap().as_ref()).expect("the dates must be readable").0
    }
    fn ca_not_after(id: &TlsIdentity) -> i64 {
        cert_not_after(CertificateDer::from_pem_slice(id.ca_pem.as_bytes()).unwrap().as_ref()).expect("the end date must be readable")
    }

    #[test]
    fn the_end_date_reader_agrees_with_the_calendar() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(days_from_civil(2000, 3, 1), 11_017);
        for day in [0i64, 11_017, 20_000, 29_000, -400] {
            let (y, m, d) = civil_from_days(day);
            assert_eq!(days_from_civil(y as i64, m as i64, d as i64), day);
        }
        let id = generate_span(&names(), (2020, 1, 1), (2031, 12, 31)).unwrap();
        assert_eq!(leaf_not_after(&id), days_from_civil(2031, 12, 31) * 86_400);
        // Past 2049 the date is a GeneralizedTime, which must read the same way.
        let far = generate_span(&names(), (2020, 1, 1), (2061, 6, 15)).unwrap();
        assert_eq!(leaf_not_after(&far), days_from_civil(2061, 6, 15) * 86_400);
    }

    #[test]
    fn the_short_lived_certificate_ends_within_825_days_and_the_default_in_ten_years() {
        let now = now_secs();
        let day = 86_400;
        let short = generate(&names(), true).unwrap();
        for end in [leaf_not_after(&short), ca_not_after(&short)] {
            assert!(end <= now + 825 * day, "longer than 825 days: {}", (end - now) / day);
            assert!(end >= now + 820 * day, "far shorter than expected: {}", (end - now) / day);
        }
        for (nb, na) in [(leaf_not_before(&short), leaf_not_after(&short)), (ca_not_before(&short), ca_not_after(&short))] {
            assert!(na - nb <= 824 * day, "the whole window must stay inside 824 days: {}", (na - nb) / day);
            assert!(nb <= now, "a short certificate is already valid");
        }
        let long = generate(&names(), false).unwrap();
        let (y_now, _, _) = civil_from_days(now.div_euclid(day));
        let end_year = civil_from_days(leaf_not_after(&long).div_euclid(day)).0;
        assert_eq!(end_year, y_now + VALIDITY_YEARS, "the default stays ten years");
        assert!(leaf_not_after(&long) > now + 9 * 365 * day);
    }

    #[test]
    fn the_flag_picks_the_lifetime_for_a_new_and_a_renewed_identity() {
        let now = now_secs();
        let mut s = Settings::default();
        s.ui_flags.insert(SHORT_LIVED_KEY.into(), true);
        let (made, _) = ensure_tls_identity(&mut s, &names(), |_| true).unwrap();
        assert!(leaf_not_after(&made) <= now + 825 * 86_400);
        let renewed = renew_tls_identity(&mut s, &names(), |_| true).unwrap();
        assert!(leaf_not_after(&renewed) <= now + 825 * 86_400);
        s.ui_flags.insert(SHORT_LIVED_KEY.into(), false);
        let renewed = renew_tls_identity(&mut s, &names(), |_| true).unwrap();
        assert!(leaf_not_after(&renewed) > now + 9 * 365 * 86_400);
    }

    #[test]
    fn the_switch_saves_flag_and_new_certificate_together_and_undoes_both_on_failure() {
        let mut s = Settings::default();
        let (old, _) = ensure_tls_identity(&mut s, &names(), |_| true).unwrap();
        let mut seen = Vec::new();
        let new = set_short_lived_core(&mut s, true, &names(), |st| {
            seen.push(st.ui_flags.get(SHORT_LIVED_KEY).copied());
            true
        })
        .unwrap();
        assert_eq!(seen, vec![Some(true)], "one write that already holds the flag");
        assert_ne!(new.ca_pem, old.ca_pem, "switching makes a new certificate");
        assert!(leaf_not_after(&new) <= now_secs() + 825 * 86_400);
        let before_ca = s.ui_texts.get(REMOTE_TLS_CA).cloned();
        assert!(set_short_lived_core(&mut s, false, &names(), |_| false).is_err());
        assert_eq!(s.ui_flags.get(SHORT_LIVED_KEY), Some(&true), "a failed write leaves the flag");
        assert_eq!(s.ui_texts.get(REMOTE_TLS_CA).cloned(), before_ca, "and the certificate");
        let mut fresh = Settings::default();
        assert!(set_short_lived_core(&mut fresh, true, &names(), |_| false).is_err());
        assert!(!fresh.ui_flags.contains_key(SHORT_LIVED_KEY));
        assert!(!fresh.ui_texts.contains_key(REMOTE_TLS_KEY));
    }

    #[test]
    fn a_certificate_that_starts_in_the_future_is_replaced() {
        let day = now_secs().div_euclid(86_400);
        let (y, m, d) = civil_from_days(day + 30);
        let (y2, m2, d2) = civil_from_days(day + 400);
        let future = generate_span(&names(), (y, m, d), (y2, m2, d2)).unwrap();
        assert!(pem_expired(&future.cert_pem, now_secs()));
        let (y, m, d) = civil_from_days(day - 1);
        let ok = generate_span(&names(), (y, m, d), (y2, m2, d2)).unwrap();
        assert!(!pem_expired(&ok.cert_pem, now_secs()));
        assert!(cert_validity(&[0x31, 0x00]).is_none(), "a non-SEQUENCE certificate is refused");
    }

    #[test]
    fn an_expired_stored_identity_is_replaced_never_kept() {
        let mut s = Settings::default();
        let old = generate_span(&names(), (2018, 1, 1), (2020, 1, 1)).unwrap();
        s.ui_texts.insert(REMOTE_TLS_KEY.into(), enc_str(&old.key_pem));
        s.ui_texts.insert(REMOTE_TLS_CERT.into(), old.cert_pem.clone());
        s.ui_texts.insert(REMOTE_TLS_CA.into(), old.ca_pem.clone());
        assert_eq!(load_stored(&s).err(), Some(IdentitySource::Expired));
        let (fresh, source) = ensure_tls_identity(&mut s, &names(), |_| true).unwrap();
        assert_eq!(source, IdentitySource::Expired);
        assert_ne!(fresh.ca_pem, old.ca_pem);
        assert!(leaf_not_after(&fresh) > now_secs());
        // A valid CA with an expired server certificate is replaced too.
        let valid = generate(&names(), false).unwrap();
        s.ui_texts.insert(REMOTE_TLS_KEY.into(), enc_str(&old.key_pem));
        s.ui_texts.insert(REMOTE_TLS_CERT.into(), old.cert_pem.clone());
        s.ui_texts.insert(REMOTE_TLS_CA.into(), valid.ca_pem.clone());
        assert_eq!(load_stored(&s).err(), Some(IdentitySource::Expired));
        // The new identity honours the current lifetime setting.
        s.ui_flags.insert(SHORT_LIVED_KEY.into(), true);
        s.ui_texts.insert(REMOTE_TLS_CA.into(), old.ca_pem.clone());
        let (short, _) = ensure_tls_identity(&mut s, &names(), |_| true).unwrap();
        assert!(leaf_not_after(&short) <= now_secs() + 825 * 86_400);
        assert!(load_stored(&s).is_ok(), "a current identity is kept");
    }

    #[test]
    fn a_certificate_without_a_readable_end_date_counts_as_expired() {
        assert!(pem_expired("not a certificate", 0));
        assert!(cert_not_after(&[0x30, 0x03, 0x02, 0x01]).is_none());
    }
}
