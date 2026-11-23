# Runs every model 3 times per thread setting, recording CPU load just before each run.
# Usage: pwsh spikes/embeddings/run.ps1 -Cache <dir outside the repo> [-Models bge,minilm] [-Threads 4,2]
param(
  [Parameter(Mandatory)] [string] $Cache,
  [string[]] $Models = @('bge', 'bge-q', 'minilm', 'nomic', 'gemma'),
  [int[]] $Threads = @(4, 2),
  [string] $Out = "$PSScriptRoot/results.jsonl"
)
$ErrorActionPreference = 'Stop'
# `pwsh -File` hands "a,b" over as one string.
$Models = $Models -split ','
$Threads = [int[]]("$Threads" -split '[, ]')
$bin = "$PSScriptRoot/target/release/embeddings-spike.exe"
foreach ($m in $Models) {
  # Untimed first load: downloads the model into the cache. The cache growth is the download size.
  $cacheMb = { if (Test-Path $Cache) { (Get-ChildItem -Recurse -File $Cache | Measure-Object Length -Sum).Sum / 1MB } else { 0 } }
  $before = & $cacheMb
  $dl = Measure-Command { & $bin load $m $Cache | Out-Null }
  Write-Host ("{0} first load incl. download: {1:N0} s, cache grew {2:N0} MB" -f $m, $dl.TotalSeconds, ((& $cacheMb) - $before))
  foreach ($t in $Threads) {
    foreach ($i in 1..3) {
      $load = (Get-Counter '\Processor(_Total)\% Processor Time' -SampleInterval 1 -MaxSamples 3).CounterSamples.CookedValue | Measure-Object -Average
      $line = & $bin run $m $t $Cache
      if ($LASTEXITCODE -ne 0) { throw "$m t=$t run $i failed" }
      $row = $line | ConvertFrom-Json
      $row | Add-Member cpu_load ([math]::Round($load.Average, 1))
      $row | ConvertTo-Json -Compress -Depth 6 | Add-Content $Out
      Write-Host ("{0} t={1} run {2}: load {3:N0}%  cold {4:N0} ms  {5:N1}/{6:N1}/{7:N1} docs/s  peak {8:N0} MB" -f $m, $t, $i, $load.Average, $row.load_ms, $row.buckets[0].docs_per_s, $row.buckets[1].docs_per_s, $row.buckets[2].docs_per_s, $row.peak_ws_mb)
    }
  }
}
