# FBQueue Automated Test Suite (Windows - Full Core)
$ErrorActionPreference = "Stop"

$FBQ_BIN_REAL = (Resolve-Path "$PSScriptRoot\..\target\debug\fbqueue.exe").Path
$TEST_ROOT = "$PSScriptRoot\tmp"
$env:FBQUEUE_DIR = "" 

Write-Host "FBQueue Binary: $FBQ_BIN_REAL" -ForegroundColor Cyan
Write-Host "Test Root: $TEST_ROOT" -ForegroundColor Cyan

function Stop-All-Daemons {
    $processes = Get-Process fbqueue -ErrorAction SilentlyContinue
    foreach ($p in $processes) {
        Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue
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
    
    Copy-Item $FBQ_BIN_REAL .\fbqueue.exe

    $configContent = @"
capacity: 8
default_queue: batch
queue: batch
  priority: 10
"@
    Set-Content -Path "$env:FBQUEUE_DIR\config" -Value $configContent
    Write-Host "Running Test Case $Global:TestCount... (Dir: case_$Global:TestCount)" -ForegroundColor Yellow
}

function Assert-Content {
    param($Path, $Pattern)
    if (Test-Path $Path) {
        if (Select-String -Path $Path -Pattern $Pattern -Quiet) {
            Write-Host "  [PASS] Found '$Pattern' in $Path." -ForegroundColor Green
            $Global:Passed++
        } else {
            Write-Host "  [FAIL] Could not find '$Pattern' in $Path." -ForegroundColor Red
            $Global:Failed++
        }
    } else {
        Write-Host "  [FAIL] File $Path does not exist." -ForegroundColor Red
        $Global:Failed++
    }
}

# --- Test Cases ---

function Test-Basic-Echo {
    Reset-State
    .\fbqueue.exe sub -N echo cmd /c echo "Hello FBQueue"
    Start-Sleep -Seconds 5
    Assert-Content "echo.o1" "Hello FBQueue"
}

function Test-Delayed-Start {
    Reset-State
    @"
capacity: 1
inactivity_timeout: 5
"@ | Set-Content "$env:FBQUEUE_DIR\config"

    Write-Host "  Submitting job with 5s delay..."
    .\fbqueue.exe sub -a +5s -N delayed_job cmd /c echo "Delayed"
    Start-Sleep -Seconds 2
    $stat = .\fbqueue.exe stat | Out-String
    if ($stat -match "Wait until") {
        Write-Host "  [PASS] Job is waiting." -ForegroundColor Green
        $Global:Passed++
    } else {
        Write-Host "  [FAIL] Job not waiting. Stat: $stat" -ForegroundColor Red
        $Global:Failed++
    }

    Write-Host "  Waiting for job to start..."
    Start-Sleep -Seconds 10
    Assert-Content "delayed_job.o1" "Delayed"
}

function Test-History-And-Archiving {
    Reset-State
    @"
capacity: 10
history_limit: 3
archive_interval_days: 0
"@ | Set-Content "$env:FBQUEUE_DIR\config"

    Write-Host "  Submitting 5 fast jobs..."
    for ($i=1; $i -le 5; $i++) {
        .\fbqueue.exe sub -N job$i cmd /c echo "Job $i"
    }
    Start-Sleep -Seconds 5
    
    Write-Host "  Checking history limit (should keep 3)..."
    $history = .\fbqueue.exe stat -H | Out-String
    $count = ([regex]::Matches($history, "ID:")).Count
    if ($count -eq 3) {
        Write-Host "  [PASS] History limit enforced (kept $count)." -ForegroundColor Green
        $Global:Passed++
    } else {
        Write-Host "  [FAIL] History limit failed (kept $count). Output: $history" -ForegroundColor Red
        $Global:Failed++
    }

    Write-Host "  Waiting for background bundling (archive_interval_days=0)..."
    # Daemon checks pruning/archiving every 10 idle seconds
    Start-Sleep -Seconds 15
    $archiveFiles = Get-ChildItem -Path "$env:FBQUEUE_DIR\archive" -Filter "archive_*.tar.gz"
    if ($archiveFiles.Count -gt 0) {
        Write-Host "  [PASS] Archive created: $($archiveFiles[0].Name)" -ForegroundColor Green
        $Global:Passed++
    } else {
        Write-Host "  [FAIL] Archive not created." -ForegroundColor Red
        Get-ChildItem -Path "$env:FBQUEUE_DIR\archive" -Recurse
        $Global:Failed++
    }
}

# --- Execution ---
try {
    Test-Basic-Echo
    Test-Delayed-Start
    Test-History-And-Archiving
} catch {
    Write-Host "An error occurred: $_" -ForegroundColor Red
    $Global:Failed++
}

Write-Host "-----------------------------------"
$finalColor = if ($Global:Failed -eq 0) { "Green" } else { "Red" }
Write-Host "All Tests Finished: $Global:Passed Passed, $Global:Failed Failed" -ForegroundColor $finalColor
Stop-All-Daemons

if ($Global:Failed -gt 0) { exit 1 }