# Service lifecycle check for the Bureau spike. Karthik runs this once, in an *elevated*
# PowerShell (Run as administrator); the spike agent could not, because installing a service
# needs an administrator token and the agent's terminal is not elevated.
#
# It installs a throwaway service "NewsBureauSpike" under NT AUTHORITY\LocalService (no password),
# starts it, checks that the Hotline answers, stops it and deletes it. Nothing is left behind
# except %ProgramData%\news-bureau-spike\service.log, which records the account, profile and
# environment the SCM gave the service (useful for the real install as Karthik's account).
#
#   cargo build --release -p bureau-spike     (in cli/spikes)
#   .\bureau-spike\elevated-service-test.ps1

$ErrorActionPreference = 'Stop'
$exe = Join-Path $PSScriptRoot '..\target\release\bureau-spike.exe' | Resolve-Path
$me = [System.Security.Principal.WindowsIdentity]::GetCurrent().User.Value

function Step($name, [scriptblock]$b) {
    $sw = [Diagnostics.Stopwatch]::StartNew()
    $out = & $b
    "{0,-34} {1,6} ms  {2}" -f $name, $sw.ElapsedMilliseconds, ($out -join ' ')
}

# The service runs as LocalService, so by default its pipe would admit only LocalService. Grant
# Karthik's SID instead, so this (non-service) terminal can talk to it.
Step 'install (LocalService)' { & $exe service install --name NewsBureauSpike --grant-sid $me }
Step 'status'                 { & $exe service status --name NewsBureauSpike }
Step 'start'                  { & $exe service start --name NewsBureauSpike }
Start-Sleep -Milliseconds 500
Step 'status'                 { & $exe service status --name NewsBureauSpike }
Step 'hotline status (R)'     { (& $exe client --cmd status | ConvertFrom-Json).ok }
Step 'hotline run.start as board (H)' { $r = & $exe board --cmd run.start | ConvertFrom-Json; "$($r.ok) $($r.error.reason)" }
Step 'stop'                   { & $exe service stop --name NewsBureauSpike }
Step 'uninstall'              { & $exe service uninstall --name NewsBureauSpike }
Step 'status after uninstall' { & $exe service status --name NewsBureauSpike }
Get-Content "$env:ProgramData\news-bureau-spike\service.log" -Tail 5
