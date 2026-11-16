# Same lifecycle check as elevated-service-test.ps1, but the throwaway service runs a copy of the
# spike binary from %ProgramData%. The first run (16 Nov 2026) failed at "start" with error 5:
# LocalService cannot read files under C:\Users\<name>\, so the SCM could not open the binary.
# The real Bureau service runs as Karthik's own account and can read his profile; this variant
# only proves the rest of the lifecycle under LocalService.
#
# Run in an elevated PowerShell. Leaves %ProgramData%\news-bureau-spike\ (binary copy and
# service.log); the last step prints how to remove it.

$ErrorActionPreference = 'Stop'
$src = Join-Path $PSScriptRoot '..\target\release\bureau-spike.exe' | Resolve-Path
$dir = Join-Path $env:ProgramData 'news-bureau-spike'
New-Item -ItemType Directory -Force $dir | Out-Null
# LocalService may read the binary and write its log here, and nothing else.
icacls $dir /grant '*S-1-5-19:(OI)(CI)M' | Out-Null
$exe = Join-Path $dir 'bureau-spike.exe'
Copy-Item $src $exe -Force
$me = [System.Security.Principal.WindowsIdentity]::GetCurrent().User.Value

function Step($name, [scriptblock]$b) {
    $sw = [Diagnostics.Stopwatch]::StartNew()
    $out = & $b
    "{0,-34} {1,6} ms  {2}" -f $name, $sw.ElapsedMilliseconds, ($out -join ' ')
}

Step 'install (LocalService)' { & $exe service install --name NewsBureauSpike --grant-sid $me }
Step 'start'                  { & $exe service start --name NewsBureauSpike }
Start-Sleep -Milliseconds 800
Step 'status'                 { & $exe service status --name NewsBureauSpike }
Step 'hotline status (R)'     { (& $exe client --cmd status | ConvertFrom-Json).ok }
Step 'hotline run.start as board (H)' { $r = & $exe board --cmd run.start | ConvertFrom-Json; "$($r.ok) $($r.error.reason)" }
Step 'stop'                   { & $exe service stop --name NewsBureauSpike }
Step 'uninstall'              { & $exe service uninstall --name NewsBureauSpike }
if (Test-Path "$dir\service.log") { Get-Content "$dir\service.log" -Tail 8 } else { 'no service.log written' }
"To clean up afterwards: Remove-Item -Recurse -Force '$dir'"
