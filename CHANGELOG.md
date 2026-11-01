# Changelog

All notable changes to Vitra are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.1] - 2026-11-01

Users of 0.1.0 should update. That version's **Update yt-dlp** trusted `SHA2-256SUMS`
from the same host as the download, and on Windows and Linux it ran `yt-dlp -U`, which
fetches a binary without Vitra's check.

### Security

- **Update yt-dlp** installs a release only after `SHA2-256SUMS.sig` verifies with
  yt-dlp's OpenPGP release key (embedded in the app, fingerprint pinned). The downloaded
  file must match that signed checksum. A failed check leaves the copy already installed.
  There is no unsigned fallback.

### Changed

- In-place `yt-dlp -U` is no longer used on any platform. macOS, Windows, and Linux
  download the published release into the app data `sidecars/` folder (macOS:
  `yt-dlp_macos.zip`; Windows: `yt-dlp.exe`; Linux: `yt-dlp_linux` or
  `yt-dlp_linux_aarch64`). A system yt-dlp is not modified.
- CI checks out with a read-only token, pins third-party Actions to reviewed commits,
  groups weekly Dependabot updates, and runs the workspace and frontend tests on
  Windows. This does not change the installed app.

## [0.1.0] - 2026-10-31

First public test release. See [docs/RELEASE-NOTES.md](docs/RELEASE-NOTES.md) for that
release's description.

[0.1.1]: https://github.com/Fil1825/vitra/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/Fil1825/vitra/releases/tag/v0.1.0

# Журнал изменений

## [0.1.1] — 2026-11-01

Пользователям 0.1.0 следует обновиться. В той версии «Обновить yt-dlp» доверяло файлу
`SHA2-256SUMS` с того же сайта, что и сама загрузка, а в Windows и Linux запускало
`yt-dlp -U`, который скачивает программу без проверки Vitra.

### Безопасность

- «Обновить yt-dlp» ставит выпуск только после проверки подписи `SHA2-256SUMS.sig`
  ключом релиза yt-dlp (OpenPGP; ключ вшит в приложение, отпечаток зафиксирован).
  Скачанный файл должен совпасть с этой подписанной контрольной суммой. Если проверка
  не проходит, уже установленная копия остаётся. Запасного пути без подписи нет.

### Изменения

- Команда `yt-dlp -U` больше не используется ни на одной системе. macOS, Windows и
  Linux скачивают опубликованный выпуск в папку `sidecars` данных приложения (macOS:
  `yt-dlp_macos.zip`; Windows: `yt-dlp.exe`; Linux: `yt-dlp_linux` или
  `yt-dlp_linux_aarch64`). Системный yt-dlp на месте не изменяется.
- В CI токен только на чтение, сторонние Actions закреплены по проверенным коммитам,
  Dependabot группирует еженедельные обновления, а тесты Rust и интерфейса идут на
  Windows. На установленное приложение это не влияет.

## [0.1.0] — 2026-10-31

Первый публичный тестовый выпуск. Описание — в
[docs/RELEASE-NOTES.md](docs/RELEASE-NOTES.md).
