param([string]$Binary, [string]$CaseDirectory, [string]$Name, [string]$Profile, [string]$RunId)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version 2
. (Join-Path $PSScriptRoot 'Report.ps1')
$clock = [Diagnostics.Stopwatch]::StartNew()
$enteredTest = $false
$record = [ordered]@{schema='fbqueue.windows.case.v1'; name=$Name; status='ERROR'; message='Worker did not finish'; seconds=0; snapshots=@()}
function Need([bool]$Condition, [string]$Message) { if (-not $Condition) { throw $Message } }
function Wait-True([scriptblock]$Predicate, [string]$Message, [int]$Seconds=35) {
    $timer=[Diagnostics.Stopwatch]::StartNew()
    do { if (& $Predicate) { return }; Start-Sleep -Milliseconds 100 } while ($timer.Elapsed.TotalSeconds -lt $Seconds)
    throw "Timeout: $Message"
}
function Text([string]$Path) { return [FBQWindowsTest.Native]::ReadShared($Path) }
function Meta([string]$Path) {
    $m=@{}
    foreach ($line in ((Text $Path) -split "`n")) { if ($line -match '^([^:]+): (.*)$') { $m[$matches[1]]=$matches[2].TrimEnd("`r") } }
    return $m
}
function Configure([int]$Capacity=4) {
    Write-Utf8 (Join-Path $script:state 'config') "capacity: $Capacity`ndefault_queue: batch`ninactivity_timeout: 40`nhistory_limit: 100`narchive_interval_days: 30`nqueue: batch`n  capacity: $Capacity`n  priority: 10`n"
}
function Fixture([string]$FileName, [string]$Body) {
    $path=Join-Path $script:work $FileName
    if ($FileName.EndsWith('.ps1')) { [IO.File]::WriteAllText($path,$Body,(New-Object Text.UTF8Encoding($true))) }
    else { Write-Utf8 $path ($Body -replace "`r?`n","`r`n") }
    return ('.\'+$FileName)
}
function Invoke-Fb([string[]]$Arguments, [string]$Alias='fbqueue') {
    $script:sequence+=1
    # Case-sensitive harness routing lets the FBQUEUE.exe fixture be exercised.
    $exe=if ($Alias -ceq 'fbqueue') { $Binary } else { Join-Path $script:work ($Alias+'.exe') }
    $prefix=Join-Path $CaseDirectory ('command-{0:d3}' -f $script:sequence)
    $start=[pscustomobject]@{event='command_start';sequence=$script:sequence;argv=@($Alias)+$Arguments;utc=[DateTime]::UtcNow.ToString('o')}
    [IO.File]::AppendAllText((Join-Path $CaseDirectory 'commands.ndjson'),(($start|ConvertTo-Json -Compress -Depth 6)+"`n"),(New-Object Text.UTF8Encoding($false)))
    try { $exitCode=[FBQWindowsTest.Native]::RunInherited($exe,$Arguments,$script:work,($prefix+'.stdout.txt'),($prefix+'.stderr.txt'),15000) }
    catch {
        $failure=[pscustomobject]@{event='command_error';sequence=$script:sequence;argv=@($Alias)+$Arguments;exit_code=$null;error=$_.Exception.Message}
        [IO.File]::AppendAllText((Join-Path $CaseDirectory 'commands.ndjson'),(($failure|ConvertTo-Json -Compress -Depth 6)+"`n"),(New-Object Text.UTF8Encoding($false)))
        throw
    }
    $r=[pscustomobject]@{event='command_end';sequence=$script:sequence;argv=@($Alias)+$Arguments;exit_code=$exitCode;stdout=(Text ($prefix+'.stdout.txt'));stderr=(Text ($prefix+'.stderr.txt'))}
    [IO.File]::AppendAllText((Join-Path $CaseDirectory 'commands.ndjson'),(($r|ConvertTo-Json -Compress -Depth 6)+"`n"),(New-Object Text.UTF8Encoding($false)))
    return $r
}
function Submit-Job([string[]]$Options) {
    $r=Invoke-Fb -Arguments (@('sub')+$Options)
    Need ($r.exit_code -eq 0) "Submission failed: $($r.stderr)"
    $ids=[regex]::Matches($r.stdout,'(?m)^\s*(\d+)\.master\s*$')
    Need ($ids.Count -eq 1) "Expected exactly one job ID: $($r.stdout)"
    Wait-True { Test-Path (Join-Path $script:state 'run/daemon.pid') } 'daemon startup metadata'
    return $ids[0].Groups[1].Value
}
function Wait-Record([string]$JobId, [string]$Directory='done') {
    $path=Join-Path $script:state ("queue/$Directory/$JobId.job")
    Wait-True { Test-Path $path } "job $JobId in $Directory"
    return (Meta $path)
}
function Gate-Job {
    return (Fixture 'gate.ps1' @'
[IO.File]::WriteAllText('started.txt','started')
$timer=[Diagnostics.Stopwatch]::StartNew()
while (-not [IO.File]::Exists('release.txt')) { if($timer.Elapsed.TotalSeconds -gt 25){exit 93}; Start-Sleep -Milliseconds 100 }
exit 0
'@)
}
function Prove-Waiting([string]$JobId,[string]$Marker) {
    Wait-True { Test-Path (Join-Path $script:state "queue/new/$JobId.job") } 'pending record'
    $timer=[Diagnostics.Stopwatch]::StartNew()
    while ($timer.Elapsed.TotalSeconds -lt 2) { Need (-not (Test-Path (Join-Path $script:work $Marker))) 'Job started before its constraint was satisfied'; Start-Sleep -Milliseconds 100 }
}
function Heartbeat-Job([string[]]$Options=@()) {
    Fixture 'leaf.ps1' @'
[IO.File]::WriteAllText('leaf.pid',[string]$PID)
for($i=0;$i -lt 300;$i++){[IO.File]::AppendAllText('heartbeat.txt','x');Start-Sleep -Milliseconds 200}
'@ | Out-Null
    $job=Fixture 'tree.ps1' @'
[IO.File]::WriteAllText('root.pid',[string]$PID)
$p=New-Object Diagnostics.ProcessStartInfo
$p.FileName=Join-Path $PSHOME 'powershell.exe'
$p.Arguments='-NoProfile -NonInteractive -ExecutionPolicy Bypass -File .\leaf.ps1'
$p.UseShellExecute=$false
$p.CreateNoWindow=$true
[Diagnostics.Process]::Start($p)|Out-Null
Start-Sleep -Seconds 50
'@
    $jobId=Submit-Job -Options ($Options+@('-N','tree',$job))
    Wait-True { (Test-Path (Join-Path $script:work 'leaf.pid')) -and (Text (Join-Path $script:work 'heartbeat.txt')).Length -ge 2 } 'live child heartbeat'
    $rootProcess=[Diagnostics.Process]::GetProcessById([int](Text (Join-Path $script:work 'root.pid')))
    $leafProcess=[Diagnostics.Process]::GetProcessById([int](Text (Join-Path $script:work 'leaf.pid')))
    $rootProcess.Handle|Out-Null; $leafProcess.Handle|Out-Null
    Need (-not $rootProcess.HasExited -and -not $leafProcess.HasExited) 'Heartbeat processes must be live before termination'
    return [pscustomobject]@{job_id=$jobId; root=$rootProcess; leaf=$leafProcess}
}
function Check-TreeStopped($Tree) {
    Need ($Tree.root.WaitForExit(5000)) 'Payload parent survived termination'
    Need ($Tree.leaf.WaitForExit(5000)) 'Payload descendant survived termination'
    $length=(Text (Join-Path $script:work 'heartbeat.txt')).Length
    Start-Sleep -Milliseconds 800
    Need ((Text (Join-Path $script:work 'heartbeat.txt')).Length -eq $length) 'Heartbeat continued after termination'
}
try {
    Need ([Environment]::OSVersion.Platform -eq [PlatformID]::Win32NT) 'Windows worker required'
    Need (([IO.File]::ReadAllText((Join-Path $CaseDirectory 'owner.txt'))).Trim() -ceq $RunId) 'Private case ownership marker mismatch'
    Add-Type -Path (Join-Path $PSScriptRoot 'SafeProcess.cs')
    Need ([FBQWindowsTest.Native]::InsideJob()) 'Refusing to run outside the test Job Object'
    $script:work=Join-Path $CaseDirectory 'work with spaces'
    $script:state=Join-Path $CaseDirectory 'private state'
    $homePath=Join-Path $CaseDirectory 'private home'
    foreach($directory in @($script:work,$script:state,$homePath,(Join-Path $CaseDirectory 'temp'))) { New-Item -ItemType Directory -Force -Path $directory | Out-Null }
    # Do not persist the user's API keys/session environment in job files.
    $allowed=@('SystemRoot','WINDIR','ComSpec','PATH','PATHEXT','ProgramFiles','ProgramFiles(x86)','ProgramW6432','PROCESSOR_ARCHITECTURE','NUMBER_OF_PROCESSORS','OS','PSModulePath','SystemDrive')
    foreach($key in @([Environment]::GetEnvironmentVariables().Keys)) { if($key -notin $allowed){[Environment]::SetEnvironmentVariable([string]$key,$null,'Process')} }
    foreach($pair in @(@('FBQUEUE_DIR',$script:state),@('HOME',$homePath),@('USERPROFILE',$homePath),@('APPDATA',$homePath),@('LOCALAPPDATA',$homePath),@('TEMP',(Join-Path $CaseDirectory 'temp')),@('TMP',(Join-Path $CaseDirectory 'temp')),@('USERNAME','fbqueue-test'),@('USER','fbqueue-test'),@('FBQUEUE_TEST_MARKER','controlled-value'))) { [Environment]::SetEnvironmentVariable($pair[0],$pair[1],'Process') }
    Set-Location $script:work
    $script:sequence=0
    Configure
    if ($Profile -eq 'Released095' -and $Name -in @('development_validation','pending_cancel_history','first_run_defaults','windows_rejects_aliases')) { throw 'SKIP: unreleased contract; not required of published v0.9.5' }
    $enteredTest=$true
    switch ($Name) {
        'help' {
            $r=Invoke-Fb @('--help')
            Need ($r.exit_code -eq 0 -and $r.stdout -match 'FBQueue.*v[0-9]') 'Missing help/version banner'
            if ($Profile -eq 'Development') { Need ($r.stdout -notmatch 'alias: q' -and $r.stdout -match 'fbqueue\.exe sub') 'Windows help must show explicit .exe commands, not aliases' }
        }
        'batch_cmd_bat' {
            foreach($extension in @('cmd','bat')) {
                $job=Fixture ('job.'+$extension) "@echo off`necho FBQ_STDOUT`necho FBQ_STDERR 1>&2`nexit /b 0`n"
                $jobId=Submit-Job @('-N',$extension,'-o',($extension+'.out'),'-e',($extension+'.err'),$job)
                $m=Wait-Record $jobId
                Need ($m['exit_code'] -eq '0') 'Batch exit code was not zero'
                Need ((Text (Join-Path $script:work ($extension+'.out'))) -match 'FBQ_STDOUT') 'Missing stdout'
                Need ((Text (Join-Path $script:work ($extension+'.err'))) -match 'FBQ_STDERR') 'Missing stderr'
            }
        }
        'powershell_environment_nodefile' {
            $job=Fixture 'environment.ps1' @'
$report=@{id=$env:PBS_JOBID;name=$env:PBS_JOBNAME;queue=$env:PBS_QUEUE;mode=$env:PBS_ENVIRONMENT;marker=$env:FBQUEUE_TEST_MARKER;cwd=$env:PBS_O_WORKDIR}
[IO.File]::WriteAllText('pbs.json',($report|ConvertTo-Json))
[IO.File]::Copy($env:PBS_NODEFILE,'nodefile.copy')
[IO.File]::WriteAllText('nodefile.path',$env:PBS_NODEFILE)
Write-Output 'PS1_OK'
'@
            $jobId=Submit-Job @('-c','3','-N','environment',$job)
            Wait-Record $jobId | Out-Null
            $p=(Text (Join-Path $script:work 'pbs.json'))|ConvertFrom-Json
            Need ($p.id -eq $jobId -and $p.name -eq 'environment' -and $p.queue -eq 'batch' -and $p.mode -eq 'PBS_BATCH' -and $p.marker -eq 'controlled-value' -and $p.cwd -eq $script:work) 'PBS/environment forwarding mismatch'
            $lines=@((Text (Join-Path $script:work 'nodefile.copy')).Trim() -split '\r?\n')
            Need ($lines.Count -eq 3 -and @($lines|Select-Object -Unique).Count -eq 1 -and $lines[0].Length -gt 0) 'Nodefile slot count/content mismatch'
            Need (-not (Test-Path (Text (Join-Path $script:work 'nodefile.path')))) 'Nodefile not removed'
        }
        'unicode_script_path' {
            $job=Fixture ([string][char]0x51e6+[char]0x7406+' file.ps1') "[IO.File]::WriteAllText('unicode.ok','ok')"
            $jobId=Submit-Job @('-N','unicode',$job)
            Wait-Record $jobId | Out-Null
            Need ((Text (Join-Path $script:work 'unicode.ok')) -ceq 'ok') 'Unicode/spaced .ps1 path did not execute'
        }
        'failure_exit_code' {
            $job=Fixture 'fail.cmd' "@echo off`nexit /b 7`n"
            $jobId=Submit-Job @('-N','failure',$job)
            $m=Wait-Record $jobId 'failed'
            Need ($m['exit_code'] -eq '7' -and $m['status'] -eq 'FAILED') 'Failed job exit/status mismatch'
        }
        'capacity_fifo' {
            Configure 1
            $first=Submit-Job @('-N','first',(Gate-Job))
            Wait-True { Test-Path (Join-Path $script:work 'started.txt') } 'first job gate'
            $job=Fixture 'second.ps1' "[IO.File]::WriteAllText('second.ok','ok')"
            $second=Submit-Job @('-N','second',$job)
            Prove-Waiting $second 'second.ok'
            Write-Utf8 (Join-Path $script:work 'release.txt') 'release'
            Wait-Record $first|Out-Null; Wait-Record $second|Out-Null
            Need (Test-Path (Join-Path $script:work 'second.ok')) 'Queued job did not run after capacity released'
        }
        'dependency_success' {
            $first=Submit-Job @('-N','dependency',(Gate-Job))
            Wait-True { Test-Path (Join-Path $script:work 'started.txt') } 'dependency gate'
            $job=Fixture 'dependent.ps1' "[IO.File]::WriteAllText('dependent.ok','ok')"
            $second=Submit-Job @('-N','dependent','-hold_jid',$first,$job)
            Prove-Waiting $second 'dependent.ok'
            Write-Utf8 (Join-Path $script:work 'release.txt') 'release'
            Wait-Record $first|Out-Null; Wait-Record $second|Out-Null
        }
        'delayed_start' {
            $job=Fixture 'delayed.ps1' "[IO.File]::WriteAllText('delayed.ok','ok')"
            $jobId=Submit-Job @('-N','delayed','-a','+5s',$job)
            Prove-Waiting $jobId 'delayed.ok'
            Wait-Record $jobId|Out-Null
            Need (Test-Path (Join-Path $script:work 'delayed.ok')) 'Delayed job did not execute'
        }
        'range_list' {
            $job=Fixture 'value.ps1' "param([string]`$Value)`n[IO.File]::WriteAllText(('value-'+`$Value+'.txt'),`$Value)"
            foreach($item in @(@('--range','1-3',3),@('--list','A,B',2))) {
                $r=Invoke-Fb @('sub',$item[0],$item[1],'-N','values',$job,'{}')
                Need ($r.exit_code -eq 0) 'Expansion submission failed'
                $ids=[regex]::Matches($r.stdout,'(?m)^\s*(\d+)\.master\s*$')
                Need ($ids.Count -eq $item[2]) 'Expansion ID count mismatch'
                foreach($match in $ids) { Wait-Record $match.Groups[1].Value|Out-Null }
            }
            foreach($value in @('1','2','3','A','B')) { Need ((Text (Join-Path $script:work ('value-'+$value+'.txt'))) -ceq $value) 'Expanded argument mismatch' }
        }
        'pbs_explicit_history_filters' {
            $job=Fixture 'echo.cmd' "@echo off`necho ALIAS_OK`n"
            $first=Submit-Job @('-N','history1',$job)
            Wait-Record $first|Out-Null
            $second=Submit-Job @('-N','history2',$job)
            Wait-Record $second|Out-Null
            $r=Invoke-Fb @('stat','--style','pbs',($first+'.master'))
            Need ($r.exit_code -eq 0 -and $r.stdout -match ([regex]::Escape($first+'.master')) -and $r.stdout -notmatch ([regex]::Escape($second+'.master'))) 'PBS ID filter mismatch'
            $r=Invoke-Fb @('stat','--style','pbs','-H')
            Need ($r.stdout -match ([regex]::Escape($first+'.master')) -and $r.stdout -match ([regex]::Escape($second+'.master'))) 'PBS history missing completed jobs'
        }
        'windows_rejects_aliases' {
            foreach($alias in @('qsub','qstat','qdel','other')) {
                [IO.File]::Copy($Binary,(Join-Path $script:work ($alias+'.exe')))
                foreach($arguments in @(@('--help'),@('daemon','start',$script:state))) {
                    $r=Invoke-Fb -Arguments $arguments -Alias $alias
                    Need ($r.exit_code -eq 2 -and [string]::IsNullOrWhiteSpace($r.stdout) -and $r.stderr -match 'Windows' -and $r.stderr -match 'fbqueue.exe') 'Alias must fail with status 2 and explicit-command guidance'
                }
            }
            Need (@(Get-ChildItem $script:state -Filter '*.job' -Recurse).Count -eq 0 -and -not (Test-Path (Join-Path $script:state 'run/daemon.pid'))) 'Rejected aliases created jobs or started a daemon'
            Need (@(Get-ChildItem $script:state -Directory).Count -eq 0) 'Rejected aliases initialized state directories'
            [IO.File]::Copy($Binary,(Join-Path $script:work 'FBQUEUE.exe'))
            $r=Invoke-Fb -Arguments @('--help') -Alias 'FBQUEUE'
            Need ($r.exit_code -eq 0 -and $r.stdout -match 'fbqueue\.exe sub') 'Canonical name must work case-insensitively'
        }
        'pending_cancel_history' {
            Configure 1
            $first=Submit-Job @('-N','blocker',(Gate-Job))
            Wait-True { Test-Path (Join-Path $script:work 'started.txt') } 'blocker gate'
            $second=Submit-Job @('-N','cancelled','cmd','/d','/c','echo','MUST_NOT_RUN')
            $r=Invoke-Fb @('del',($second+'.master'))
            Need ($r.exit_code -eq 0) 'fbqueue del failed'
            $m=Wait-Record $second 'failed'
            Need ($m['status'] -eq 'CANCELLED' -and $m.ContainsKey('end_at') -and -not $m.ContainsKey('start_at') -and -not $m.ContainsKey('exit_code')) 'Pending cancellation history incorrect'
            Write-Utf8 (Join-Path $script:work 'release.txt') 'release'
            Wait-Record $first|Out-Null
        }
        'running_cancel_tree' {
            $tree=Heartbeat-Job
            try {
                $r=Invoke-Fb @('del',($tree.job_id+'.master'))
                Need ($r.exit_code -eq 0) 'Running fbqueue del failed'
                $m=Wait-Record $tree.job_id 'failed'
                Need ($m['status'] -eq 'CANCELLED') 'Running cancellation status mismatch'
                Check-TreeStopped $tree
            } finally { $tree.root.Dispose(); $tree.leaf.Dispose() }
        }
        'walltime_tree' {
            $tree=Heartbeat-Job @('-W','00:00:08')
            try {
                $m=Wait-Record $tree.job_id 'failed'
                Need ($m['status'] -eq 'TIMEOUT') 'Walltime status mismatch'
                Check-TreeStopped $tree
            } finally { $tree.root.Dispose(); $tree.leaf.Dispose() }
        }
        'windows_daemon_stop_tree' {
            $tree=Heartbeat-Job
            try {
                $r=Invoke-Fb @('daemon','stop')
                Need ($r.exit_code -eq 0) 'daemon stop failed'
                Check-TreeStopped $tree
                Need (-not (Test-Path (Join-Path $script:state 'run/daemon.pid'))) 'Stopped daemon PID file retained'
                # Do not issue stat/sub here: Windows would replay the unresolved record.
            } finally { $tree.root.Dispose(); $tree.leaf.Dispose() }
        }
        'development_validation' {
            $bad=@(@('sub'),@('sub','-c'),@('sub','-c','0','cmd'),@('sub','-c','-1','cmd'),@('sub','-c','abc','cmd'),@('sub','-c','999999999999999999999999','cmd'),@('sub','-c','5','cmd'),@('sub','-q','missing','cmd'),@('sub','--range','3-1','cmd'),@('sub','--list','A,,B','cmd'))
            foreach($arguments in $bad) {
                $r=Invoke-Fb -Arguments $arguments
                Need ($r.exit_code -eq 2 -and -not [string]::IsNullOrWhiteSpace($r.stderr) -and [string]::IsNullOrWhiteSpace($r.stdout)) 'Invalid input did not return stderr/status 2 without ID'
            }
            Need (@(Get-ChildItem $script:state -Filter '*.job' -Recurse).Count -eq 0 -and -not (Test-Path (Join-Path $script:state 'run/last_id'))) 'Rejected request allocated jobs or IDs'
            $r=Invoke-Fb @('sub','--help')
            Need ($r.exit_code -eq 0 -and $r.stdout -match 'Usage:' -and [string]::IsNullOrWhiteSpace($r.stderr)) 'Submission help contract mismatch'
        }
        'first_run_defaults' {
            Remove-Item (Join-Path $script:state 'config')
            $job=Fixture 'first.cmd' "@echo off`necho FIRST_RUN_OK`n"
            $jobId=Submit-Job @('-N','first',$job)
            $m=Wait-Record $jobId
            Need ($m['queue'] -eq 'batch' -and $m['cost'] -eq '1') 'First-run defaults mismatch'
        }
        'archive_if_tar_available' {
            $tar=Get-Command tar.exe -CommandType Application -ErrorAction SilentlyContinue
            if (-not $tar) { throw 'SKIP: tar.exe is optional and not on PATH' }
            Write-Utf8 (Join-Path $script:state 'config') "capacity: 4`ndefault_queue: batch`ninactivity_timeout: 50`nhistory_limit: 2`narchive_interval_days: 0`nqueue: batch`n  capacity: 4`n"
            $job=Fixture 'archive.cmd' "@echo off`necho ARCHIVE`n"
            1..5|ForEach-Object { $jobId=Submit-Job @('-N',('archive'+$_),$job); Wait-Record $jobId|Out-Null }
            Wait-True { @(Get-ChildItem (Join-Path $script:state 'archive') -Filter 'archive_*.tar.gz' -ErrorAction SilentlyContinue).Count -gt 0 } 'background archive' 40
            $archive=@(Get-ChildItem (Join-Path $script:state 'archive') -Filter 'archive_*.tar.gz')[0].FullName
            $exitCode=[FBQWindowsTest.Native]::RunInherited($tar.Source,@('-tzf',$archive),$script:work,(Join-Path $CaseDirectory 'tar.stdout.txt'),(Join-Path $CaseDirectory 'tar.stderr.txt'),15000)
            Need ($exitCode -eq 0 -and (Text (Join-Path $CaseDirectory 'tar.stdout.txt')) -match '\.job') 'Archive could not be listed or contained no job records'
            Need (@(Get-ChildItem (Join-Path $script:state 'queue/done') -Filter '*.job').Count -eq 2) 'History retention count mismatch'
        }
        'harness_containment' {
            Fixture 'orphan.ps1' "[IO.File]::WriteAllText('owned-child.ready','ready');Start-Sleep -Seconds 60"|Out-Null
            $info=New-Object Diagnostics.ProcessStartInfo
            $info.FileName=Join-Path $env:SystemRoot 'System32/WindowsPowerShell/v1.0/powershell.exe'
            $info.Arguments='-NoProfile -NonInteractive -ExecutionPolicy Bypass -File .\orphan.ps1'
            $info.UseShellExecute=$false; $info.CreateNoWindow=$true; $info.WorkingDirectory=$script:work
            $owned=[Diagnostics.Process]::Start($info)
            try { Wait-True { Test-Path (Join-Path $script:work 'owned-child.ready') } 'contained child ready'; Need (-not $owned.HasExited) 'Containment control child exited prematurely' } finally { $owned.Dispose() }
            # Parent must observe this live descendant, kill the case job, and verify zero.
        }
        default { throw "Unknown case name: $Name" }
    }
    $record.status='PASS'; $record.message='All assertions passed'
} catch {
    $record.message=$_.Exception.Message
    $infrastructure=$_.Exception.InnerException -is [TimeoutException] -or $_.Exception.InnerException -is [ComponentModel.Win32Exception]
    $record.status=if ($record.message.StartsWith('SKIP:')) { 'SKIP' } elseif (-not $enteredTest -or $infrastructure) { 'ERROR' } else { 'FAIL' }
    $record['stack']=$_.ScriptStackTrace
} finally {
    if (Test-Path (Join-Path $CaseDirectory 'private state/queue')) {
        $snapshots=@()
        foreach($directory in @('new','running','done','failed','cancel')) {
            foreach($file in @(Get-ChildItem (Join-Path $CaseDirectory ('private state/queue/'+$directory)) -Filter '*.job' -ErrorAction SilentlyContinue)) {
                try {
                    $selected=[ordered]@{directory=$directory; file=$file.Name}
                    $metadata=Meta $file.FullName
                    foreach($key in @('id','name','cost','queue','status','exit_code','exit_reason','start_at','end_at')) { if($metadata.ContainsKey($key)) { $selected[$key]=$metadata[$key] } }
                    $snapshots+=[pscustomobject]$selected
                } catch { $snapshots+=[pscustomobject]@{directory=$directory;file=$file.Name;error=$_.Exception.Message} }
            }
        }
        $record.snapshots=$snapshots
    }
    $record.seconds=[Math]::Round($clock.Elapsed.TotalSeconds,3)
    Write-Utf8 (Join-Path $CaseDirectory 'case.json') ($record|ConvertTo-Json -Depth 10)
}
if ($record.status -eq 'ERROR') { exit 2 }
if ($record.status -eq 'FAIL') { exit 1 }
exit 0
