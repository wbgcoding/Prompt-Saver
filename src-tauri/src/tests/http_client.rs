use crate::ffmpeg::download_agent;
use crate::update::body_text;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::mpsc;
use std::time::Duration;

// One-shot local HTTP server: answers the first request with `head` + `body` and hands the
// received request head back through the channel.
fn serve_once(head: &'static str, body: Vec<u8>) -> (String, mpsc::Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let url = format!("http://{}/file", listener.local_addr().expect("addr"));
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let (mut sock, _) = listener.accept().expect("accept");
        let mut req = Vec::new();
        let mut one = [0u8; 1];
        while !req.ends_with(b"\r\n\r\n") {
            if sock.read(&mut one).unwrap_or(0) == 0 {
                break;
            }
            req.push(one[0]);
        }
        let _ = tx.send(String::from_utf8_lossy(&req).into_owned());
        let _ = sock.write_all(
            format!("HTTP/1.1 {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", head, body.len()).as_bytes(),
        );
        let _ = sock.write_all(&body);
    });
    (url, rx)
}

#[test]
fn the_download_agent_sends_the_app_user_agent_and_reads_the_body() {
    let (url, rx) = serve_once("200 OK", b"hello".to_vec());
    let resp = download_agent().get(&url).header("User-Agent", crate::brand::USER_AGENT).call().expect("get");
    let len = resp.headers().get("Content-Length").and_then(|v| v.to_str().ok()).map(str::to_owned);
    assert_eq!(len.as_deref(), Some("5"));
    let mut got = Vec::new();
    resp.into_body().into_reader().read_to_end(&mut got).expect("body");
    assert_eq!(got, b"hello");
    let head = rx.recv_timeout(Duration::from_secs(5)).expect("request seen").to_ascii_lowercase();
    assert!(head.contains(&format!("user-agent: {}", crate::brand::USER_AGENT.to_ascii_lowercase())), "{head}");
}

#[test]
fn the_download_agent_bounds_connect_and_single_reads_but_not_the_whole_body() {
    let agent = download_agent();
    let cfg = agent.config();
    assert_eq!(cfg.timeouts().connect, Some(Duration::from_secs(30)));
    assert_eq!(cfg.timeout_per_read(), Some(Duration::from_secs(60)));
    assert_eq!(cfg.timeouts().global, None);
    assert_eq!(cfg.timeouts().recv_body, None);
}

#[test]
fn an_error_status_is_a_failed_download() {
    let (url, _rx) = serve_once("404 Not Found", b"nope".to_vec());
    assert!(download_agent().get(&url).call().is_err());
}

#[test]
fn a_read_capped_with_take_stops_at_the_cap() {
    let (url, _rx) = serve_once("200 OK", vec![7u8; 4096]);
    let resp = download_agent().get(&url).call().expect("get");
    let mut got = Vec::new();
    resp.into_body().into_reader().take(1000).read_to_end(&mut got).expect("body");
    assert_eq!(got.len(), 1000, "the cap leaves a full-length read for the caller's truncation check");
}

#[test]
fn text_bodies_are_capped_at_ten_mib_and_must_be_utf8() {
    let (url, _rx) = serve_once("200 OK", b"abc".to_vec());
    let ok = download_agent().get(&url).call().expect("get");
    assert_eq!(body_text(ok).as_deref(), Some("abc"));

    let (url, _rx) = serve_once("200 OK", vec![b'a'; 10 * 1024 * 1024 + 1]);
    let big = download_agent().get(&url).call().expect("get");
    assert_eq!(body_text(big), None, "a body over the cap is rejected, not truncated");

    let (url, _rx) = serve_once("200 OK", vec![0xff, 0xfe, 0x41]);
    let bad = download_agent().get(&url).call().expect("get");
    assert_eq!(body_text(bad), None, "invalid UTF-8 is an error, not a repaired string");
}
