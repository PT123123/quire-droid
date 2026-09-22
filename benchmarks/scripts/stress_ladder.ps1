param(
    [Parameter(Mandatory = $true)][string]$Exe,
    [string]$Tag = "vg",
    [string]$Out = "",
    [string]$Root = (Join-Path $env:TEMP "quire-stress"),
    [int]$SampleSeconds = 8,
    # The standing harness waits 15 s for a window and 60 s for an exit. Those
    # caps are right for a matrix and wrong for a ladder: past a certain size a
    # late window IS the finding, and a killed process throws away the number
    # that explains it. So the ladder gets its own, wider clocks.
    [int]$WindowTimeoutSeconds = 180,
    [int]$ExitTimeoutSeconds = 120,
    # Idle means quiet, and the standing harness's sample starts two seconds
    # after the window appears. Measured 2026-09-23, that is fine at 10 000 rows
    # and wrong at 50 000+: the seeded session is still being written out, and a
    # 100 000-row page holds ~95 % of a core until t≈45 s and 0.0 % after it. So
    # the ladder waits for the process to go quiet, and publishes how long that
    # wait was (`settle_wait_ms`) instead of pretending the sample is idle.
    [double]$QuietCpuPct = 5,
    [int]$QuietStreakMs = 3000,
    [int]$SettleTimeoutSeconds = 90,
    # rerun a subset without repeating the rest, e.g. -Only S20000,S50000
    [string[]]$Only = @()
)
$ErrorActionPreference = "Stop"
. (Join-Path $PSScriptRoot "redact.ps1")

# Beyond the standing matrix (bench_matrix.ps1). Every arm there tops out at
# 10 000 blocks; this file asks where the curve actually breaks: 20 000 /
# 50 000 / 100 000 rows of the same bench page, a scroll at those sizes, the
# media and code fixtures across a whole page, and a sidebar 5× the matrix's.
#
# The row shape is bench.ps1's plus the two fields only a ladder needs:
#   window_up_ms  - the interval bench.ps1 calls startup_ms (main window handle
#                   present), named here for what it actually waits on,
#   peak_ws_mb    - the high-water working set sampled through the window,
#                   because on a 100 000-row load the end of the sample is not
#                   where the memory was.
#   hung / exited_early - which clock ran out, so "no window" is never silently
#                   the same row as "the process died before it had one",
#   settle_wait_ms / settled - for the idle arms, how long the process took to
#                   stop doing anything and whether it ever did (see above),
#   exit_code       - 0, or which clock ended the process: "refused-to-exit"
#                   (its own --auto-exit horizon passed with it still alive),
#                   "harness-grace" (this script stopped an arm it had already
#                   measured, because the app's timer was parked far away).
$scenes = @(
    @{ label = "$Tag-S20000"; command = "--blocks 20000"; settle = $true; args = @{ Blocks = 20000 } },
    @{ label = "$Tag-S50000"; command = "--blocks 50000"; settle = $true; args = @{ Blocks = 50000 } },
    @{ label = "$Tag-S100000"; command = "--blocks 100000"; settle = $true; args = @{ Blocks = 100000 } },
    @{ label = "$Tag-S50000-F"; command = "--blocks 50000 --scroll"; args = @{ Blocks = 50000; Scroll = $true } },
    @{ label = "$Tag-S50000-F200"; command = "--blocks 50000 --scroll --scroll-step 200"; args = @{ Blocks = 50000; Scroll = $true; ScrollStep = 200 } },
    # The decode pool is capped, so 20 000 picture rows on a 10 000-row page is
    # a scene about how often a raster re-enters the viewport, not about 20 000
    # decodes - which is exactly the assumption this arm exists to test.
    @{ label = "$Tag-S10000-P20000-F200"; command = "--blocks 10000 --pictures 20000 --scroll --scroll-step 200"; args = @{ Blocks = 10000; Pictures = 20000; Scroll = $true; ScrollStep = 200 } },
    @{ label = "$Tag-S10000-C10000"; command = "--blocks 10000 --code 10000"; settle = $true; args = @{ Blocks = 10000; Code = 10000 } },
    @{ label = "$Tag-S10000-C10000-F200"; command = "--blocks 10000 --code 10000 --scroll --scroll-step 200"; args = @{ Blocks = 10000; Code = 10000; Scroll = $true; ScrollStep = 200 } },
    @{ label = "$Tag-SG500"; command = "--page-switch 500"; args = @{ PageSwitch = 500 } }
)

# A filter that matches nothing is not a measurement - the same rule bench_matrix
# enforces, because a typo'd arm name otherwise prints a clean, empty success.
if ($Only.Count -gt 0) {
    $matched = @($scenes | Where-Object { $s = $_; $Only | Where-Object { $s.label -like "*$_*" } })
    if ($matched.Count -eq 0) {
        throw "-Only [$($Only -join ',')] matched none of: $(($scenes | ForEach-Object { $_.label }) -join ' ')"
    }
    $scenes = $matched
    Write-Host "ladder running $($matched.Count) of the defined arms"
}

function Clear-Database([string]$path) {
    foreach ($stale in @($path, "$path-wal", "$path-shm",
        "$path.bak1", "$path.bak2", "$path.bak3", "$path.bak4", "$path.bak5")) {
        if (Test-Path $stale) { Remove-Item $stale -Force }
    }
}

# Working set and private bytes are read here rather than at the sample's end,
# because on the arms that matter the peak is not where the sample finishes.
function Update-Peak($proc, [ref]$peakWS, [ref]$peakPriv) {
    $proc.Refresh()
    if ($proc.WorkingSet64 -gt $peakWS.Value) { $peakWS.Value = $proc.WorkingSet64 }
    if ($proc.PrivateMemorySize64 -gt $peakPriv.Value) { $peakPriv.Value = $proc.PrivateMemorySize64 }
}

if (-not (Test-Path $Root)) { New-Item -ItemType Directory -Path $Root | Out-Null }
# A leftover attachment pool would let the measured pass re-use files this arm
# never made - the guard bench_matrix.ps1 already carries.
$attPool = Join-Path $Root "attachments"
if (Test-Path $attPool) { Remove-Item -Path $attPool -Recurse -Force }

foreach ($scene in $scenes) {
    $db = Join-Path $Root "$($scene.label).db"
    Clear-Database $db
    # Two passes, same rule as the matrix: the seed pass starts from nothing and
    # therefore times the WRITE; the labelled pass loads what that pass wrote.
    # Nothing is deleted between them - that is what makes the second one a load.
    foreach ($pass in @("$($scene.label)-seed", $scene.label)) {
        $settle = [bool]$scene.settle
        # The app's own quit timer counts from launch, so on an arm that waits
        # for quiet the horizon has to cover the wait - otherwise the process
        # leaves before the measurement starts. Busy arms keep the short horizon
        # the matrix uses, because there "it did not reach its own timer" is the
        # finding, not an artefact of the clock this script chose.
        $autoExitSecs = if ($settle) { $SettleTimeoutSeconds + $SampleSeconds + 10 } else { $SampleSeconds + 14 }
        $childArgs = @("--auto-exit", "$autoExitSecs", "--db", $db, "--dump-state")
        if ($scene.args.ContainsKey("Blocks")) { $childArgs += @("--blocks", "$($scene.args.Blocks)") }
        if ($scene.args.ContainsKey("Scroll")) { $childArgs += @("--scroll") }
        if ($scene.args.ContainsKey("ScrollStep")) { $childArgs += @("--scroll-step", "$($scene.args.ScrollStep)") }
        if ($scene.args.ContainsKey("Pictures")) { $childArgs += @("--pictures", "$($scene.args.Pictures)") }
        if ($scene.args.ContainsKey("Code")) { $childArgs += @("--code", "$($scene.args.Code)") }
        if ($scene.args.ContainsKey("Marks")) { $childArgs += @("--marks", "$($scene.args.Marks)") }
        if ($scene.args.ContainsKey("PageSwitch")) { $childArgs += @("--page-switch", "$($scene.args.PageSwitch)") }

        $errFile = Join-Path $env:TEMP "quire-stress-$pass.err"
        if (Test-Path $errFile) { Remove-Item $errFile }
        $sw = [System.Diagnostics.Stopwatch]::StartNew()
        # The app's own quit timer counts from process start, so the horizon
        # this script compares against has to be measured from the same instant
        # - not from when the window happened to appear.
        $launchedAt = Get-Date
        $p = Start-Process -FilePath $Exe -ArgumentList $childArgs -PassThru -RedirectStandardError $errFile
        $p.EnableRaisingEvents = $true

        $h = [IntPtr]::Zero
        $exitedEarly = $false
        while ($h -eq [IntPtr]::Zero -and $sw.Elapsed.TotalSeconds -lt $WindowTimeoutSeconds) {
            $proc = Get-Process -Id $p.Id -ErrorAction SilentlyContinue
            if ($proc) {
                $h = $proc.MainWindowHandle
                if ($proc.HasExited) { $exitedEarly = $true; break }
            }
            if ($h -eq [IntPtr]::Zero) { Start-Sleep -Milliseconds 50 }
        }
        $windowUpMs = [math]::Round($sw.Elapsed.TotalMilliseconds)
        $hung = ($h -eq [IntPtr]::Zero) -and (-not $exitedEarly)

        $peakWS = [int64]0; $peakPriv = [int64]0

        # Wait for quiet, and report the wait. A "settled":false row says the
        # arm never went quiet inside SettleTimeoutSeconds - which is a result,
        # not a timeout to hide.
        $settleWaitMs = "null"
        $settled = "null"
        if ($settle -and -not $hung -and -not $p.HasExited) {
            Start-Sleep -Milliseconds 500
            $settleStart = Get-Date
            $quietSince = $null
            $prevCpu = $p.TotalProcessorTime
            $prevAt = $settleStart
            while ($true) {
                Start-Sleep -Milliseconds 500
                if ($p.HasExited) { break }
                $p.Refresh()
                if ($p.WorkingSet64 -gt $peakWS) { $peakWS = $p.WorkingSet64 }
                if ($p.PrivateMemorySize64 -gt $peakPriv) { $peakPriv = $p.PrivateMemorySize64 }
                $now = Get-Date
                $spanMs = ($now - $prevAt).TotalMilliseconds
                if ($spanMs -ge 400) {
                    $pct = (($p.TotalProcessorTime - $prevCpu).TotalMilliseconds / $spanMs) * 100
                    $prevCpu = $p.TotalProcessorTime
                    $prevAt = $now
                    if ($pct -le $QuietCpuPct) {
                        if ($null -eq $quietSince) { $quietSince = $now }
                        if ((($now - $quietSince).TotalMilliseconds) -ge $QuietStreakMs) {
                            $settled = "true"
                            break
                        }
                    } else {
                        $quietSince = $null
                    }
                }
                if (($now - $settleStart).TotalSeconds -ge $SettleTimeoutSeconds) { break }
            }
            $settleWaitMs = [math]::Round(((Get-Date) - $settleStart).TotalMilliseconds)
            if ($settled -eq "null") { $settled = "false" }
        }

        # sample: CPU delta over the window, plus the peak inside it
        if (-not $settle) { Start-Sleep -Seconds 2 }
        if ($p.HasExited) {
            $cpuPct = 0; $wsMB = 0; $privMB = 0
            $exitedEarly = $true
        } else {
            $p.Refresh(); $cpu0 = $p.TotalProcessorTime
            $sampleStart = Get-Date
            while (((Get-Date) - $sampleStart).TotalSeconds -lt $SampleSeconds) {
                Start-Sleep -Milliseconds 200
                if ($p.HasExited) { break }
                $p.Refresh()
                if ($p.WorkingSet64 -gt $peakWS) { $peakWS = $p.WorkingSet64 }
                if ($p.PrivateMemorySize64 -gt $peakPriv) { $peakPriv = $p.PrivateMemorySize64 }
            }
            $p.Refresh(); $cpu1 = $p.TotalProcessorTime
            $wall = ((Get-Date) - $sampleStart).TotalSeconds
            $cpuPct = [math]::Round((($cpu1 - $cpu0).TotalMilliseconds / 1000) / $wall * 100, 2)
            $wsMB = [math]::Round($p.WorkingSet64 / 1MB, 1)
            $privMB = [math]::Round($p.PrivateMemorySize64 / 1MB, 1)
        }
        $peakWsMB = [math]::Round($peakWS / 1MB, 1)
        $peakPrivMB = [math]::Round($peakPriv / 1MB, 1)

        $killedBy = ""
        if (-not $p.HasExited) {
            # Which clock should run out first depends on what the arm already
            # proved. An arm that went quiet has nothing left to show and the
            # app's own timer is parked ~90 s away, so stop it early and label
            # the row for it. An arm that never went quiet, or a self-driving
            # arm, must be allowed to reach `--auto-exit` - "it did not reach
            # its own timer" is the finding there.
            $ownTimerAt = $launchedAt.AddSeconds($autoExitSecs + 25)
            $graceSecs = if ($settle -and $settled -eq "true") { 20 } else { ($ownTimerAt - (Get-Date)).TotalSeconds }
            $graceMs = [int][math]::Max(5000, [math]::Min($ExitTimeoutSeconds * 1000, $graceSecs * 1000))
            if (-not $p.WaitForExit($graceMs)) {
                $p.Kill(); [void]$p.WaitForExit()
                $killedBy = if ((Get-Date) -lt $ownTimerAt) { "harness-grace" } else { "refused-to-exit" }
            }
        }
        $ec = if ($killedBy -ne "") { '"' + $killedBy + '"' } else { $p.ExitCode }

        # The arm's own identity: what the app says it built, not what this
        # script's label claims. Plus the decode-cache line a media scene prints.
        $identity = ""
        $cache = "null"
        if (Test-Path $errFile) {
            $lines = @(Get-Content $errFile)
            $dump = $lines | Where-Object { $_ -like 'dump-state:*' } | Select-Object -Last 1
            if ($dump) { $identity = $dump.Trim() }
            $c = $lines | Where-Object { $_ -like '*"event":"attachment_cache"*' } | Select-Object -Last 1
            if ($c) { $cache = $c.Trim() }
            Remove-Item $errFile -Force
        }
        # The database is this scene's mess and the loop head clears it before
        # the next one; deleting it here would turn the measured pass of the
        # next run into a second seed pass.

        $jsonExe = (Redact-MachinePath $Exe) -replace '\\', '\\'
        $jsonDb = (Redact-MachinePath $db) -replace '\\', '\\'
        $jsonIdentity = (Redact-MachinePath $identity) -replace '\\', '\\'
        $jsonIdentity = $jsonIdentity -replace '"', '\"'
        $jsonCommand = $scene.command -replace '"', '\"'
        $line = "{`"label`":`"$pass`",`"exe`":`"$jsonExe`",`"db`":`"$jsonDb`",`"command`":`"$jsonCommand`",`"window_up_ms`":$windowUpMs,`"hung`":$($hung.ToString().ToLower()),`"exited_early`":$($exitedEarly.ToString().ToLower()),`"settle_wait_ms`":$settleWaitMs,`"settled`":$settled,`"idle_cpu_pct`":$cpuPct,`"ram_workingset_mb`":$wsMB,`"ram_private_mb`":$privMB,`"peak_ws_mb`":$peakWsMB,`"peak_private_mb`":$peakPrivMB,`"exit_code`":$ec,`"dump_state`":`"$jsonIdentity`",`"attachment_cache`":$cache}"
        $text = Open-JsonPlaceholders $line
        Write-Output $text
        if ($Out -ne "") { Add-Content -Path $Out -Value $text }
    }
}
