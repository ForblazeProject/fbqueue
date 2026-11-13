[CmdletBinding()]
param(
    [string]$Binary,
    [switch]$Build,
    [ValidateSet('Development','Released095')][string]$Profile='Development',
    [string]$SourceRoot,
    [string]$OutputDirectory,
    [string[]]$Case=@()
)
$ErrorActionPreference='Stop'
Set-StrictMode -Version 2
# Script automatic variables are not reliable during parameter-default binding.
if ([string]::IsNullOrWhiteSpace($OutputDirectory)) {
    $OutputDirectory=Join-Path $PSScriptRoot 'results'
}
. (Join-Path $PSScriptRoot 'Report.ps1')
$runId=[Guid]::NewGuid().ToString('N')
$reportRoot=Join-Path ([IO.Path]::GetFullPath($OutputDirectory)) ('run-'+$runId)
New-Item -ItemType Directory -Force -Path $reportRoot|Out-Null
$catalog=@('help','batch_cmd_bat','powershell_environment_nodefile','unicode_script_path','failure_exit_code','capacity_fifo','dependency_success','delayed_start','range_list','pbs_explicit_history_filters','windows_rejects_aliases','pending_cancel_history','running_cancel_tree','walltime_tree','windows_daemon_stop_tree','development_validation','first_run_defaults','archive_if_tar_available','harness_containment')
$records=@()
$report=[ordered]@{schema='fbqueue.windows.run.v1'; harness_version=3; run_id=$runId; started_utc=[DateTime]::UtcNow.ToString('o'); finished_utc=$null; profile=$Profile; catalog_count=$catalog.Count; selection=@($Case); binary=$null; binary_sha256=$null; built_from_source=$false; source_manifest_sha256=$null; platform=[Environment]::OSVersion.VersionString; architecture=$env:PROCESSOR_ARCHITECTURE; powershell=$PSVersionTable.PSVersion.ToString(); clr=[Environment]::Version.ToString(); bootstrap_error=$null; scratch=$null; records=@(); summary=$null}
function Log([string]$Message) {
    $line=[DateTime]::UtcNow.ToString('o')+' '+$Message
    Write-Host $Message
    [IO.File]::AppendAllText((Join-Path $reportRoot 'run.log'),($line+"`r`n"),(New-Object Text.UTF8Encoding($false)))
}
function Save-Report {
    $report.records=@($script:records)
    $report.summary=Get-RunVerdict @($script:records)
    if ($report.bootstrap_error) { $report.summary.exit_code=2 }
    $path=Join-Path $reportRoot 'results.json'
    Write-Utf8 ($path+'.partial') ($report|ConvertTo-Json -Depth 12)
    Move-Item -Force ($path+'.partial') $path
}
$report['encoding']=[pscustomobject]@{culture=[Globalization.CultureInfo]::CurrentCulture.Name;ansi_code_page=[Globalization.CultureInfo]::CurrentCulture.TextInfo.ANSICodePage;oem_code_page=[Globalization.CultureInfo]::CurrentCulture.TextInfo.OEMCodePage;raw_streams_preserved=$true;json_stream_decoder='UTF-8 with BOM detection; use raw files for legacy code pages'}
$scratch=$null
$sentinel=$null
$sentinelVerified=$false
try {
    foreach($name in $Case) { if($name -notin $catalog){throw "Unknown case: $name"} }
    $selected=@($catalog|Where-Object { $Case.Count -eq 0 -or $_ -in $Case })
    $script:records=@($selected|ForEach-Object { [pscustomobject]@{name=$_;status='NOT_RUN';message='Not started';seconds=0;cleanup=$null} })
    Save-Report
    if ([Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT) { throw 'Windows 10/11 native execution required. Linux parsing/unit checks are not Windows test results.' }
    Add-Type -Path (Join-Path $PSScriptRoot 'SafeProcess.cs')
    $shell=Join-Path $env:SystemRoot 'System32/WindowsPowerShell/v1.0/powershell.exe'
    if (-not (Test-Path $shell -PathType Leaf)) { throw 'Windows PowerShell powershell.exe was not found' }
    if (-not $SourceRoot) {
        $SourceRoot=if (Test-Path (Join-Path $PSScriptRoot 'source/Cargo.toml')) { Join-Path $PSScriptRoot 'source' } else { Join-Path $PSScriptRoot '../..' }
    }
    $SourceRoot=[IO.Path]::GetFullPath($SourceRoot)
    $report['source_root']=$SourceRoot
    $report['binary_source_relationship']='Not inferred from version; built_from_source records an actual requested build.'
    $manifest=Join-Path $PSScriptRoot 'SOURCE-MANIFEST.json'
    if (Test-Path $manifest) { $report.source_manifest_sha256=(Get-FileHash $manifest -Algorithm SHA256).Hash.ToLowerInvariant() }
    if ($Build) {
        if ($Profile -ne 'Development') { throw '-Build requires the Development profile' }
        if (-not (Test-Path (Join-Path $SourceRoot 'Cargo.toml'))) { throw 'Source Cargo.toml not found' }
        $cargo=Get-Command cargo.exe -CommandType Application -ErrorAction Stop
        $buildTarget='x86_64-pc-windows-msvc'
        $buildDirectory=Join-Path $SourceRoot ('target/windows-test-'+$runId)
        $buildArguments=@('build','--release','--locked','--target',$buildTarget,'--target-dir',$buildDirectory)
        $report['build_argv']=@($cargo.Source)+$buildArguments
        $report['build_output']=$buildDirectory
        Log 'Building the supplied source for x86_64-pc-windows-msvc in a fresh target directory...'
        $buildJob=New-Object FBQWindowsTest.Job
        try {
            $buildExit=$buildJob.Run($cargo.Source,$buildArguments,$SourceRoot,(Join-Path $reportRoot 'build.stdout.txt'),(Join-Path $reportRoot 'build.stderr.txt'),300000)
            $buildJob.StopAndVerify()
            if ($buildExit -ne 0) { throw "Cargo build failed with exit $buildExit; see build.stderr.txt" }
        } finally { $buildJob.Dispose() }
        $Binary=Join-Path $buildDirectory ($buildTarget+'/release/fbqueue.exe')
        $report.built_from_source=$true
    }
    if (-not $Binary) {
        foreach($candidate in @((Join-Path $PSScriptRoot 'fbqueue.exe'),(Join-Path $SourceRoot 'target/release/fbqueue.exe'),(Join-Path $SourceRoot 'target/debug/fbqueue.exe'))) {
            if (Test-Path $candidate -PathType Leaf) { $Binary=$candidate; break }
        }
        if (-not $Binary) { $Binary=(Read-Host 'Enter the full path to fbqueue.exe (or use Build-And-Test.cmd)').Trim().Trim('"') }
    }
    $Binary=(Resolve-Path -LiteralPath $Binary -ErrorAction Stop).Path
    if (-not (Test-Path -LiteralPath $Binary -PathType Leaf) -or [IO.Path]::GetExtension($Binary) -ine '.exe') { throw 'Choose an existing Windows fbqueue.exe, not a Linux binary' }
    $report.binary=$Binary
    $report.binary_sha256=(Get-FileHash -LiteralPath $Binary -Algorithm SHA256).Hash.ToLowerInvariant()
    Log ("Binary: $Binary; SHA256: "+$report.binary_sha256)
    Log "Profile: $Profile (not inferred from version number)"
    $scratch=Join-Path ([IO.Path]::GetTempPath()) ('FBQueue Windows tests '+$runId)
    New-Item -ItemType Directory -Path $scratch|Out-Null
    $report.scratch=$scratch
    # Separate test-owned sentinel proves case cleanup does not kill other jobs.
    Write-Utf8 (Join-Path $scratch 'sentinel.ps1') "[IO.File]::WriteAllText((Join-Path `$PSScriptRoot 'sentinel.ready'),'ready');Start-Sleep -Seconds 1800"
    Write-Utf8 (Join-Path $scratch 'launch-sentinel.ps1') @'
$p=New-Object Diagnostics.ProcessStartInfo
$p.FileName=Join-Path $env:SystemRoot 'System32/WindowsPowerShell/v1.0/powershell.exe'
$p.Arguments='-NoProfile -NonInteractive -ExecutionPolicy Bypass -File "'+(Join-Path $PSScriptRoot 'sentinel.ps1')+'"'
$p.UseShellExecute=$false
$p.CreateNoWindow=$true
[Diagnostics.Process]::Start($p)|Out-Null
'@
    $sentinel=New-Object FBQWindowsTest.Job
    $sentinelExit=$sentinel.Run($shell,@('-NoProfile','-NonInteractive','-ExecutionPolicy','Bypass','-File',(Join-Path $scratch 'launch-sentinel.ps1')),$scratch,(Join-Path $reportRoot 'sentinel.stdout.txt'),(Join-Path $reportRoot 'sentinel.stderr.txt'),20000)
    if ($sentinelExit -ne 0) { throw 'Unrelated sentinel could not start' }
    $timer=[Diagnostics.Stopwatch]::StartNew()
    while (-not (Test-Path (Join-Path $scratch 'sentinel.ready'))) { if($timer.Elapsed.TotalSeconds -gt 15){throw 'Sentinel readiness timed out'}; Start-Sleep -Milliseconds 100 }
    if ($sentinel.ActiveProcesses -lt 1) { throw 'Sentinel exited before tests began' }
    $suiteTimer=[Diagnostics.Stopwatch]::StartNew()
    $index=0
    foreach($entry in $script:records) {
        if ($suiteTimer.Elapsed.TotalSeconds -gt 900) { throw 'Suite exceeded 15 minutes; remaining cases were not run' }
        $index+=1
        $caseRoot=Join-Path $scratch ('{0:d2} {1}' -f $index,$entry.name)
        New-Item -ItemType Directory -Path $caseRoot|Out-Null
        Write-Utf8 (Join-Path $caseRoot 'owner.txt') $runId
        $caseClock=[Diagnostics.Stopwatch]::StartNew()
        $job=$null
        Log ("[$index/$($script:records.Count)] "+$entry.name)
        try {
            $job=New-Object FBQWindowsTest.Job
            $arguments=@('-NoProfile','-NonInteractive','-ExecutionPolicy','Bypass','-File',(Join-Path $PSScriptRoot 'Case-Worker.ps1'),'-Binary',$Binary,'-CaseDirectory',$caseRoot,'-Name',$entry.name,'-Profile',$Profile,'-RunId',$runId)
            $caseExit=$job.Run($shell,$arguments,$caseRoot,(Join-Path $caseRoot 'worker.stdout.txt'),(Join-Path $caseRoot 'worker.stderr.txt'),120000)
            $result=Get-CaseRecord (Join-Path $caseRoot 'case.json') $entry.name $caseExit
            $entry.status=$result.status; $entry.message=$result.message
            $active=$job.ActiveProcesses
            if ($entry.name -eq 'harness_containment' -and $entry.status -eq 'PASS' -and $active -lt 1) { throw 'Containment control did not leave a live child to verify cleanup' }
        } catch { $entry.status='ERROR'; $entry.message=$_.Exception.Message }
        finally {
            if ($job) {
                try {
                    $before=$job.ActiveProcesses
                    $job.StopAndVerify()
                    $entry.cleanup=[pscustomobject]@{active_before=$before; active_after=$job.ActiveProcesses; verified=$true}
                } catch { $entry.status='ERROR'; $entry.message+='; cleanup: '+$_.Exception.Message; $entry.cleanup=[pscustomobject]@{verified=$false} }
                finally { $job.Dispose() }
            }
            $entry.seconds=[Math]::Round($caseClock.Elapsed.TotalSeconds,3)
            $caseReport=Join-Path $reportRoot ('cases/'+$entry.name)
            New-Item -ItemType Directory -Force -Path $caseReport|Out-Null
            # Only curated diagnostics; never copy raw .job files or home/environment dumps.
            foreach($pattern in @('case.json','commands.ndjson','*.stdout.txt','*.stderr.txt')) {
                Get-ChildItem -LiteralPath $caseRoot -Filter $pattern -File|ForEach-Object { Copy-Item -LiteralPath $_.FullName -Destination $caseReport }
            }
            Log ($entry.status+' '+$entry.name+': '+$entry.message)
            Save-Report
        }
        if ($sentinel.ActiveProcesses -lt 1) { $entry.status='ERROR'; $entry.message='Unrelated sentinel died during case cleanup'; throw $entry.message }
        $sentinelVerified=$true
    }
} catch {
    $report.bootstrap_error=$_.Exception.Message
    Log ('ERROR: '+$report.bootstrap_error)
} finally {
    if ($sentinel) {
        try { $report['unrelated_sentinel_survived']=$sentinelVerified -and $sentinel.ActiveProcesses -gt 0; $sentinel.StopAndVerify(); $report['sentinel_cleanup_verified']=($sentinel.ActiveProcesses -eq 0) }
        catch { $report.bootstrap_error='Sentinel cleanup: '+$_.Exception.Message }
        finally { $sentinel.Dispose() }
    }
    $report.finished_utc=[DateTime]::UtcNow.ToString('o')
    Save-Report
    $summary=$report.summary
    Log ("TOTAL=$($summary.total) PASS=$($summary.passed) FAIL=$($summary.failed) SKIP=$($summary.skipped) ERROR=$($summary.errors) NOT_RUN=$($summary.not_run) EXIT=$($summary.exit_code)")
    $zip=Join-Path $reportRoot 'FBQueue-Windows-Results.zip'
    try {
        $items=@(Get-ChildItem -LiteralPath $reportRoot|Where-Object { $_.Name -ne 'FBQueue-Windows-Results.zip' }|ForEach-Object { $_.FullName })
        Compress-Archive -LiteralPath $items -DestinationPath $zip -Force
        Log ("SEND THIS FILE: $zip")
    } catch { $report.bootstrap_error='Report ZIP failed: '+$_.Exception.Message; Save-Report; Log ('Send results.json, run.log and cases/ from '+$reportRoot) }
}
exit $report.summary.exit_code
