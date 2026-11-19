@echo off
rem Windows version of the mock `file` configuration plugin (see the `file` shell script)
if "%~1"=="list" goto list
if "%~1"=="get" goto get
if "%~1"=="set" goto set
>&2 echo Unknown command: %~1
exit /b 1

:list
echo tedge-configuration-plugin
echo type_one
echo type_two
echo type_three
echo type_four
exit /b 0

:get
if "%~2"=="tedge-configuration-plugin" echo tedge-configuration-plugin.toml& exit /b 0
if "%~2"=="type_one" echo file_a& exit /b 0
if "%~2"=="type_two" echo file_b& exit /b 0
if "%~2"=="type_three" echo file_c& exit /b 0
if "%~2"=="type_four" echo file_d& exit /b 0
>&2 echo Unknown config type: %~2
exit /b 1

:set
if "%~2"=="tedge-configuration-plugin" echo updated tedge-configuration-plugin.toml& exit /b 0
if "%~2"=="type_one" echo updated file_a& exit /b 0
if "%~2"=="type_two" echo updated file_b& exit /b 0
if "%~2"=="type_three" echo updated file_c& exit /b 0
if "%~2"=="type_four" echo updated file_d& exit /b 0
>&2 echo Unknown config type: %~2
exit /b 1
