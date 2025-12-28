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
$resourceRva = [BitConverter]::ToUInt32($bytes, $optionalOffset + 128)
if ($resourceRva -eq 0) { throw 'Missing Windows resource directory and application icon.' }
$resourceBase = Resolve-Rva $resourceRva
function Get-ResourceEntries([int]$RelativeOffset) {
    $directory = $resourceBase + $RelativeOffset
    $count = [BitConverter]::ToUInt16($bytes, $directory + 12) + [BitConverter]::ToUInt16($bytes, $directory + 14)
    for ($index = 0; $index -lt $count; $index++) {
        $entryOffset = $directory + 16 + 8 * $index
        $name = [BitConverter]::ToUInt32($bytes, $entryOffset)
        $destination = [BitConverter]::ToUInt32($bytes, $entryOffset + 4)
        [pscustomobject]@{ Name=$name; IsDirectory=(($destination -band [uint32]2147483648) -ne 0); Offset=($destination -band 0x7fffffff) }
    }
}
function Get-ResourceData([int]$RelativeOffset, [int]$Depth=0) {
    if ($Depth -gt 3) { throw 'Unexpected resource directory depth.' }
    foreach ($entry in @(Get-ResourceEntries $RelativeOffset)) {
        if ($entry.IsDirectory) { return Get-ResourceData $entry.Offset ($Depth + 1) }
        $dataEntry = $resourceBase + $entry.Offset
        return [pscustomobject]@{ Offset=(Resolve-Rva ([BitConverter]::ToUInt32($bytes,$dataEntry))); Size=([BitConverter]::ToUInt32($bytes,$dataEntry+4)) }
    }
    throw 'Empty Windows icon resource.'
}
$resourceTypes = @(Get-ResourceEntries 0)
$iconType = @($resourceTypes | Where-Object { $_.Name -eq 3 })
$groupType = @($resourceTypes | Where-Object { $_.Name -eq 14 })
if ($iconType.Count -ne 1 -or $groupType.Count -ne 1) { throw 'Missing RT_ICON or RT_GROUP_ICON resources.' }
$iconImages = @(Get-ResourceEntries $iconType[0].Offset)
$iconGroups = @(Get-ResourceEntries $groupType[0].Offset | Where-Object { $_.Name -eq 1 })
if ($iconGroups.Count -ne 1) { throw 'Missing default application icon group (ID 1).' }
$groupData = Get-ResourceData $iconGroups[0].Offset
if ($groupData.Size -lt 6 -or [BitConverter]::ToUInt16($bytes,$groupData.Offset+2) -ne 1) { throw 'Invalid application icon group.' }
$iconCount = [BitConverter]::ToUInt16($bytes,$groupData.Offset+4)
if ($groupData.Size -ne 6+14*$iconCount) { throw 'Truncated application icon group.' }
$iconSizes = @()
$sourceIcon = [System.IO.File]::ReadAllBytes((Join-Path $repoRoot 'assets/text-reader.ico'))
$sourceIconCount = [BitConverter]::ToUInt16($sourceIcon,4)
$iconHasher = [System.Security.Cryptography.SHA256]::Create()
for ($index = 0; $index -lt $iconCount; $index++) {
    $entry = $groupData.Offset + 6 + 14*$index
    $width = [int]$bytes[$entry]; if ($width -eq 0) { $width=256 }
    $height = [int]$bytes[$entry+1]; if ($height -eq 0) { $height=256 }
    if ($width -ne $height) { throw 'Non-square application icon.' }
    $imageId = [BitConverter]::ToUInt16($bytes,$entry+12)
    $imageEntry = @($iconImages | Where-Object { $_.Name -eq $imageId })
    if ($imageEntry.Count -ne 1) { throw "Missing icon image ID $imageId." }
    $imageData = Get-ResourceData $imageEntry[0].Offset
    if ($imageData.Size -ne [BitConverter]::ToUInt32($bytes,$entry+8)) { throw 'Icon payload size mismatch.' }
    $sourceEntry = @(0..($sourceIconCount-1) | Where-Object { $sourceIcon[6+16*$_] -eq $bytes[$entry] -and $sourceIcon[7+16*$_] -eq $bytes[$entry+1] })
    if ($sourceEntry.Count -ne 1) { throw 'Embedded icon size has no unique source asset.' }
    $sourceOffset = [BitConverter]::ToUInt32($sourceIcon,6+16*$sourceEntry[0]+12)
    $sourceSize = [BitConverter]::ToUInt32($sourceIcon,6+16*$sourceEntry[0]+8)
    $expectedPayload = [byte[]]$sourceIcon[$sourceOffset..($sourceOffset+$sourceSize-1)]
    $actualPayload = [byte[]]$bytes[$imageData.Offset..($imageData.Offset+$imageData.Size-1)]
    if ([BitConverter]::ToString($iconHasher.ComputeHash($expectedPayload)) -ne [BitConverter]::ToString($iconHasher.ComputeHash($actualPayload))) { throw 'Embedded icon pixels do not match the committed icon asset.' }
    $iconSizes += $width
}
$iconHasher.Dispose()
foreach ($requiredSize in @(16,32,48,256)) {
    if ($requiredSize -notin $iconSizes) { throw "Missing Windows icon size $requiredSize." }
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
Write-Output "Embedded application icon: $($iconSizes -join ', ') px (RT_GROUP_ICON 1, $iconCount RT_ICON images)"
