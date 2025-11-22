param([string]$Path)
$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path -Parent $PSScriptRoot
if (-not $Path) {
    $manifestText = Get-Content -LiteralPath (Join-Path $repoRoot 'Cargo.toml') -Raw
    $versionMatch = [regex]::Match($manifestText, '(?m)^version\s*=\s*"([^"]+)"')
    if (-not $versionMatch.Success) { throw 'Cannot find package version.' }
    $Path = Join-Path $repoRoot "dist/TextReader-v$($versionMatch.Groups[1].Value).exe"
}
$artifactPath = (Resolve-Path -LiteralPath $Path).Path
$bytes = [System.IO.File]::ReadAllBytes($artifactPath)
if ($bytes.Length -lt 256 -or $bytes[0] -ne 0x4d -or $bytes[1] -ne 0x5a) { throw 'Missing DOS/PE header.' }
$peOffset = [BitConverter]::ToInt32($bytes, 0x3c)
if ([BitConverter]::ToUInt32($bytes, $peOffset) -ne 0x4550) { throw 'Invalid PE signature.' }
$machine = [BitConverter]::ToUInt16($bytes, $peOffset + 4)
$sectionCount = [BitConverter]::ToUInt16($bytes, $peOffset + 6)
$optionalSize = [BitConverter]::ToUInt16($bytes, $peOffset + 20)
$optionalOffset = $peOffset + 24
if ($machine -ne 0x8664 -or [BitConverter]::ToUInt16($bytes, $optionalOffset) -ne 0x20b) { throw 'Expected an x64 PE32+ executable.' }
if ([BitConverter]::ToUInt16($bytes, $optionalOffset + 68) -ne 2) { throw 'Expected the Windows GUI subsystem.' }
$sections = @()
for ($index = 0; $index -lt $sectionCount; $index++) {
    $sectionOffset = $optionalOffset + $optionalSize + 40 * $index
    $sections += [pscustomobject]@{
        VirtualSize = [BitConverter]::ToUInt32($bytes, $sectionOffset + 8)
        Address = [BitConverter]::ToUInt32($bytes, $sectionOffset + 12)
        RawSize = [BitConverter]::ToUInt32($bytes, $sectionOffset + 16)
        RawOffset = [BitConverter]::ToUInt32($bytes, $sectionOffset + 20)
    }
}
function Resolve-Rva([uint32]$Rva) {
    foreach ($section in $sections) {
        if ($Rva -ge $section.Address -and $Rva -lt $section.Address + [Math]::Max($section.VirtualSize, $section.RawSize)) {
            return [int]($section.RawOffset + $Rva - $section.Address)
        }
    }
    throw "Cannot resolve PE RVA $Rva."
}
$importRva = [BitConverter]::ToUInt32($bytes, $optionalOffset + 120)
$descriptor = Resolve-Rva $importRva
$imports = @()
while ([BitConverter]::ToUInt32($bytes, $descriptor + 12) -ne 0) {
    $nameOffset = Resolve-Rva ([BitConverter]::ToUInt32($bytes, $descriptor + 12))
    $nameEnd = $nameOffset
    while ($bytes[$nameEnd] -ne 0) { $nameEnd++ }
    $imports += [System.Text.Encoding]::ASCII.GetString($bytes, $nameOffset, $nameEnd - $nameOffset)
    $descriptor += 20
}
if ($imports | Where-Object { $_ -match '^(VCRUNTIME|MSVCP|api-ms-win-crt)' }) { throw 'Unexpected dynamically imported MSVC CRT.' }
if ($imports | Where-Object { $_ -match '^(opengl32|d3d11|d3d12|dxgi|vulkan-1)\.dll$' }) { throw 'Unexpected GPU renderer import in the CPU/GDI build.' }
$hash = (Get-FileHash -LiteralPath $artifactPath -Algorithm SHA256).Hash.ToLowerInvariant()
$sumsPath = Join-Path (Split-Path -Parent $artifactPath) 'SHA256SUMS.txt'
$expectedLine = "$hash  $(Split-Path -Leaf $artifactPath)"
if ((Get-Content -LiteralPath $sumsPath -Raw).Trim() -ne $expectedLine) { throw 'Package checksum file does not match.' }
Write-Output "Verified PE metadata and checksum without launching: $artifactPath"
Write-Output "Architecture: x64; subsystem: Windows GUI; bytes: $($bytes.Length)"
Write-Output "Imports: $($imports -join ', ')"
