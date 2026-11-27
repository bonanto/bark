# Build the Bark installer (run AFTER dist.ps1 has filled ..\dist with x86, x64 and arm64).
$ErrorActionPreference = 'Stop'
Set-Location $PSScriptRoot
rustup target add i686-pc-windows-msvc
cargo build --release --target i686-pc-windows-msvc
$exe = 'target\i686-pc-windows-msvc\release\BarkSetup.exe'
New-Item -ItemType Directory -Force ..\dist | Out-Null
Copy-Item $exe ..\dist\BarkSetup-1.0.2.exe -Force
Write-Host "Done: ..\dist\BarkSetup-1.0.2.exe"
