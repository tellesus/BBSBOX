@echo off
setlocal
set "BBSBOX_CLIENT=%~dp0target\debug\bbsbox-native-probe.exe"
if not exist "%BBSBOX_CLIENT%" (
  echo BBSBOX desktop client has not been built yet.
  echo See README.md in this folder for the build instructions.
  pause
  exit /b 1
)
start "" "%BBSBOX_CLIENT%"
