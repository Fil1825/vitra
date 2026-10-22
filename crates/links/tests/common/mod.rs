//! A tiny HTTP server for tests: serves fixed files on 127.0.0.1.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;

/// A path, its body and its content type.
pub type Route = (&'static str, Vec<u8>, &'static str);

/// Serves the routes `routes(base)` returns (given the base URL, for pages that link to
/// other routes); anything else is a 404. Returns the base URL.
pub fn serve(routes: impl FnOnce(&str) -> Vec<Route>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let routes = routes(&base);
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut request = String::new();
            if reader.read_line(&mut request).is_err() {
                continue;
            }
            // Skip the headers.
            let mut header = String::new();
            while reader.read_line(&mut header).is_ok_and(|n| n > 2) {
                header.clear();
            }
            let path = request.split(' ').nth(1).unwrap_or("/");
            let found = routes.iter().find(|(route, ..)| *route == path);
            let (status, body, kind) = match found {
                Some((_, body, kind)) => ("200 OK", body.as_slice(), *kind),
                None => ("404 Not Found", b"not found".as_slice(), "text/plain"),
            };
            let mut stream = stream;
            let _ = write!(
                stream,
                "HTTP/1.1 {status}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\n\
                 Connection: close\r\n\r\n",
                body.len()
            );
            let _ = stream.write_all(body);
        }
    });
    base
}
