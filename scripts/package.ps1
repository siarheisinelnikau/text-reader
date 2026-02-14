param([switch]$Offline)
$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path -Parent $PSScriptRoot
Push-Location -LiteralPath $repoRoot
try {
    $previousFlags = $env:RUSTFLAGS
    $env:RUSTFLAGS = "$previousFlags -C target-feature=+crt-static".Trim()
    $buildArgs = @('build', '--release', '--locked', '--target', 'x86_64-pc-windows-msvc', '-j', '2')
    if ($Offline) { $buildArgs += '--offline' }
    & cargo @buildArgs
    if ($LASTEXITCODE -ne 0) { throw 'Cargo release build failed.' }
    $packageData = Get-Content -LiteralPath Cargo.toml -Raw
    $versionMatch = [regex]::Match($packageData, '(?m)^version\s*=\s*"([^"]+)"')
    if (-not $versionMatch.Success) { throw 'Cannot find package version.' }
    $artifactName = "TextReader-v$($versionMatch.Groups[1].Value).exe"
    New-Item -ItemType Directory -Path dist -Force | Out-Null
    $destination = Join-Path $repoRoot "dist/$artifactName"
    Copy-Item -LiteralPath target/x86_64-pc-windows-msvc/release/text-reader.exe -Destination $destination
    Copy-Item -LiteralPath docs/licenses/rust-auto-updator-MIT.txt -Destination dist/LICENSE-rust-auto-updator.txt
    $digest = (Get-FileHash -LiteralPath $destination -Algorithm SHA256).Hash.ToLowerInvariant()
    [System.IO.File]::WriteAllText((Join-Path $repoRoot 'dist/SHA256SUMS.txt'), "$digest  $artifactName`n", [System.Text.UTF8Encoding]::new($false))
    Write-Output "Prepared $destination"
    Write-Output "SHA256: $digest"
} finally {
    $env:RUSTFLAGS = $previousFlags
    Pop-Location
}
