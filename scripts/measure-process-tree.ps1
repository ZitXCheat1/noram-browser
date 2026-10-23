param([Parameter(Mandatory=$true)][int]$RootProcessId)
$ErrorActionPreference = 'Stop'
$all = @(Get-CimInstance Win32_Process | Select-Object ProcessId, ParentProcessId)
$ids = [Collections.Generic.HashSet[int]]::new()
[void]$ids.Add($RootProcessId)
do {
  $changed = $false
  foreach ($entry in $all) {
    if ($ids.Contains([int]$entry.ParentProcessId) -and $ids.Add([int]$entry.ProcessId)) { $changed = $true }
  }
} while ($changed)
$working = 0L; $privateBytes = 0L; $cpu = 0.0; $count = 0
foreach ($processId in $ids) {
  $process = Get-Process -Id $processId -ErrorAction SilentlyContinue
  if ($process) {
    $working += $process.WorkingSet64
    $privateBytes += $process.PrivateMemorySize64
    $cpu += $process.TotalProcessorTime.TotalSeconds
    $count++
  }
}
[ordered]@{processCount=$count; workingSetBytes=$working; privateBytes=$privateBytes; cpuSeconds=$cpu} | ConvertTo-Json -Compress
