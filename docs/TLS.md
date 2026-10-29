# HTTPS audio analysis

Vitra's release FFmpeg sidecars use OpenSSL. Their build-time certificate directory may
not exist on the user's machine. Vitra supplies trust to child processes only:

- An existing `SSL_CERT_FILE` or `SSL_CERT_DIR` is preserved, including custom proxy roots.
- macOS uses `/etc/ssl/cert.pem`.
- Linux checks the common Debian, Fedora/RHEL, and openSUSE CA bundle paths.
- Windows exports OS roots through `rustls-native-certs` into a random, exclusively created
  temporary PEM file. That small Windows-only dependency reads the native trust store;
  Vitra does not ship a separate list of trusted authorities. The file stays available
  for the lifetime of the process and is reused by subsequent children.

The same configuration is inherited by FFmpeg/ffprobe processes started by yt-dlp
(downloads and post-processing). The loudness meter reads local files only. Stream
analysis explicitly enables `-tls_verify 1` for HTTPS inputs. Plain HTTP inputs must not
receive that HTTPS-only option. Do not work around failures by disabling verification.

Migration 11 requeues previously failed stream decodes without discarding successful
measurements or unrelated file/video errors. New decoder failures are eligible for retry
after five minutes; the background loop's normal scheduling may delay the next attempt.
After five similar decoder failures, Settings → My Wave shows a persistent certificate
or decoder problem and analysis waits five minutes between further attempts. A successful
stream decode clears that problem. Detailed TLS errors are retained in the log rather
than replaced by FFmpeg's final generic I/O error.

Analysis and radio share `LinkService::ytdlp()` with the player. It locates the bundled Deno
and the common runner passes `--js-runtimes deno:<path>`, including analysis without cookies.

## Reproduce locally without external media

After installing the development dependencies, fetch the platform's FFmpeg sidecar:

```sh
npm run fetch-sidecars -- --only ffmpeg
python3 scripts/test-ffmpeg-tls.py --ffmpeg src-tauri/sidecars/aarch64-apple-darwin/ffmpeg
```

On Linux replace the target with `x86_64-unknown-linux-gnu`. The script requires OpenSSL,
Python 3, Cargo, and the Tauri build prerequisites. It runs offline after the initial Rust
build has downloaded dependencies (`cargo test --workspace`). It creates temporary local
HTTP/HTTPS audio servers and runs the real analysis decoder. The assertions require five
seconds of decoded audio with trusted roots, successful plain HTTP, and rejection with
untrusted roots. The CA override applies only to the test subprocess. No system trust is
changed. CI runs this check against the downloaded Linux and macOS sidecars.

## Verify the original issue on macOS

Fetch all sidecars, then run a real YouTube Music link without a shell CA override:

```sh
npm ci
npm run fetch-sidecars
env -u SSL_CERT_FILE -u SSL_CERT_DIR \
  VITRA_YTDLP="$PWD/src-tauri/sidecars/aarch64-apple-darwin/yt-dlp/yt-dlp_macos" \
  VITRA_FFMPEG="$PWD/src-tauri/sidecars/aarch64-apple-darwin/ffmpeg" \
  VITRA_DENO="$PWD/src-tauri/sidecars/aarch64-apple-darwin/deno" \
  VITRA_STREAM_URL='https://music.youtube.com/watch?v=HwmUEkbYgQg' \
  cargo test -p vitra wave::process::tests::real_ytdlp_https_stream -- --ignored --exact --nocapture
```

Keep any custom CA override if your network requires it. This test needs access to the
media site and can fail independently if that video is unavailable, requires sign-in,
or the site throttles requests. Those conditions differ from a TLS failure.

Start `npm run tauri dev`, enable link analysis in Settings → My Wave, and check that
previously failed links gain measurements without deleting the library. Start a mood
radio and check that its links are analyzed too. Verify the same behavior in the packaged
app; macOS UI and actual Windows trust-store behavior need checks on those systems.
