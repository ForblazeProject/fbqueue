@echo off
setlocal
"%SystemRoot%\System32\WindowsPowerShell\v1.0\powershell.exe" -NoProfile -ExecutionPolicy Bypass -File "%~dp0Run-Tests.ps1" -Build %*
set "FBQ_TEST_EXIT=%ERRORLEVEL%"
echo.
echo Build/test exit code: %FBQ_TEST_EXIT%
pause
exit /b %FBQ_TEST_EXIT%
