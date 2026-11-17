@echo off
rem Log plugin used by the Windows system tests.
rem The plugin name is the file name without the extension: robotlog
if "%~1"=="list" (
    echo robot_log
    exit /b 0
)
if "%~1"=="get" (
    if "%~2"=="robot_log" (
        echo Robot log content
        exit /b 0
    )
    echo No logs found for log type "%~2" 1>&2
    exit /b 1
)
exit /b 1
