# FBQueue Automated Test Suite (Windows)
$ErrorActionPreference = "Stop"

# Use relative path from script location to find binary
$FBQ_BIN_REAL = (Resolve-Path "$PSScriptRoot\..	arget\debug\fbqueue.exe").Path
if (-not (Test-Path $FBQ_BIN_REAL)) {
    $FBQ_BIN_REAL = (Resolve-Path "$PSScriptRoot\..	argetelease\fbqueue.exe").Path
}
$TEST_ROOT = "$PSScriptRoot	mp"
$env:FBQUEUE_DIR = "" # Clear initially

Write-Host "FBQueue Binary: $FBQ_BIN_REAL" -ForegroundColor Cyan
Write-Host "Test Root: $TEST_ROOT" -ForegroundColor Cyan

function Stop-All-Daemons {
    $targets = @("fbqueue", "qsub", "qstat", "qdel")
    foreach ($t in $targets) {
        $processes = Get-Process $t -ErrorAction SilentlyContinue
        foreach ($p in $processes) {
            Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue
        }
    }
    Start-Sleep -Seconds 1
}

function Remove-Dir-Force {
    param($Path)
    if (Test-Path $Path) {
        for ($i=0; $i -lt 5; $i++) {
            try {
                Remove-Item -Path $Path -Recurse -Force -ErrorAction Stop
                return
            } catch {
                Start-Sleep -Seconds 1
            }
        }
        Write-Warning "Failed to delete $Path after retries."
    }
}

Remove-Dir-Force $TEST_ROOT
New-Item -ItemType Directory -Path $TEST_ROOT | Out-Null

$Global:Passed = 0
$Global:Failed = 0
$Global:TestCount = 0

function Reset-State {
    $Global:TestCount++
    Stop-All-Daemons
    
    $env:FBQUEUE_DIR = "$TEST_ROOT\case_$Global:TestCount"
    Remove-Dir-Force $env:FBQUEUE_DIR
    New-Item -ItemType Directory -Path $env:FBQUEUE_DIR | Out-Null
    
    $workDir = "$TEST_ROOT\work_$Global:TestCount"
    Remove-Dir-Force $workDir
    New-Item -ItemType Directory -Path $workDir | Out-Null
    Set-Location $workDir
    
    # Create copies for aliases
    Copy-Item $FBQ_BIN_REAL .\fbqueue.exe
    Copy-Item $FBQ_BIN_REAL .\qsub.exe
    Copy-Item $FBQ_BIN_REAL .\qstat.exe
    Copy-Item $FBQ_BIN_REAL .\qdel.exe

    $configContent = @"
capacity: 8
default_queue: batch
queue: batch
  priority: 10
"@
    Set-Content -Path "$env:FBQUEUE_DIR\config" -Value $configContent
    Write-Host "Running Test Case $Global:TestCount... (Dir: case_$Global:TestCount)" -ForegroundColor Yellow
}

function Assert-Exists {
    param($Path)
    if (Test-Path $Path) {
        Write-Host "  [PASS] File(s) $Path exist." -ForegroundColor Green
        $Global:Passed++
    } else {
        Write-Host "  [FAIL] File(s) $Path do not exist." -ForegroundColor Red
        $Global:Failed++
    }
}

function Assert-Content {
    param($Path, $Pattern)
    if (Select-String -Path $Path -Pattern $Pattern -Quiet) {
        Write-Host "  [PASS] Found '$Pattern' in $Path." -ForegroundColor Green
        $Global:Passed++
    } else {
        Write-Host "  [FAIL] Could not find '$Pattern' in $Path." -ForegroundColor Red
        $Global:Failed++
    }
}

# --- Test Cases ---

function Test-Basic-Echo {
    Reset-State
    .\qsub.exe -N echo cmd /c echo "Hello FBQueue"
    Start-Sleep -Seconds 3
    Assert-Content "echo.o1" "Hello FBQueue"
}

function Test-Script-No-X {
    Reset-State
    @"
@echo off
echo Script working
"@ | Set-Content myscript.bat
    .\qsub.exe myscript.bat
    Start-Sleep -Seconds 3
    Assert-Content "myscript.bat.o1" "Script working"
}

function Test-PBS-Directives {
    # PBS is not officially supported on Windows native, but core queue logic should work
    Reset-State
    @"
#PBS -N PbsName
#PBS -l nodes=1:ppn=2
timeout /t 5
"@ | Set-Content pbs_test.bat
    .\qsub.exe pbs_test.bat
    Start-Sleep -Seconds 2
    .\qstat.exe > stat.txt
    Assert-Content "stat.txt" "PbsName"
    .\fbqueue.exe stat --style default > stat_def.txt
    Assert-Content "stat_def.txt" "COST: 2"
}

function Test-Capacity-Limit {
    Reset-State
    @"
capacity: 2
"@ | Set-Content "$env:FBQUEUE_DIR\config"

    .\qsub.exe -N job1 timeout /t 5
    .\qsub.exe -N job2 timeout /t 5
    .\qsub.exe -N job3 timeout /t 5
    Start-Sleep -Seconds 2
    .\qstat.exe > stat.txt
    
    $running = (Select-String " R " stat.txt).Count
    $queued = (Select-String " Q " stat.txt).Count
    
    if ($running -eq 2 -and $queued -eq 1) {
        Write-Host "  [PASS] Resource limit enforced (R:2, Q:1)." -ForegroundColor Green
        $Global:Passed++
    } else {
        Write-Host "  [FAIL] Limit fail. R:$running, Q:$queued. Stat:" -ForegroundColor Red
        cat stat.txt
        $Global:Failed++
    }
}

function Test-Priority-Queue {
    Reset-State
    @"
capacity: 1
default_queue: batch
queue: batch
  priority: 10
queue: express
  priority: 100
"@ | Set-Content "$env:FBQUEUE_DIR\config"

    .\qsub.exe -N blocker timeout /t 3
    .\qsub.exe -q batch -N low_prio cmd /c echo "low"
    .\qsub.exe -q express -N high_prio cmd /c echo "high"
    Start-Sleep -Seconds 10
    
    Assert-Exists "high_prio.o3"
    Assert-Exists "low_prio.o2"
}

function Test-Job-Cancellation {
    Reset-State
    .\qsub.exe -N kill_me timeout /t 20
    Start-Sleep -Seconds 2
    .\qdel.exe 1
    Start-Sleep -Seconds 2
    .\qstat.exe > stat.txt
    if (-not (Select-String "kill_me" stat.txt -Quiet)) {
        if (Test-Path "$env:FBQUEUE_DIR\queue\failed\1.job" -PathType Leaf) {
             Write-Host "  [PASS] Job cancellation verified (failed)." -ForegroundColor Green
             $Global:Passed++
        } elseif (Test-Path "$env:FBQUEUE_DIR\queue\cancel\1.job" -PathType Leaf) {
             Write-Host "  [PASS] Job cancellation verified (cancel)." -ForegroundColor Green
             $Global:Passed++
        } else {
             Write-Host "  [FAIL] Job file missing from failed/cancel." -ForegroundColor Red
             $Global:Failed++
        }
    } else {
        Write-Host "  [FAIL] Job still in qstat output." -ForegroundColor Red
        $Global:Failed++
    }
}

function Test-Daemon-Recovery {
    Reset-State
    .\qsub.exe -N interrupted_job timeout /t 20
    Start-Sleep -Seconds 2
    Stop-All-Daemons
    if (Test-Path "$env:FBQUEUE_DIR\queueunning\1.job") {
        Write-Host "  [PASS] Job state preserved." -ForegroundColor Green
        $Global:Passed++
    } else {
        Write-Host "  [FAIL] Job state lost." -ForegroundColor Red
        $Global:Failed++
    }
    
    .\qstat.exe | Out-Null
    Start-Sleep -Seconds 3
    $stat = .\qstat.exe | Out-String
    if ($stat -match "interrupted_job" -and $stat -match " R ") {
         Write-Host "  [PASS] Job recovered and running." -ForegroundColor Green
         $Global:Passed++
    } else {
         Write-Host "  [FAIL] Recovery failed. Stat:" -ForegroundColor Red
         Write-Host $stat
         $Global:Failed++
    }
}

# --- Execution ---
try {
    Test-Basic-Echo
    Test-Script-No-X
    Test-PBS-Directives
    Test-Capacity-Limit
    Test-Priority-Queue
    Test-Job-Cancellation
    Test-Daemon-Recovery
} catch {
    Write-Host "An error occurred: $_" -ForegroundColor Red
    $Global:Failed++
}

Write-Host "-----------------------------------"
Write-Host "All Tests Finished: $Global:Passed Passed, $Global:Failed Failed" -ForegroundColor $(if ($Global:Failed -eq 0) { "Green" } else { "Red" })
Stop-All-Daemons

if ($Global:Failed -gt 0) { exit 1 }
