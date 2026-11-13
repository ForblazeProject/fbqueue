# Run on Windows or PowerShell 7/Linux; no product processes are started.
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version 2
function Need([bool]$Condition, [string]$Message) { if (-not $Condition) { throw $Message } }
$report = Join-Path $PSScriptRoot 'Report.ps1'
if (Test-Path $report) { . $report }
Need ([bool](Get-Command Get-RunVerdict -ErrorAction SilentlyContinue)) 'RED: report verdict implementation is missing'
$v = Get-RunVerdict @(@{status='PASS'}, @{status='FAIL'}, @{status='SKIP'}, @{status='NOT_RUN'})
Need ($v.total -eq 4 -and $v.passed -eq 1 -and $v.failed -eq 1 -and $v.skipped -eq 1 -and $v.not_run -eq 1 -and $v.exit_code -eq 2) 'Incomplete runs must not report success'
$v = Get-RunVerdict @(@{status='PASS'}, @{status='FAIL'})
Need ($v.exit_code -eq 1 -and $v.failed -eq 1) 'Assertion failure must return 1'
$v = Get-RunVerdict @(@{status='PASS'}, @{status='SKIP'})
Need ($v.exit_code -eq 0 -and $v.skipped -eq 1) 'Explicit optional/profile skips are separate from passes'
Need ((Get-RunVerdict @(@{status='SKIP'})).exit_code -eq 2) 'Zero executed cases must not succeed'
Need ((Get-RunVerdict @(@{status='ERROR'})).exit_code -eq 2) 'Harness error must return 2'
$root = Join-Path ([IO.Path]::GetTempPath()) ('fbqueue-protocol-' + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $root | Out-Null
try {
    $path = Join-Path $root 'case.json'
    $rejected = $false
    try { Get-CaseRecord $path 'probe' 0 | Out-Null } catch { $rejected = $true }
    Need $rejected 'Missing case JSON must be rejected'
    Write-Utf8 $path '{broken'
    $rejected = $false
    try { Get-CaseRecord $path 'probe' 0 | Out-Null } catch { $rejected = $true }
    Need $rejected 'Malformed case JSON must be rejected'
    Write-Utf8 $path '{"schema":"fbqueue.windows.case.v1","name":"probe","status":"PASS","message":"ok"}'
    Need ((Get-CaseRecord $path 'probe' 0).status -eq 'PASS') 'Valid PASS must be accepted'
    $rejected = $false
    try { Get-CaseRecord $path 'probe' 1 | Out-Null } catch { $rejected = $true }
    Need $rejected 'Exit 1 plus PASS JSON must be rejected'
    $rejected = $false
    try { Get-CaseRecord $path 'wrong-case' 0 | Out-Null } catch { $rejected = $true }
    Need $rejected 'Wrong case identity must be rejected'
    Write-Utf8 $path '{"schema":"fbqueue.windows.case.v1","name":"probe","status":"FAIL","message":"deliberate assertion"}'
    Need ((Get-CaseRecord $path 'probe' 1).status -eq 'FAIL') 'Real FAIL must remain a failure'
} finally { Remove-Item -Recurse -Force $root }
Add-Type -Path (Join-Path $PSScriptRoot 'SafeProcess.cs')
Need ([FBQWindowsTest.Native]::Quote('') -ceq '""') 'Empty argument quoting'
Need ([FBQWindowsTest.Native]::Quote('a b') -ceq '"a b"') 'Space argument quoting'
Need ([FBQWindowsTest.Native]::Quote('a"b') -ceq '"a\"b"') 'Embedded quote escaping'
Need ([FBQWindowsTest.Native]::Quote('C:\space path\') -ceq '"C:\space path\\"') 'Trailing backslash escaping'
$workerText=[IO.File]::ReadAllText((Join-Path $PSScriptRoot 'Case-Worker.ps1'))
$mainText=[IO.File]::ReadAllText((Join-Path $PSScriptRoot 'Run-Tests.ps1'))
# Reproduce Windows PowerShell's parameter-binding scope: no script-root value yet.
$bindingTokens=$null; $bindingErrors=$null
$mainAst=[Management.Automation.Language.Parser]::ParseInput($mainText,[ref]$bindingTokens,[ref]$bindingErrors)
$bindingProbe=[scriptblock]::Create($mainAst.ParamBlock.Extent.Text+"`n[pscustomobject]@{output_directory=`$OutputDirectory;cases=@(`$Case)}")
$bound=& $bindingProbe -Case help
Need ([string]::IsNullOrWhiteSpace($bound.output_directory) -and $bound.cases[0] -eq 'help') 'Default binding must not resolve paths before the script body'
$bound=& $bindingProbe -OutputDirectory 'explicit results path'
Need ($bound.output_directory -ceq 'explicit results path') 'Explicit output directory must survive parameter binding'
$declared=[regex]::Match($mainText,'\$catalog=@\(([^\r\n]+)\)').Groups[1].Value
$names=@([regex]::Matches($declared,"'([^']+)'")|ForEach-Object {$_.Groups[1].Value})
$implemented=@([regex]::Matches($workerText,"(?m)^        '([^']+)' \{")|ForEach-Object {$_.Groups[1].Value})
Need ($names.Count -eq 19 -and @($names|Select-Object -Unique).Count -eq 19 -and @(Compare-Object $names $implemented).Count -eq 0) 'Case catalog and worker implementations must match exactly'
$tokens=$null; $errors=$null
$ast=[Management.Automation.Language.Parser]::ParseInput($workerText,[ref]$tokens,[ref]$errors)
Need (@($errors).Count -eq 0) 'Worker syntax'
foreach($function in $ast.FindAll({param($node) $node -is [Management.Automation.Language.FunctionDefinitionAst]},$false)) {
    if($function.Name -in @('Meta','Text','Configure','Fixture','Wait-True','Submit-Job')) { . ([scriptblock]::Create($function.Extent.Text)) }
}
$helperRoot=Join-Path ([IO.Path]::GetTempPath()) ('fbqueue-worker-helpers-'+[Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $helperRoot|Out-Null
try {
    $script:work=$helperRoot; $script:state=$helperRoot
    Configure 3
    $config=[IO.File]::ReadAllText((Join-Path $helperRoot 'config'))
    Need ($config -match 'capacity: 3' -and $config -match 'default_queue: batch') 'Private config generation'
    $relative=Fixture 'sample.cmd' "@echo off`necho ok`n"
    Need ($relative -ceq '.\sample.cmd') 'Fixture relative Windows path'
    Need ([IO.File]::ReadAllText((Join-Path $helperRoot 'sample.cmd')).Contains("`r`n")) 'Batch fixture CRLF'
    Fixture 'sample.ps1' 'exit 0'|Out-Null
    $bytes=[IO.File]::ReadAllBytes((Join-Path $helperRoot 'sample.ps1'))
    Need ($bytes[0] -eq 239 -and $bytes[1] -eq 187 -and $bytes[2] -eq 191) 'PowerShell 5 fixture UTF-8 BOM'
    Write-Utf8 (Join-Path $helperRoot 'record.job') "id: 12`nstatus: FAILED`nexit_code: 7`n"
    $metadata=Meta (Join-Path $helperRoot 'record.job')
    Need ($metadata['id'] -eq '12' -and $metadata['exit_code'] -eq '7') 'Shared-read metadata parser'
    $routingFunction=@($ast.FindAll({param($n) $n -is [Management.Automation.Language.FunctionDefinitionAst] -and $n.Name -eq 'Invoke-Fb'},$false))[0]
    $routingAssignment=@($routingFunction.Body.EndBlock.Statements|Where-Object { $_ -is [Management.Automation.Language.AssignmentStatementAst] -and $_.Left.Extent.Text -eq '$exe' })[0]
    $routingProbe=[scriptblock]::Create($routingAssignment.Extent.Text+"`n`$exe")
    $Binary='provided-fbqueue.exe'; $Alias='fbqueue'
    Need ((& $routingProbe) -ceq $Binary) 'Default route must use supplied binary'
    $Alias='FBQUEUE'
    Need ((& $routingProbe) -ceq (Join-Path $script:work 'FBQUEUE.exe')) 'Uppercase fixture must actually use the copied executable'
    New-Item -ItemType Directory -Path (Join-Path $helperRoot 'run')|Out-Null
    Write-Utf8 (Join-Path $helperRoot 'run/daemon.pid') '123'
    function Invoke-Fb([string[]]$Arguments,[string]$Alias) { $script:captured=$Arguments; return [pscustomobject]@{exit_code=0;stdout="12.master`n";stderr=''} }
    Need ((Submit-Job @('-N','probe','cmd')) -ceq '12') 'Submit helper ID extraction'
    Need ($script:captured.Count -eq 4 -and $script:captured[0] -eq 'sub') 'Submission options must remain individual arguments'
} finally { Remove-Item -Recurse -Force $helperRoot }
Write-Host 'PASS: early parameter binding, protocol/verdicts, deliberate negative controls, C# compilation/quoting, 19-case catalog, fixture/config/metadata/submission helpers. No Windows product execution claimed.'
