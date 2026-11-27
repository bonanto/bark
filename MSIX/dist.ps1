# Builds Bark for each architecture and collects the files, with the final names, in dist\<arch>\.
# Run it from the folder that holds the workspace Cargo.toml (the one you run `cargo build` in).
param(
    [string[]]$Targets = @('x86_64-pc-windows-msvc', 'i686-pc-windows-msvc', 'aarch64-pc-windows-msvc'),
    [string]$TargetDir = (Join-Path $PSScriptRoot 'target')
)
$ErrorActionPreference = 'Stop'
# triple -> (core dll, ui dll, folder)
$names = @{
    'x86_64-pc-windows-msvc'  = @('sysvol64.dll',  'bui64.dll',  'x64')
    'i686-pc-windows-msvc'    = @('sysvol.dll',    'bui86.dll',  'x86')
    'aarch64-pc-windows-msvc' = @('sysvola64.dll', 'buia64.dll', 'arm64')
}
Push-Location $PSScriptRoot
try {
    foreach ($t in $Targets) {
        $n = $names[$t]
        if (-not $n) { throw "Unknown target $t" }
        rustup target add $t
        cargo build --release --target $t
        if ($LASTEXITCODE -ne 0) { throw "Build failed for $t" }
        $src = Join-Path $TargetDir "$t\release"
        $out = Join-Path $PSScriptRoot "dist\$($n[2])"
        New-Item -ItemType Directory -Force $out | Out-Null
        Copy-Item (Join-Path $src 'bark.exe')  (Join-Path $out 'bark.exe')  -Force
        Copy-Item (Join-Path $src 'sysvol.dll') (Join-Path $out $n[0]) -Force
        Copy-Item (Join-Path $src 'bui.dll')    (Join-Path $out $n[1]) -Force
        Write-Host "OK  $t -> $out"
    }
} finally { Pop-Location }