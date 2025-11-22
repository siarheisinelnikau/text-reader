param([Parameter(Mandatory=$true)][string]$Output)
$ErrorActionPreference='Stop'
$repoRoot=Split-Path -Parent $PSScriptRoot
$executable=Join-Path $repoRoot 'dist/TextReader-v0.1.0.exe'
$outputPath=[System.IO.Path]::GetFullPath($Output,$repoRoot)
if (Test-Path -LiteralPath $outputPath) {throw "Diagnostic output already exists: $outputPath"}
New-Item -ItemType Directory -Path (Split-Path -Parent $outputPath) -Force | Out-Null
$diagnosticProcess=Start-Process -FilePath $executable -ArgumentList @('--diagnose',('"'+$outputPath+'"')) -WorkingDirectory $repoRoot -WindowStyle Hidden -PassThru -RedirectStandardError "$outputPath.error.log"
Write-Output "Bounded isolated CPU/GDI diagnostic, PID $($diagnosticProcess.Id)"
$clock=[System.Diagnostics.Stopwatch]::StartNew()
$moduleNames=@()
while (-not $diagnosticProcess.HasExited -and $clock.Elapsed.TotalSeconds -lt 28) {
    if ($moduleNames.Count -eq 0 -and $clock.Elapsed.TotalSeconds -gt 2) {
        try { $moduleNames=@($diagnosticProcess.Modules | ForEach-Object { $_.ModuleName }) } catch { }
    }
    Start-Sleep -Milliseconds 500
    $diagnosticProcess.Refresh()
}
if (-not $diagnosticProcess.HasExited) {
    # Only this exact disposable fixture instance can be terminated on timeout.
    Stop-Process -Id $diagnosticProcess.Id
    throw 'Diagnostic exceeded the 28-second external deadline.'
}
$diagnosticProcess.WaitForExit()
if ($diagnosticProcess.ExitCode -ne 0 -or -not (Test-Path -LiteralPath $outputPath)) {throw 'Diagnostic did not complete; inspect its error log. No gate pass is claimed.'}
$report=Get-Content -LiteralPath $outputPath -Raw | ConvertFrom-Json
$report | Add-Member -NotePropertyName 'observed_loaded_modules' -NotePropertyValue $moduleNames
$report | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $outputPath -Encoding utf8
$report.samples | Format-Table state,observed_visible,percent_of_one_core,layouts,rasters,presents,settled
if (@($report.samples | Where-Object { -not $_.settled -or -not $_.state_valid -or $_.percent_of_one_core -gt 5 }).Count -ne 0) {
    throw 'Idle/minimized gate failed. Do not proceed to interactive testing.'
}
Write-Output "Bounded diagnostic completed and exited: $outputPath"
