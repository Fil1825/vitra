# Fail closed: requires WINDOWS_CERTIFICATE (base64 PFX) and WINDOWS_CERTIFICATE_PASSWORD.
# Writes a transient Tauri config overlay with certificateThumbprint and prints it to stdout.
$ErrorActionPreference = 'Stop'
if (-not $env:WINDOWS_CERTIFICATE -or -not $env:WINDOWS_CERTIFICATE_PASSWORD) {
  throw 'WINDOWS_CERTIFICATE and WINDOWS_CERTIFICATE_PASSWORD are required for trusted Windows builds'
}
$pfx = Join-Path $env:RUNNER_TEMP 'vitra-codesign.pfx'
[IO.File]::WriteAllBytes($pfx, [Convert]::FromBase64String($env:WINDOWS_CERTIFICATE))
$sec = ConvertTo-SecureString -String $env:WINDOWS_CERTIFICATE_PASSWORD -Force -AsPlainText
$imported = Import-PfxCertificate -FilePath $pfx -Password $sec -CertStoreLocation Cert:\CurrentUser\My
Remove-Item -Force $pfx
$thumb = $imported.Thumbprint
if (-not $thumb) { throw 'PFX import did not return a thumbprint' }
$env:VITRA_WINDOWS_SIGN_THUMBPRINT = $thumb
$overlay = Join-Path $env:RUNNER_TEMP 'tauri-windows-trusted.json'
@{
  bundle = @{
    windows = @{
      certificateThumbprint = $thumb
      timestampUrl          = 'http://timestamp.digicert.com'
      digestAlgorithm       = 'sha256'
    }
  }
} | ConvertTo-Json -Depth 4 | Set-Content -Encoding utf8 $overlay
Write-Output $overlay
