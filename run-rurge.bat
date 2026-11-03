@echo off
title Rurge Horizon Client
cd /d "%~dp0"
echo Starting Rurge Horizon Client...
target\debug\rurge-horizon.exe
if %ERRORLEVEL% NEQ 0 (
    echo.
    echo Game exited with error code %ERRORLEVEL%
    pause
)
