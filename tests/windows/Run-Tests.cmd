@echo off
setlocal
"%SystemRoot%\System32\WindowsPowerShell\v1.0\powershell.exe" -NoProfile -ExecutionPolicy Bypass -File "%~dp0Run-Tests.ps1" %*
set "FBQ_TEST_EXIT=%ERRORLEVEL%"
echo.
echo Test exit code: %FBQ_TEST_EXIT%   ^(0=pass, 1=test failure, 2=incomplete/harness error^)
pause
exit /b %FBQ_TEST_EXIT%
