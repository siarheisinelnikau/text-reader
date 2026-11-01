param(
    [Parameter(Mandatory=$true)][string]$Output,
    [ValidateSet('reader','bookmarks','settings','empty')][string]$Screen='reader',
    [int]$Width=1180,
    [int]$Height=820,
    [double]$Scale=1,
    [ValidateSet('en','ru','fr','de','es','it')][string]$Language='en',
    [ValidateSet('light','dark')][string]$Theme='light'
)
$ErrorActionPreference='Stop'
$repoRoot=Split-Path -Parent $PSScriptRoot
$manifest=Get-Content -LiteralPath (Join-Path $repoRoot 'Cargo.toml') -Raw
$version=[regex]::Match($manifest,'(?m)^version\s*=\s*"([^"]+)"').Groups[1].Value
if (-not $version) {throw 'Cannot find package version.'}
$executable=Join-Path $repoRoot "dist/TextReader-v$version.exe"
$outputPath=[System.IO.Path]::GetFullPath($Output,$repoRoot)
if (Test-Path -LiteralPath $outputPath) {throw "Capture already exists: $outputPath"}
New-Item -ItemType Directory -Path (Split-Path -Parent $outputPath) -Force | Out-Null
$captureArgs=@('--screenshot',('"'+$outputPath+'"'),'--screen',$Screen,'--width',"$Width",'--height',"$Height",'--scale',$Scale.ToString([System.Globalization.CultureInfo]::InvariantCulture),'--language',$Language,'--theme',$Theme)
$captureProcess=Start-Process -FilePath $executable -ArgumentList $captureArgs -WorkingDirectory $repoRoot -WindowStyle Hidden -PassThru -RedirectStandardOutput "$outputPath.log" -RedirectStandardError "$outputPath.error.log"
if (-not $captureProcess.WaitForExit(20000)) {
    # This exact disposable render-only process has no document/session side effects.
    Stop-Process -Id $captureProcess.Id
    throw 'The bounded capture timed out.'
}
$captureProcess.WaitForExit()
if ($captureProcess.ExitCode -ne 0 -or -not (Test-Path -LiteralPath $outputPath)) {
    if (Test-Path -LiteralPath "$outputPath.error.log") {Get-Content -LiteralPath "$outputPath.error.log"}
    throw "Capture failed with exit code $($captureProcess.ExitCode)."
}
Write-Output "Captured $Screen and exited: $outputPath"
