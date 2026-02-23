# FBQueue Automated Test Suite (Windows - Core Only)
$ErrorActionPreference = "Stop"

# Use relative path from script location to find binary
$FBQ_BIN_REAL = (Resolve-Path "$PSScriptRoot\..	argetelease\fbqueue.exe" -ErrorAction SilentlyContinue).Path
if (-not $FBQ_BIN_REAL) {
    $FBQ_BIN_REAL = (Resolve-Path "$PSScriptRoot\..	arget\debug\fbqueue.exe").Path
}
$TEST_ROOT = "$PSScriptRoot	mp"
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
    .\fbqueue.exe sub -N echo cmd /c echo "Hello FBQueue"
    Start-Sleep -Seconds 5
    Assert-Content "echo.o1" "Hello FBQueue"
}

function Test-Script-Execution {
    Reset-State
    @"
@echo off
echo Script working
"@ | Set-Content myscript.bat
    .\fbqueue.exe sub myscript.bat
    Start-Sleep -Seconds 5
    Assert-Content "myscript.bat.o1" "Script working"
}

function Test-PowerShell-Execution {
    Reset-State
    @"
Write-Output "PowerShell working"
"@ | Set-Content myscript.ps1
    .\fbqueue.exe sub myscript.ps1
    Start-Sleep -Seconds 7
    Assert-Content "myscript.ps1.o1" "PowerShell working"
}

function Test-Capacity-Limit {
    Reset-State
    @"
capacity: 2
"@ | Set-Content "$env:FBQUEUE_DIR\config"

    .\fbqueue.exe sub -N job1 timeout /t 5
    .\fbqueue.exe sub -N job2 timeout /t 5
    .\fbqueue.exe sub -N job3 timeout /t 5
    Start-Sleep -Seconds 3
    .\fbqueue.exe stat > stat.txt
    
    $stat = Get-Content stat.txt | Out-String
    if ($stat -match "Done: [1-9]" -or $stat -match "ID:.*TIME:") {
        Write-Host "  [PASS] Jobs are being processed." -ForegroundColor Green
        $Global:Passed++
    } else {
        Write-Host "  [FAIL] No jobs seem to be processing." -ForegroundColor Red
        Write-Host $stat
        $Global:Failed++
    }
}

function Test-Job-Cancellation {
    Reset-State
    .\fbqueue.exe sub -N kill_me timeout /t 20
    Start-Sleep -Seconds 2
    .\fbqueue.exe del 1
    Start-Sleep -Seconds 3
    $stat = .\fbqueue.exe stat | Out-String
    if ($stat -notmatch "kill_me") {
         Write-Host "  [PASS] Job cancellation verified." -ForegroundColor Green
         $Global:Passed++
    } else {
        Write-Host "  [FAIL] Job still active." -ForegroundColor Red
        $Global:Failed++
    }
}

# --- Execution ---
try {
    Test-Basic-Echo
    Test-Script-Execution
    Test-PowerShell-Execution
    Test-Capacity-Limit
    Test-Job-Cancellation
} catch {
    Write-Host "An error occurred: $_" -ForegroundColor Red
    $Global:Failed++
}

Write-Host "-----------------------------------"
$finalColor = if ($Global:Failed -eq 0) { "Green" } else { "Red" }
Write-Host "All Tests Finished: $Global:Passed Passed, $Global:Failed Failed" -ForegroundColor $finalColor
Stop-All-Daemons

if ($Global:Failed -gt 0) { exit 1 }
