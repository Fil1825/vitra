# Removes the certificate imported by windows-trusted-bundle.ps1 (ephemeral runner cleanup).
param([string]$Thumbprint)
$ErrorActionPreference = 'Stop'
$thumb = $Thumbprint ?? $env:VITRA_WINDOWS_SIGN_THUMBPRINT
if (-not $thumb) { return }
Get-ChildItem Cert:\CurrentUser\My | Where-Object { $_.Thumbprint -eq $thumb } | ForEach-Object {
  Remove-Item $_.PSPath -Force
}
