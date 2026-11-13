# Shared report protocol; safe to unit-test without Windows APIs.
function Write-Utf8([string]$Path, [string]$Text) {
    [IO.File]::WriteAllText($Path, $Text, (New-Object Text.UTF8Encoding($false)))
}
function Get-RunVerdict([object[]]$Records) {
    $v = [ordered]@{ total=@($Records).Count; passed=0; failed=0; skipped=0; errors=0; not_run=0; exit_code=0 }
    foreach ($r in $Records) {
        switch ($r.status) {
            'PASS' { $v.passed++ }
            'FAIL' { $v.failed++ }
            'SKIP' { $v.skipped++ }
            'ERROR' { $v.errors++ }
            'NOT_RUN' { $v.not_run++ }
            default { $v.errors++ }
        }
    }
    if ($v.errors -gt 0 -or $v.not_run -gt 0 -or ($v.passed + $v.failed) -eq 0) { $v.exit_code=2 }
    elseif ($v.failed -gt 0) { $v.exit_code=1 }
    return [pscustomobject]$v
}
function Get-CaseRecord([string]$Path, [string]$ExpectedName, [int]$ExitCode) {
    if (-not [IO.File]::Exists($Path)) { throw 'Worker did not write case.json' }
    $record = [IO.File]::ReadAllText($Path) | ConvertFrom-Json
    if ($record.schema -ne 'fbqueue.windows.case.v1' -or $record.name -cne $ExpectedName) { throw 'Case protocol/identity mismatch' }
    if ($record.status -notin @('PASS','FAIL','SKIP','ERROR')) { throw 'Invalid worker verdict' }
    $expectedExit = if ($record.status -eq 'ERROR') { 2 } elseif ($record.status -eq 'FAIL') { 1 } else { 0 }
    if ($ExitCode -ne $expectedExit) { throw "Worker exit $ExitCode contradicts JSON verdict $($record.status)" }
    if ($record.status -eq 'SKIP' -and [string]::IsNullOrWhiteSpace($record.message)) { throw 'Skip reason is required' }
    return $record
}
