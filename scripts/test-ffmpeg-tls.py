#!/usr/bin/env python3
"""Test a real sidecar against local trusted/untrusted HTTPS; no external media needed.

The trusted PEM is a CA, and the server certificate is signed by it.
macOS runners use LibreSSL. Its system openssl.cnf already adds
basicConstraints, and ``req -addext basicConstraints=...`` appends a second
copy. mbedTLS 3.6.7 rejects that duplicate as -9472
(MBEDTLS_ERR_X509_INVALID_EXTENSIONS). OpenSSL 3 merges the duplicate, which
is why the same commands pass on Linux. Each certificate is minted from a
private config so the system openssl.cnf is not consulted. Verification stays on.
"""

import argparse
import functools
import http.server
import os
from pathlib import Path
import ssl
import subprocess
import tempfile
import threading


def mint_fixture(directory):
    """Write trusted/untrusted CAs and a server certificate signed by the trusted CA."""
    for name in ("trusted", "untrusted"):
        config = directory / f"{name}.cnf"
        config.write_text(
            "[req]\n"
            "distinguished_name = dn\n"
            "x509_extensions = ext\n"
            "prompt = no\n"
            "[dn]\n"
            f"CN = Vitra {name} CA\n"
            "[ext]\n"
            "basicConstraints = critical,CA:TRUE\n"
            "keyUsage = critical,keyCertSign,cRLSign\n"
        )
        subprocess.run(
            ["openssl", "req", "-config", str(config), "-x509",
             "-newkey", "rsa:2048", "-nodes",
             "-keyout", str(directory / f"{name}.key"),
             "-out", str(directory / f"{name}.pem"), "-days", "1"],
            check=True, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE,
        )
    request = directory / "server-req.cnf"
    request.write_text(
        "[req]\n"
        "distinguished_name = dn\n"
        "prompt = no\n"
        "[dn]\n"
        "CN = 127.0.0.1\n"
    )
    subprocess.run(
        ["openssl", "req", "-config", str(request), "-newkey", "rsa:2048",
         "-nodes", "-keyout", str(directory / "server.key"),
         "-out", str(directory / "server.csr")],
        check=True, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE,
    )
    ext = directory / "server.ext"
    ext.write_text(
        "[server]\n"
        "basicConstraints = CA:FALSE\n"
        "subjectAltName = IP:127.0.0.1,DNS:localhost\n"
        "extendedKeyUsage = serverAuth\n"
        "keyUsage = digitalSignature,keyEncipherment\n"
    )
    subprocess.run(
        ["openssl", "x509", "-req",
         "-in", str(directory / "server.csr"),
         "-CA", str(directory / "trusted.pem"),
         "-CAkey", str(directory / "trusted.key"),
         "-CAcreateserial",
         "-out", str(directory / "server.pem"),
         "-days", "1", "-extfile", str(ext), "-extensions", "server"],
        check=True, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE,
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--ffmpeg", required=True, type=Path)
    args = parser.parse_args()
    ffmpeg = args.ffmpeg.resolve(strict=True)
    repo = Path(__file__).resolve().parent.parent
    with tempfile.TemporaryDirectory(prefix="vitra-https-") as temporary:
        directory = Path(temporary)
        mint_fixture(directory)
        subprocess.run(
            [str(ffmpeg), "-nostdin", "-hide_banner", "-loglevel", "error",
             "-f", "lavfi", "-i", "sine=frequency=440:duration=6",
             str(directory / "tone.wav")], check=True,
        )
        handler = functools.partial(http.server.SimpleHTTPRequestHandler, directory=temporary)
        server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), handler)
        context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
        context.load_cert_chain(directory / "server.pem", directory / "server.key")
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
