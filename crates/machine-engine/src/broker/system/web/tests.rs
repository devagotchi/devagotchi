//! Real HTTP paging, including servers that ignore Range and an end-of-range answer.
use super::*;
use tokio::{io::{AsyncReadExt, AsyncWriteExt}, net::TcpListener};

async fn server(response: &'static str) -> (String, tokio::task::JoinHandle<String>) {
    let socket = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/page", socket.local_addr().unwrap());
    let task = tokio::spawn(async move {
        let (mut stream, _) = socket.accept().await.unwrap();
        let mut request = Vec::new();
        loop {
            let byte = stream.read_u8().await.unwrap(); request.push(byte);
            if request.ends_with(b"

") { break; }
            assert!(request.len() < 8192);
        }
        stream.write_all(response.as_bytes()).await.unwrap();
        String::from_utf8(request).unwrap()
    });
    (url, task)
}

#[tokio::test]
async fn web_pages_continue_when_range_is_honored_or_ignored_and_end_at_416() {
    for (response, offset, expected, eof) in [
        ("HTTP/1.1 200 OK\r\nContent-Length: 6\r\nConnection: close\r\n\r\nabcdef", 0, "abc", false),
        ("HTTP/1.1 206 Partial Content\r\nContent-Length: 3\r\nContent-Range: bytes 3-5/6\r\nConnection: close\r\n\r\ndef", 3, "def", true),
        ("HTTP/1.1 200 OK\r\nContent-Length: 6\r\nConnection: close\r\n\r\nabcdef", 3, "def", true),
        ("HTTP/1.1 416 Range Not Satisfiable\r\nContent-Length: 0\r\nContent-Range: bytes */6\r\nConnection: close\r\n\r\n", 6, "", true),
    ] {
        let (url, task) = server(response).await;
        let page = read(UrlReadInput { url, offset, max_bytes: Some(3) }).await.unwrap();
        assert_eq!(page.text.as_deref(), Some(expected));
        assert_eq!(page.next_offset, offset + expected.len() as u64);
        assert_eq!(page.eof, eof);
        let request = task.await.unwrap().to_lowercase();
        if offset > 0 { assert!(request.contains(&format!("range: bytes={offset}-"))); }
    }
}

#[tokio::test]
async fn non_web_schemes_and_zero_byte_pages_are_refused_before_io() {
    for (url, max_bytes) in [("file:///not-read", None), ("http://127.0.0.1:1", Some(0))] {
        let error = read(UrlReadInput { url: url.into(), offset: 0, max_bytes }).await.unwrap_err();
        assert_eq!(error.code, "invalid_request");
    }
}
