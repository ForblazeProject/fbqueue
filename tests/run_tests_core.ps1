# Safe compatibility entrypoint; never stop processes by executable name.
[CmdletBinding()]
param(
    [string]$Binary,
    [switch]$Build,
    [ValidateSet('Development','Released095')][string]$Profile='Development',
    [string]$SourceRoot,
    [string]$OutputDirectory,
    [string[]]$Case
)
& (Join-Path $PSScriptRoot 'windows/Run-Tests.ps1') @PSBoundParameters
exit $LASTEXITCODE
