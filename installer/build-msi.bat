@echo off
:: Batch wrapper for MSI build script
:: This makes it easier to run from Windows Explorer

powershell.exe -ExecutionPolicy Bypass -File "%~dp0build-msi.ps1" %*

if %ERRORLEVEL% NEQ 0 (
    echo.
    echo Build failed with error code %ERRORLEVEL%
    pause
    exit /b %ERRORLEVEL%
)

echo.
echo Build completed successfully!
pause