// The receiver cannot be started inside a unit test, so the wiring is checked on
// the source: both places that replace an expired certificate tell the window.
#[test]
fn renewing_an_expired_certificate_announces_it_and_a_save_restarts_the_receiver() {
    let src = include_str!("remote.rs");
    let prep = &src[src.find("fn prepare_acceptor").unwrap()..];
    let expired = &prep[prep.find("IdentitySource::Expired").unwrap()..];
    assert!(expired[..400].contains("announce_tls_renewed(app)"));
    let save = &src[src.find("async fn remote_tls_save_cert").unwrap()..];
    let body = &save[..save.find("let file =").unwrap()];
    assert!(body.contains("restart_receiver(&app)") && body.contains("remote-tls-renewed"));
}

#[test]
fn a_port_change_restarts_the_receiver_only_while_it_runs() {
    let mut on = Settings::default();
    on.ui_flags.insert("remoteApi".into(), true);
    let off = Settings::default();
    assert!(needs_receiver_restart("remotePort", Some(51823.0), 51900.0, &on));
    assert!(needs_receiver_restart("remotePort", None, 51900.0, &on));
    assert!(!needs_receiver_restart("remotePort", Some(51900.0), 51900.0, &on), "same value");
    assert!(!needs_receiver_restart("remotePort", Some(51823.0), 51900.0, &off), "receiver off");
    assert!(!needs_receiver_restart("remoteRateLimit", Some(1.0), 2.0, &on), "other key");
    assert!(!needs_receiver_restart("remotePort", None, 51823.0, &on), "default spelled out");
    assert!(!needs_receiver_restart("remotePort", Some(70000.0), 65535.0, &on), "clamped to the same port");
}

#[test]
fn a_port_held_by_another_socket_is_refused_and_the_old_one_stays() {
    let held = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let busy = held.local_addr().unwrap().port();
    let mut s = Settings::default();
    s.ui_flags.insert("remoteApi".into(), true);
    s.ui_values.insert("remotePort".into(), 51900.0);
    let mut saved = 0;
    let r = commit_port_value(&mut s, busy as f64, probe_port, |_| {
        saved += 1;
        true
    });
    assert_eq!(r, Err(ERR_PORT_UNAVAILABLE.to_string()));
    assert_eq!(s.ui_values.get("remotePort"), Some(&51900.0));
    assert_eq!(saved, 0, "nothing written");
    drop(held);
    let free = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let p = free.local_addr().unwrap().port();
    drop(free);
    assert_eq!(commit_port_value(&mut s, p as f64, probe_port, |_| true), Ok(true));
    assert_eq!(s.ui_values.get("remotePort"), Some(&(p as f64)));
}

#[test]
fn a_failed_write_puts_the_old_port_back() {
    let mut s = Settings::default();
    s.ui_flags.insert("remoteApi".into(), true);
    s.ui_values.insert("remotePort".into(), 51900.0);
    let r = commit_port_value(&mut s, 51901.0, |_, _| Ok(()), |_| false);
    assert_eq!(r, Err(ERR_SAVE_FAILED.to_string()));
    assert_eq!(s.ui_values.get("remotePort"), Some(&51900.0));
    let mut t = Settings::default();
    assert_eq!(commit_port_value(&mut t, 51901.0, |_, _| Err("x".into()), |_| false), Err(ERR_SAVE_FAILED.to_string()));
    assert!(!t.ui_values.contains_key("remotePort"));
}

#[test]
fn a_reset_that_turns_the_receiver_and_the_scheme_off_owes_both_side_effects() {
    let mut old = Settings::default();
    old.ui_flags.insert("remoteApi".into(), true);
    old.ui_flags.insert("urlScheme".into(), true);
    let new = Settings::default();
    assert_eq!(reset_side_effects(&old, &new), ResetSideEffects { stop_receiver: true, unregister_scheme: true });
    assert_eq!(reset_side_effects(&new, &new), ResetSideEffects { stop_receiver: false, unregister_scheme: false });
    // A flag that stays on after the reset is not turned off by it.
    assert_eq!(reset_side_effects(&old, &old), ResetSideEffects { stop_receiver: false, unregister_scheme: false });
}

use super::*;
use std::net::Ipv4Addr;

// ---- password ----

#[test]
fn password_hash_round_trips_and_verifies() {
    let pw = generate_password();
    assert_eq!(pw.len(), 48); // 24 bytes, hex-encoded
    let hash = hash_password(&pw);
    assert!(verify_password(&pw, &hash));
}

#[test]
fn wrong_password_is_rejected() {
    let hash = hash_password("correct horse battery staple");
    assert!(!verify_password("wrong password entirely", &hash));
    // Same length as the real one, proving the check is not just a length shortcut.
    assert!(!verify_password("correct horse battery staplf", &hash));
}

#[test]
fn nothing_verifies_against_an_empty_stored_hash() {
    // With no password configured there is nothing to compare against. handle_connection
    // refuses before calling verify_password, but an empty stored value must also never match.
    assert!(!verify_password("anything", ""));
    assert!(!verify_password("", ""));
}

#[test]
fn constant_time_eq_rejects_different_lengths_without_panic() {
    assert!(!constant_time_eq(b"short", b"a bit longer"));
}

// ---- CIDR / network filter ----

#[test]
fn loopback_is_always_allowed() {
    assert!(peer_allowed(IpAddr::V4(Ipv4Addr::LOCALHOST), false, &[]));
    assert!(peer_allowed(IpAddr::V4(Ipv4Addr::LOCALHOST), true, &[]));
    assert!(peer_allowed("::1".parse().unwrap(), false, &[]));
}

#[test]
fn outside_address_is_refused_when_filter_is_off() {
    // TEST-NET-3 (RFC 5737): a non-loopback address standing in for "outside this machine",
    // never dialled.
    let outside = IpAddr::V4(Ipv4Addr::new(203, 0, 113, 5));
    assert!(!peer_allowed(outside, false, &["203.0.113.0/24".to_string()]));
}

#[test]
fn outside_address_in_range_is_allowed_only_with_filter_on() {
    let peer = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 42));
    let ranges = vec!["192.168.1.0/24".to_string()];
    assert!(!peer_allowed(peer, false, &ranges));
    assert!(peer_allowed(peer, true, &ranges));
}

#[test]
fn address_outside_every_configured_range_is_refused() {
    let peer = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 5));
    let ranges = vec!["192.168.1.0/24".to_string()];
    assert!(!peer_allowed(peer, true, &ranges));
}

#[test]
fn ipv6_peer_never_matches_the_ipv4_only_filter() {
    let peer: IpAddr = "2001:db8::1".parse().unwrap();
    assert!(!peer_allowed(peer, true, &["0.0.0.0/0".to_string()]));
}

#[test]
fn cidr_prefix_zero_matches_everything_v4() {
    let peer = IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8));
    assert!(peer_allowed(peer, true, &["0.0.0.0/0".to_string()]));
}

#[test]
fn range_syntax_validation_rejects_garbage() {
    assert!(validate_range_syntax("192.168.1.0/24, 10.0.0.0/8").is_ok());
    assert!(validate_range_syntax("not-an-address").is_err());
    assert!(validate_range_syntax("192.168.1.0/99").is_err());
    assert!(validate_range_syntax("300.1.1.1/24").is_err());
}

// ---- rate limit + lockout ----

#[test]
fn rate_limit_admits_up_to_the_cap_then_rejects() {
    let guard = RemoteGuard::new();
    let ip = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
    for _ in 0..5 {
        assert!(guard.check_rate(ip, 5));
    }
    assert!(!guard.check_rate(ip, 5));
}

// The slider's rightmost "unlimited" stop is stored as 0 and must skip the throttle.
#[test]
fn a_zero_limit_never_throttles() {
    let guard = RemoteGuard::new();
    let ip = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
    for _ in 0..1000 {
        assert!(guard.check_rate(ip, 0));
    }
}

#[test]
fn a_real_limit_still_blocks_the_request_past_it() {
    let guard = RemoteGuard::new();
    let ip = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
    for _ in 0..60 {
        assert!(guard.check_rate(ip, 60));
    }
    assert!(!guard.check_rate(ip, 60), "the 61st request must be blocked");
}

#[test]
fn ten_failures_lock_the_address_out() {
    let guard = RemoteGuard::new();
    let ip = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
    assert!(!guard.locked(ip));
    let mut triggered = false;
    for _ in 0..10 {
        triggered = guard.record_auth_failure(ip);
    }
    assert!(triggered, "the tenth failure must be the one that reports the lockout");
    assert!(guard.locked(ip));
}

#[test]
fn a_success_clears_the_failure_count() {
    let guard = RemoteGuard::new();
    let ip = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
    for _ in 0..9 {
        guard.record_auth_failure(ip);
    }
    guard.record_auth_success(ip);
    // Nine more failures after a success must not reach the lockout: the counter was reset.
    let mut triggered = false;
    for _ in 0..9 {
        triggered = guard.record_auth_failure(ip);
    }
    assert!(!triggered);
    assert!(!guard.locked(ip));
}

#[test]
fn concurrent_rate_checks_never_lose_a_count() {
    // The shared counter must not be racy: N tasks each register one request and the guard must
    // count exactly N.
    let guard = Arc::new(RemoteGuard::new());
    let ip = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
    let n = 200u32;
    let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(4).build().unwrap();
    let admitted = rt.block_on(async {
        let mut handles = Vec::new();
        for _ in 0..n {
            let g = guard.clone();
            handles.push(tokio::spawn(async move { g.check_rate(ip, n) }));
        }
        let mut admitted = 0u32;
        for h in handles {
            if h.await.unwrap() {
                admitted += 1;
            }
        }
        admitted
    });
    assert_eq!(admitted, n, "every one of the N concurrent requests should have been counted exactly once");
}

// ---- promptsaver:// link parsing ----

#[test]
fn parses_open_and_copy_links() {
    assert!(matches!(parse_scheme_url("promptsaver://open/abc123"), Some(SchemeAction::Open(id)) if id == "abc123"));
    assert!(matches!(parse_scheme_url("promptsaver://copy/abc123"), Some(SchemeAction::Copy(id)) if id == "abc123"));
}

#[test]
fn rejects_unknown_verb_and_write_style_links() {
    assert!(parse_scheme_url("promptsaver://delete/abc123").is_none());
    assert!(parse_scheme_url("promptsaver://edit/abc123").is_none());
    assert!(parse_scheme_url("promptsaver://open/").is_none());
    assert!(parse_scheme_url("promptsaver://open").is_none());
}

#[test]
fn rejects_a_path_smuggled_into_the_id() {
    assert!(parse_scheme_url("promptsaver://open/../../etc/passwd").is_none());
    assert!(parse_scheme_url("promptsaver://open/abc/def").is_none());
    assert!(parse_scheme_url("promptsaver://open/abc?x=1").is_none());
}

#[test]
fn rejects_ids_outside_the_shared_id_pattern() {
    assert!(parse_scheme_url("promptsaver://open/abc def").is_none());
    assert!(parse_scheme_url("promptsaver://open/abc;drop").is_none());
}

#[test]
fn is_copy_link_only_true_for_copy() {
    assert!(is_copy_link("promptsaver://copy/abc123"));
    assert!(!is_copy_link("promptsaver://open/abc123"));
    assert!(!is_copy_link("not-a-link"));
}

#[test]
fn find_scheme_arg_locates_the_url_among_other_args() {
    let args = vec!["--minimized".to_string(), "promptsaver://open/abc123".to_string()];
    assert_eq!(find_scheme_arg(&args), Some("promptsaver://open/abc123"));
    assert_eq!(find_scheme_arg(&["--minimized".to_string()]), None);
}

// ---- request-shape rejection (method, path, cross-site headers) ----

fn shape_req(method: &str, path: &str, headers: &[(&str, &str)]) -> HttpRequest {
    HttpRequest {
        method: method.to_string(),
        path: path.to_string(),
        headers: headers.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
        body: Vec::new(),
    }
}

#[test]
fn a_well_formed_request_passes_the_shape_check() {
    assert!(reject_request_shape(&shape_req("POST", "/api", &[])).is_none());
}

#[test]
fn a_non_post_method_is_rejected() {
    assert!(matches!(reject_request_shape(&shape_req("GET", "/api", &[])), Some(ShapeRejection::Method)));
}

#[test]
fn a_path_other_than_api_is_rejected() {
    assert!(matches!(reject_request_shape(&shape_req("POST", "/", &[])), Some(ShapeRejection::Path)));
}

#[test]
fn a_request_carrying_an_origin_header_is_refused() {
    let req = shape_req("POST", "/api", &[("origin", "https://evil.example")]);
    assert!(matches!(reject_request_shape(&req), Some(ShapeRejection::CrossSite)));
}

#[test]
fn a_request_carrying_a_referer_header_is_refused() {
    let req = shape_req("POST", "/api", &[("referer", "https://evil.example/page")]);
    assert!(matches!(reject_request_shape(&req), Some(ShapeRejection::CrossSite)));
}

// ---- refuses to start without a password ----

#[test]
fn no_stored_password_hash_means_password_hash_hex_returns_none() {
    // The condition start() checks before binding a socket: no password on record, no receiver.
    let settings = Settings::default();
    assert!(password_hash_hex(&settings).is_none());
}

#[test]
fn an_empty_stored_password_hash_also_means_no_password() {
    let mut settings = Settings::default();
    settings.ui_texts.insert("remotePasswordHash".to_string(), String::new());
    assert!(password_hash_hex(&settings).is_none());
}

#[test]
fn a_real_stored_hash_is_found() {
    let mut settings = Settings::default();
    settings.ui_texts.insert("remotePasswordHash".to_string(), hash_password("correct horse battery staple"));
    assert!(password_hash_hex(&settings).is_some());
}

// ---- an id shaped like a SQL injection attempt never reaches the store ----

#[test]
fn a_sql_injection_shaped_id_fails_the_shared_id_pattern() {
    assert!(!crate::cli::valid_id("abc'; DROP TABLE prompts;--"));
    assert!(!crate::cli::valid_id("' OR '1'='1"));
    assert!(parse_scheme_url("promptsaver://open/' OR '1'='1").is_none());
    assert!(parse_scheme_url("promptsaver://copy/abc';DROP TABLE prompts;--").is_none());
}

// ---- HTTP framing ----

#[tokio::test]
async fn oversized_body_is_rejected_without_reading_it_all() {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        read_request(&mut stream).await
    });
    let mut client = TcpStream::connect(addr).await.unwrap();
    let head = format!("POST /api HTTP/1.1\r\nContent-Length: {}\r\n\r\n", MAX_BODY_BYTES + 1);
    client.write_all(head.as_bytes()).await.unwrap();
    let result = server.await.unwrap();
    assert_eq!(result.err(), Some(413));
}

#[tokio::test]
async fn well_formed_request_round_trips() {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        read_request(&mut stream).await
    });
    let mut client = TcpStream::connect(addr).await.unwrap();
    let body = br#"{"argv":["list"]}"#;
    let head = format!(
        "POST /api HTTP/1.1\r\nContent-Length: {}\r\nX-Promptsaver-Password: x\r\n\r\n",
        body.len()
    );
    client.write_all(head.as_bytes()).await.unwrap();
    client.write_all(body).await.unwrap();
    let req = server.await.unwrap().unwrap();
    assert_eq!(req.method, "POST");
    assert_eq!(req.path, "/api");
    assert_eq!(req.headers.get("x-promptsaver-password").map(String::as_str), Some("x"));
    assert_eq!(req.body, body);
}

#[test]
fn wire_body_rejects_unknown_fields() {
    let bad = br#"{"argv":["list"],"extra":true}"#;
    assert!(serde_json::from_slice::<RemoteRequest>(bad).is_err());
}

#[test]
fn wire_body_requires_argv() {
    let bad = br#"{}"#;
    assert!(serde_json::from_slice::<RemoteRequest>(bad).is_err());
}

// ---- durable-or-reversible remote settings ----

fn settings_with(on: bool, hash: Option<&str>) -> Settings {
    let mut s = Settings::default();
    if on {
        s.ui_flags.insert("remoteApi".into(), true);
    }
    if let Some(h) = hash {
        s.ui_texts.insert("remotePasswordHash".into(), h.into());
    }
    s
}

#[test]
fn enabling_with_a_failed_write_restores_flag_and_first_password() {
    let mut s = settings_with(false, None);
    let mut calls = 0;
    let r = apply_remote_switch(&mut s, true, |_| {
        calls += 1;
        false
    });
    assert_eq!(calls, 1);
    assert_eq!(r.action, SwitchAction::None);
    assert!(r.password.is_none());
    assert_eq!(r.error.as_deref(), Some(ERR_SAVE_FAILED));
    assert!(!remote_api_enabled(&s));
    assert!(!s.ui_texts.contains_key("remotePasswordHash"));
}

#[test]
fn enabling_with_a_failed_write_keeps_an_existing_hash() {
    let mut s = settings_with(false, Some("aa"));
    let r = apply_remote_switch(&mut s, true, |_| false);
    assert_eq!(r.action, SwitchAction::None);
    assert_eq!(s.ui_texts.get("remotePasswordHash").map(String::as_str), Some("aa"));
    assert!(!remote_api_enabled(&s));
}

#[test]
fn enabling_with_a_good_write_starts_once_and_shows_the_new_password() {
    let mut s = settings_with(false, None);
    let mut calls = 0;
    let r = apply_remote_switch(&mut s, true, |_| {
        calls += 1;
        true
    });
    assert_eq!(calls, 1);
    assert_eq!(r.action, SwitchAction::Start);
    assert!(r.error.is_none());
    let pw = r.password.expect("first password");
    assert!(verify_password(&pw, &password_hash_hex(&s).unwrap()));
    // A second enable keeps the stored password and mints nothing.
    let r2 = apply_remote_switch(&mut s, true, |_| true);
    assert!(r2.password.is_none());
    assert!(verify_password(&pw, &password_hash_hex(&s).unwrap()));
}

#[test]
fn disabling_stops_even_when_the_write_fails_and_says_so() {
    let mut s = settings_with(true, Some("aa"));
    let r = apply_remote_switch(&mut s, false, |_| false);
    assert_eq!(r.action, SwitchAction::Stop);
    assert_eq!(r.error.as_deref(), Some(ERR_OFF_NOT_SAVED));
    assert!(!remote_api_enabled(&s));
    let mut s2 = settings_with(true, Some("aa"));
    let ok = apply_remote_switch(&mut s2, false, |_| true);
    assert_eq!(ok.action, SwitchAction::Stop);
    assert!(ok.error.is_none());
}

#[test]
fn regenerating_with_a_failed_write_restores_the_old_hash() {
    let mut s = settings_with(true, Some("old-hash"));
    let r = regenerate_password_core(&mut s, |_| false);
    assert_eq!(r.err().as_deref(), Some(ERR_SAVE_FAILED));
    assert_eq!(s.ui_texts.get("remotePasswordHash").map(String::as_str), Some("old-hash"));
}

#[test]
fn regenerating_with_a_good_write_replaces_the_hash_and_returns_the_password() {
    let mut s = settings_with(true, Some("old-hash"));
    let pw = regenerate_password_core(&mut s, |_| true).unwrap();
    assert!(verify_password(&pw, &password_hash_hex(&s).unwrap()));
}

#[test]
fn regenerating_while_off_is_refused_without_saving() {
    let mut s = settings_with(false, Some("old-hash"));
    let mut calls = 0;
    let r = regenerate_password_core(&mut s, |_| {
        calls += 1;
        true
    });
    assert!(r.is_err());
    assert_eq!(calls, 0);
}

#[test]
fn a_failed_range_write_restores_the_old_ranges() {
    let mut s = settings_with(true, Some("aa"));
    s.ui_texts.insert("remoteAllowedRanges".into(), "10.0.0.0/8".into());
    let r = set_allowed_ranges_core(&mut s, "0.0.0.0/0".into(), |_| false);
    assert_eq!(r.err().as_deref(), Some(ERR_SAVE_FAILED));
    assert_eq!(s.ui_texts.get("remoteAllowedRanges").map(String::as_str), Some("10.0.0.0/8"));
    // Never set before: the failed write leaves it unset, not empty.
    let mut fresh = settings_with(true, Some("aa"));
    let _ = set_allowed_ranges_core(&mut fresh, "10.0.0.0/8".into(), |_| false);
    assert!(!fresh.ui_texts.contains_key("remoteAllowedRanges"));
}

#[test]
fn a_good_range_write_lands_and_a_bad_range_never_reaches_the_save() {
    let mut s = settings_with(true, Some("aa"));
    assert!(set_allowed_ranges_core(&mut s, "192.168.1.0/24".into(), |_| true).is_ok());
    assert_eq!(s.ui_texts.get("remoteAllowedRanges").map(String::as_str), Some("192.168.1.0/24"));
    let mut calls = 0;
    let r = set_allowed_ranges_core(&mut s, "nonsense".into(), |_| {
        calls += 1;
        true
    });
    assert!(r.is_err());
    assert_eq!(calls, 0);
    assert_eq!(s.ui_texts.get("remoteAllowedRanges").map(String::as_str), Some("192.168.1.0/24"));
}

// ---- one shutdown per run ----

#[derive(Default)]
struct Probe {
    bound: std::sync::atomic::AtomicUsize,
    finished_current: std::sync::atomic::AtomicUsize,
    finished_stale: std::sync::atomic::AtomicUsize,
}

fn launch(controller: &Arc<RunController>, addr: SocketAddr, probe: &Arc<Probe>) {
    let c = controller.clone();
    let (p1, p2) = (probe.clone(), probe.clone());
    controller.start_run(|ticket| {
        tauri::async_runtime::spawn(serve_run(
            ticket,
            addr,
            c,
            move |_| {
                p1.bound.fetch_add(1, Ordering::SeqCst);
            },
            |_, _| {},
            |_stream, _peer| {},
            move |current| {
                let counter = if current { &p2.finished_current } else { &p2.finished_stale };
                counter.fetch_add(1, Ordering::SeqCst);
            },
        ))
    });
}

async fn free_port() -> u16 {
    let l = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
    l.local_addr().unwrap().port()
}

async fn wait_until(mut cond: impl FnMut() -> bool) -> bool {
    for _ in 0..200 {
        if cond() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    cond()
}

#[tokio::test]
async fn a_stop_while_idle_does_not_kill_the_next_start() {
    let controller = Arc::new(RunController::new());
    controller.stop();
    controller.stop();
    let probe = Arc::new(Probe::default());
    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, free_port().await));
    launch(&controller, addr, &probe);
    assert!(wait_until(|| probe.bound.load(Ordering::SeqCst) == 1).await);
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(probe.finished_current.load(Ordering::SeqCst) + probe.finished_stale.load(Ordering::SeqCst), 0);
    assert!(TcpStream::connect(addr).await.is_ok());
    controller.stop();
    assert!(wait_until(|| probe.finished_current.load(Ordering::SeqCst) == 0 && probe.finished_stale.load(Ordering::SeqCst) == 1).await);
}

#[tokio::test]
async fn rapid_start_stop_start_leaves_one_live_listener_and_a_quiet_old_run() {
    let controller = Arc::new(RunController::new());
    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, free_port().await));
    let first = Arc::new(Probe::default());
    let second = Arc::new(Probe::default());
    launch(&controller, addr, &first);
    controller.stop();
    launch(&controller, addr, &second);
    assert!(wait_until(|| second.bound.load(Ordering::SeqCst) == 1).await);
    // The old run never took the port after the stop, and never published.
    assert!(wait_until(|| first.finished_stale.load(Ordering::SeqCst) == 1).await);
    assert_eq!(first.finished_current.load(Ordering::SeqCst), 0);
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(second.finished_current.load(Ordering::SeqCst) + second.finished_stale.load(Ordering::SeqCst), 0);
    assert!(TcpStream::connect(addr).await.is_ok());
    // One owner: a second bind on the same port is refused while it runs.
    assert!(TcpListener::bind(addr).await.is_err());
    // A restart waits for the running one to release the port, then owns it.
    let third = Arc::new(Probe::default());
    launch(&controller, addr, &third);
    assert!(wait_until(|| third.bound.load(Ordering::SeqCst) == 1).await);
    assert!(wait_until(|| second.finished_stale.load(Ordering::SeqCst) == 1).await);
    assert_eq!(second.finished_current.load(Ordering::SeqCst), 0);
    assert!(TcpStream::connect(addr).await.is_ok());
}

// ---- HTTPS transport ----

use std::sync::atomic::AtomicUsize;
use tokio_rustls::rustls::pki_types::{pem::PemObject, CertificateDer, ServerName};
use tokio_rustls::rustls::{ClientConfig, RootCertStore};

fn test_identity(settings: &mut Settings) -> crate::remote_tls::TlsIdentity {
    let names = vec!["localhost".to_string(), "127.0.0.1".to_string()];
    crate::remote_tls::ensure_tls_identity(settings, &names, |_| true).unwrap().0
}

fn acceptor_for(id: &crate::remote_tls::TlsIdentity) -> TlsAcceptor {
    TlsAcceptor::from(crate::remote_tls::build_server_config(id).unwrap())
}

fn connector_trusting(ca_pem: &str) -> tokio_rustls::TlsConnector {
    let mut roots = RootCertStore::empty();
    for c in CertificateDer::pem_slice_iter(ca_pem.as_bytes()) {
        roots.add(c.unwrap()).unwrap();
    }
    let provider = Arc::new(tokio_rustls::rustls::crypto::ring::default_provider());
    let cfg = ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_root_certificates(roots)
        .with_no_client_auth();
    tokio_rustls::TlsConnector::from(Arc::new(cfg))
}

// A responder built from the receiver's own pieces (handshake, read_request, authorize,
// write_response). `calls` counts requests that were parsed, i.e. got past the handshake.
async fn spawn_responder(acceptor: Option<TlsAcceptor>, stored: Option<String>, calls: Arc<AtomicUsize>) -> SocketAddr {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else { continue };
            let acceptor = acceptor.clone();
            let stored = stored.clone();
            let calls = calls.clone();
            tokio::spawn(async move {
                async fn respond<S: AsyncRead + AsyncWrite + Unpin>(mut s: S, stored: Option<String>, calls: Arc<AtomicUsize>) {
                    let Ok(req) = read_request(&mut s).await else { return };
                    calls.fetch_add(1, Ordering::SeqCst);
                    match authorize(&req, stored.as_deref()) {
                        Ok(()) => write_response(&mut s, 200, b"{}").await,
                        Err(status) => write_response(&mut s, status, b"{}").await,
                    }
                }
                match acceptor {
                    Some(a) => {
                        if let Ok(tls) = tls_accept(&a, stream).await {
                            respond(tls, stored, calls).await;
                        }
                    }
                    None => respond(stream, stored, calls).await,
                }
            });
        }
    });
    addr
}

fn post_with(password: &str) -> Vec<u8> {
    format!("POST /api HTTP/1.1\r\nHost: x\r\nX-Promptsaver-Password: {password}\r\nContent-Length: 2\r\n\r\n{{}}").into_bytes()
}

async fn https_status(connector: &tokio_rustls::TlsConnector, addr: SocketAddr, password: &str) -> Result<u16, String> {
    let tcp = TcpStream::connect(addr).await.map_err(|e| e.to_string())?;
    let mut tls = connector.connect(ServerName::try_from("localhost").unwrap(), tcp).await.map_err(|e| e.to_string())?;
    tls.write_all(&post_with(password)).await.map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    let _ = tls.read_to_end(&mut out).await;
    let text = String::from_utf8_lossy(&out).to_string();
    text.strip_prefix("HTTP/1.1 ").and_then(|r| r.get(..3)).and_then(|c| c.parse().ok()).ok_or_else(|| format!("no status in {text:?}"))
}

#[test]
fn bind_plan_is_loopback_plain_or_everywhere_with_tls() {
    let mut s = Settings::default();
    assert_eq!(bind_plan(&s), (Ipv4Addr::LOCALHOST, false));
    s.ui_flags.insert("remoteNetworkFilter".into(), true);
    assert_eq!(bind_plan(&s), (Ipv4Addr::UNSPECIFIED, true));
}

#[test]
fn authorize_checks_shape_then_password() {
    let hash = hash_password("secret");
    let req = |method: &str, path: &str, pw: Option<&str>, origin: bool| {
        let mut headers = HashMap::new();
        if let Some(p) = pw {
            headers.insert("x-promptsaver-password".to_string(), p.to_string());
        }
        if origin {
            headers.insert("origin".to_string(), "http://evil".to_string());
        }
        HttpRequest { method: method.into(), path: path.into(), headers, body: vec![] }
    };
    assert_eq!(authorize(&req("POST", "/api", Some("secret"), false), Some(&hash)), Ok(()));
    assert_eq!(authorize(&req("POST", "/api", Some("wrong"), false), Some(&hash)), Err(401));
    assert_eq!(authorize(&req("POST", "/api", None, false), Some(&hash)), Err(401));
    assert_eq!(authorize(&req("POST", "/api", Some("secret"), false), None), Err(401));
    assert_eq!(authorize(&req("GET", "/api", Some("secret"), false), Some(&hash)), Err(405));
    assert_eq!(authorize(&req("POST", "/x", Some("secret"), false), Some(&hash)), Err(404));
    assert_eq!(authorize(&req("POST", "/api", Some("secret"), true), Some(&hash)), Err(403));
}

#[tokio::test]
async fn plaintext_to_a_tls_listener_gets_no_http_answer_and_reaches_no_request_code() {
    let mut settings = Settings::default();
    let id = test_identity(&mut settings);
    let calls = Arc::new(AtomicUsize::new(0));
    let addr = spawn_responder(Some(acceptor_for(&id)), Some(hash_password("pw")), calls.clone()).await;
    let mut plain = TcpStream::connect(addr).await.unwrap();
    plain.write_all(&post_with("pw")).await.unwrap();
    let mut out = Vec::new();
    let _ = tokio::time::timeout(Duration::from_secs(5), plain.read_to_end(&mut out)).await;
    assert!(!out.starts_with(b"HTTP/"), "a plaintext client must never get an HTTP answer");
    assert!(!String::from_utf8_lossy(&out).contains("200"));
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn a_client_trusting_the_saved_ca_gets_200_with_the_password_and_401_without() {
    let mut settings = Settings::default();
    let id = test_identity(&mut settings);
    let calls = Arc::new(AtomicUsize::new(0));
    let addr = spawn_responder(Some(acceptor_for(&id)), Some(hash_password("pw")), calls.clone()).await;
    let connector = connector_trusting(&id.ca_pem);
    assert_eq!(https_status(&connector, addr, "pw").await, Ok(200));
    assert_eq!(https_status(&connector, addr, "wrong").await, Ok(401));
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn a_client_trusting_a_different_or_the_old_ca_fails_the_handshake() {
    let mut settings = Settings::default();
    let old = test_identity(&mut settings);
    let names = vec!["localhost".to_string(), "127.0.0.1".to_string()];
    let new = crate::remote_tls::renew_tls_identity(&mut settings, &names, |_| true).unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let addr = spawn_responder(Some(acceptor_for(&new)), Some(hash_password("pw")), calls.clone()).await;
    // The device that trusted the certificate from before the renewal.
    assert!(https_status(&connector_trusting(&old.ca_pem), addr, "pw").await.is_err());
    let mut other = Settings::default();
    let stranger = test_identity(&mut other);
    assert!(https_status(&connector_trusting(&stranger.ca_pem), addr, "pw").await.is_err());
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_eq!(https_status(&connector_trusting(&new.ca_pem), addr, "pw").await, Ok(200));
}

#[tokio::test]
async fn a_stalled_handshake_is_dropped_within_the_limit() {
    let mut settings = Settings::default();
    let id = test_identity(&mut settings);
    let acceptor = acceptor_for(&id);
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
    let addr = listener.local_addr().unwrap();
    let _idle = TcpStream::connect(addr).await.unwrap();
    let (server_side, _) = listener.accept().await.unwrap();
    let started = Instant::now();
    assert!(tls_accept_within(&acceptor, server_side, Duration::from_millis(200)).await.is_err());
    assert!(started.elapsed() < Duration::from_secs(3));
}

#[tokio::test]
async fn a_refused_peer_in_tls_mode_gets_zero_bytes_and_in_plain_mode_a_status() {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
    let addr = listener.local_addr().unwrap();
    for (tls, expect_bytes) in [(true, false), (false, true)] {
        let client = tokio::spawn(async move {
            let mut c = TcpStream::connect(addr).await.unwrap();
            let mut out = Vec::new();
            let _ = tokio::time::timeout(Duration::from_secs(5), c.read_to_end(&mut out)).await;
            out
        });
        let (mut server_side, _) = listener.accept().await.unwrap();
        refuse_before_request(&mut server_side, tls, 403, "locked out").await;
        drop(server_side);
        let out = client.await.unwrap();
        assert_eq!(!out.is_empty(), expect_bytes, "tls={tls}");
        if expect_bytes {
            assert!(out.starts_with(b"HTTP/1.1 403"));
        }
    }
}

// ---- the network switch ----

fn names_for_test() -> Vec<String> {
    vec!["localhost".to_string(), "127.0.0.1".to_string()]
}

#[test]
fn turning_the_network_switch_on_makes_the_identity_then_saves_and_asks_for_a_restart() {
    let mut s = settings_with(true, Some("aa"));
    let r = apply_network_switch(&mut s, true, &names_for_test(), |_| true);
    assert!(r.error.is_none());
    assert!(r.restart);
    assert!(network_filter_enabled(&s));
    assert!(s.ui_texts.get(crate::remote_tls::REMOTE_TLS_KEY).unwrap().starts_with(crate::ENC_PREFIX));
}

#[test]
fn a_failed_final_write_leaves_the_flag_off_in_memory_and_on_disk() {
    let mut s = settings_with(true, Some("aa"));
    // Every write the switch makes: what the disk would hold after each.
    let mut disk_flag_after_each_write = Vec::new();
    let r = apply_network_switch(&mut s, true, &names_for_test(), |st| {
        disk_flag_after_each_write.push(network_filter_enabled(st));
        // The identity write lands, the flag write fails.
        !network_filter_enabled(st)
    });
    assert_eq!(r.error.as_deref(), Some(ERR_SAVE_FAILED));
    assert!(!r.restart);
    assert!(!network_filter_enabled(&s));
    // The only write that succeeded never carried the flag.
    assert_eq!(disk_flag_after_each_write.first(), Some(&false));
}

#[test]
fn a_failing_identity_leaves_the_flag_alone() {
    let mut s = settings_with(true, Some("aa"));
    let r = apply_network_switch(&mut s, true, &names_for_test(), |_| false);
    assert_eq!(r.error.as_deref(), Some(ERR_SAVE_FAILED));
    assert!(!network_filter_enabled(&s));
    assert!(!s.ui_texts.contains_key(crate::remote_tls::REMOTE_TLS_KEY));
}

#[test]
fn narrowing_the_network_switch_always_applies_and_says_when_it_was_not_saved() {
    let mut s = settings_with(true, Some("aa"));
    s.ui_flags.insert("remoteNetworkFilter".into(), true);
    let r = apply_network_switch(&mut s, false, &names_for_test(), |_| false);
    assert!(r.restart, "the receiver must be restarted bound to loopback");
    assert_eq!(r.error.as_deref(), Some(ERR_NETWORK_OFF_NOT_SAVED));
    assert!(!network_filter_enabled(&s));
    let mut s2 = settings_with(true, Some("aa"));
    s2.ui_flags.insert("remoteNetworkFilter".into(), true);
    let ok = apply_network_switch(&mut s2, false, &names_for_test(), |_| true);
    assert!(ok.restart && ok.error.is_none());
}

#[test]
fn the_network_switch_with_the_receiver_off_needs_no_identity_and_no_restart() {
    let mut s = settings_with(false, None);
    let r = apply_network_switch(&mut s, true, &names_for_test(), |_| true);
    assert!(!r.restart && r.error.is_none());
    assert!(!s.ui_texts.contains_key(crate::remote_tls::REMOTE_TLS_KEY));
}

#[test]
fn secret_keys_are_the_password_hash_and_the_certificate_values() {
    assert!(is_remote_secret_key("remotePasswordHash"));
    for k in crate::remote_tls::REMOTE_TLS_KEYS {
        assert!(is_remote_secret_key(k));
    }
    assert!(!is_remote_secret_key("remoteAllowedRanges"));
    assert!(!is_remote_secret_key("remotePort"));
}

#[tokio::test]
async fn a_chain_of_starts_and_stops_never_binds_twice() {
    let controller = Arc::new(RunController::new());
    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, free_port().await));
    let probes: Vec<Arc<Probe>> = (0..4).map(|_| Arc::new(Probe::default())).collect();
    launch(&controller, addr, &probes[0]);
    controller.stop();
    launch(&controller, addr, &probes[1]);
    controller.stop();
    launch(&controller, addr, &probes[2]);
    controller.stop();
    launch(&controller, addr, &probes[3]);
    assert!(wait_until(|| probes[3].bound.load(Ordering::SeqCst) == 1).await);
    let bound_total: usize = probes.iter().map(|p| p.bound.load(Ordering::SeqCst)).sum();
    assert_eq!(bound_total, 1, "only the last run may ever have owned the port");
    assert!(TcpStream::connect(addr).await.is_ok());
    assert!(!controller.current(1) && controller.current(controller.generation.load(Ordering::SeqCst)));
}

// ---- peer table bound ----

#[test]
fn the_peer_table_is_pruned_to_its_bound_once_windows_have_expired() {
    let mut peers: HashMap<IpAddr, PeerState> = HashMap::new();
    for i in 0..5000u32 {
        peers.insert(IpAddr::V4(Ipv4Addr::from(0x0A00_0000 + i)), PeerState::default());
    }
    let later = Instant::now() + Duration::from_secs(RATE_WINDOW_SECS + 60);
    let locked_ip = IpAddr::V4(Ipv4Addr::new(192, 168, 9, 9));
    peers.insert(
        locked_ip,
        PeerState { locked_until: Some(later + Duration::from_secs(600)), ..PeerState::default() },
    );
    prune_peers(&mut peers, later);
    assert!(peers.len() <= MAX_TRACKED_PEERS, "left {}", peers.len());
    assert!(peers.contains_key(&locked_ip), "a peer that is still locked out is kept");
}

#[test]
fn a_small_peer_table_is_left_alone() {
    let mut peers: HashMap<IpAddr, PeerState> = HashMap::new();
    for i in 0..10u32 {
        peers.insert(IpAddr::V4(Ipv4Addr::from(0x0A00_0000 + i)), PeerState::default());
    }
    prune_peers(&mut peers, Instant::now() + Duration::from_secs(RATE_WINDOW_SECS + 60));
    assert_eq!(peers.len(), 10);
}

#[test]
fn busy_peers_inside_their_window_are_capped_oldest_first() {
    let base = Instant::now();
    let mut peers: HashMap<IpAddr, PeerState> = HashMap::new();
    for i in 0..2000u32 {
        peers.insert(
            IpAddr::V4(Ipv4Addr::from(0x0A00_0000 + i)),
            PeerState { window_start: base + Duration::from_millis(u64::from(i)), ..PeerState::default() },
        );
    }
    let locked_ip = IpAddr::V4(Ipv4Addr::new(192, 168, 9, 9));
    peers.insert(
        locked_ip,
        PeerState { window_start: base, locked_until: Some(base + Duration::from_secs(600)), ..PeerState::default() },
    );
    prune_peers(&mut peers, base + Duration::from_secs(1));
    assert_eq!(peers.len(), MAX_TRACKED_PEERS);
    assert!(peers.contains_key(&locked_ip), "a locked-out peer outlives older unlocked ones");
    assert!(!peers.contains_key(&IpAddr::V4(Ipv4Addr::from(0x0A00_0000))), "the oldest window goes first");
    assert!(peers.contains_key(&IpAddr::V4(Ipv4Addr::from(0x0A00_0000 + 1999))), "the newest window stays");
}

// Idle sockets must not pile up without a limit: the 65th open connection gets no slot until
// one of the first ones ends.
#[test]
fn open_connections_are_capped_and_a_slot_returns_when_one_ends() {
    let slots = Arc::new(Semaphore::new(MAX_OPEN_CONNECTIONS));
    let mut held: Vec<_> = (0..MAX_OPEN_CONNECTIONS).map(|_| take_connection_slot(&slots).expect("a free slot")).collect();
    assert!(take_connection_slot(&slots).is_none());
    held.pop();
    assert!(take_connection_slot(&slots).is_some());
}

// A second link must not open a second consent dialog while the first is still open.
#[test]
fn a_second_consent_question_is_refused_while_one_is_open() {
    let flag = AtomicBool::new(false);
    let first = AskingGuard::claim(&flag).expect("the first question may open");
    assert!(AskingGuard::claim(&flag).is_none());
    drop(first);
    assert!(AskingGuard::claim(&flag).is_some());
}
