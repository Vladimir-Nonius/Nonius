//! Окно просмотра по-настоящему: сервер, запросы браузера, ответы.

use std::io::{Read, Write};
use std::net::TcpStream;

use nonius_view::{Page, Viewer};

/// GET-запрос; ответ — (код, тело).
fn get(port: u16, host: &str, path: &str) -> (u16, Vec<u8>) {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    write!(
        stream,
        "GET {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n"
    )
    .unwrap();
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).unwrap();
    let split = raw
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .expect("заголовки ответа");
    let head = String::from_utf8_lossy(&raw[..split]).into_owned();
    let code = head.split(' ').nth(1).unwrap().parse().unwrap();
    (code, raw[split + 4..].to_vec())
}

#[test]
fn serves_page_model_and_three_js() {
    let viewer = Viewer::start(Page {
        title: "кронштейн".into(),
        subtitle: "v1 → v2".into(),
        model: b"glTF model bytes".to_vec(),
    })
    .unwrap();
    let port = viewer.port();
    assert_eq!(viewer.url(), format!("http://127.0.0.1:{port}/"));
    std::thread::spawn(move || viewer.run());

    let host = format!("127.0.0.1:{port}");
    let (code, body) = get(port, &host, "/");
    assert_eq!(code, 200);
    assert!(String::from_utf8(body).unwrap().contains("importmap"));

    let (code, body) = get(port, &host, "/model.glb");
    assert_eq!((code, body.as_slice()), (200, &b"glTF model bytes"[..]));

    let (code, body) = get(port, &format!("localhost:{port}"), "/info.json");
    assert_eq!(code, 200);
    assert!(String::from_utf8(body).unwrap().contains("кронштейн"));

    let (code, body) = get(port, &host, "/three/three.module.min.js");
    assert_eq!(code, 200);
    assert!(body.len() > 100_000);

    assert_eq!(get(port, &host, "/../Cargo.toml").0, 404);
    // Чужое имя хоста — подмена адреса, отказ.
    assert_eq!(
        get(port, &format!("evil.example:{port}"), "/model.glb").0,
        403
    );
}
