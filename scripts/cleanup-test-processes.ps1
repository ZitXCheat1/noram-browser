param([Parameter(Mandatory=$true)][string]$ProfileRoot)
$ErrorActionPreference = 'Stop'
$root = [IO.Path]::GetFullPath($ProfileRoot).TrimEnd('\')
$temp = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\') + '\'
if (-not $root.StartsWith($temp, [StringComparison]::OrdinalIgnoreCase) -or
    [IO.Path]::GetFileName($root) -notmatch '^noram-(qa|bench)-[a-zA-Z0-9]+$') {
  throw 'Cleanup requires a disposable NoRAM test-profile root inside the temporary directory.'
}
$owned = Get-CimInstance Win32_Process | Where-Object {
  $_.Name -match '^(noram-browser|msedgewebview2|msedge|chrome|brave)\.exe$' -and
  $_.CommandLine -and $_.CommandLine.IndexOf($root, [StringComparison]::OrdinalIgnoreCase) -ge 0
}
foreach ($entry in $owned) { Stop-Process -Id $entry.ProcessId -Force -ErrorAction SilentlyContinue }
