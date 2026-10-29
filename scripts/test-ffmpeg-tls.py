#!/usr/bin/env python3
"""Test a real sidecar against local trusted/untrusted HTTPS; no external media needed."""

import argparse
import functools
import http.server
import os
from pathlib import Path
import ssl
import subprocess
import tempfile
import threading


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--ffmpeg", required=True, type=Path)
    args = parser.parse_args()
    ffmpeg = args.ffmpeg.resolve(strict=True)
    repo = Path(__file__).resolve().parent.parent
    with tempfile.TemporaryDirectory(prefix="vitra-https-") as temporary:
        directory = Path(temporary)
        for name in ("trusted", "untrusted"):
            subprocess.run(
                ["openssl", "req", "-x509", "-newkey", "rsa:2048", "-nodes",
                 "-keyout", str(directory / f"{name}.key"),
                 "-out", str(directory / f"{name}.pem"), "-days", "1",
                 "-subj", f"/CN=Vitra {name} test",
                 "-addext", "subjectAltName=IP:127.0.0.1,DNS:localhost"],
                check=True, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE,
            )
        subprocess.run(
            [str(ffmpeg), "-nostdin", "-hide_banner", "-loglevel", "error",
             "-f", "lavfi", "-i", "sine=frequency=440:duration=6",
             str(directory / "tone.wav")], check=True,
        )
        handler = functools.partial(http.server.SimpleHTTPRequestHandler, directory=temporary)
        server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), handler)
        context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
        context.load_cert_chain(directory / "trusted.pem", directory / "trusted.key")
        server.socket = context.wrap_socket(server.socket, server_side=True)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        plain = http.server.ThreadingHTTPServer(("127.0.0.1", 0), handler)
        plain_thread = threading.Thread(target=plain.serve_forever, daemon=True)
        plain_thread.start()
        try:
            child_env = os.environ.copy()
            # These requests are strictly local. Do not route them through a cloud proxy.
            for key in ("http_proxy", "https_proxy", "HTTP_PROXY", "HTTPS_PROXY", "ALL_PROXY", "all_proxy", "SSL_CERT_DIR"):
                child_env.pop(key, None)
            child_env.update({
                "SSL_CERT_FILE": str(directory / "trusted.pem"),
                "VITRA_TLS_UNTRUSTED_CA": str(directory / "untrusted.pem"),
                "VITRA_TLS_TEST_URL": f"https://127.0.0.1:{server.server_port}/tone.wav",
                "VITRA_HTTP_TEST_URL": f"http://127.0.0.1:{plain.server_port}/tone.wav",
                "VITRA_FFMPEG": str(ffmpeg),
            })
            result = subprocess.run(
                ["cargo", "test", "--offline", "-p", "vitra",
                 "wave::process::tests::real_https_stream_authenticates_server",
                 "--", "--ignored", "--exact", "--nocapture"],
                cwd=repo, env=child_env, text=True, stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
            )
            print(result.stdout, end="")
            result.check_returncode()
            if "1 passed; 0 failed" not in result.stdout:
                raise RuntimeError("the HTTPS test did not execute")
        finally:
            server.shutdown()
            server.server_close()
            thread.join()
            plain.shutdown()
            plain.server_close()
            plain_thread.join()


if __name__ == "__main__":
    main()
