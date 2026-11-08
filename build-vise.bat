@echo off

cd /d "%~dp0"
title Vise build

call "%~dp0tools\windows\ensure-msvc.bat"
if errorlevel 1 goto :end
call "%~dp0tools\windows\ensure-rust.bat"
if errorlevel 1 goto :end

set "PROFILE_ARG=--release"
set "PROFILE_DIR=release"
if /i "%~1"=="fast" (
  set "PROFILE_ARG=--profile fast"
  set "PROFILE_DIR=fast"
)
if /i "%~1"=="debug" (
  set "PROFILE_ARG="
  set "PROFILE_DIR=debug"
)

echo Building Vise (%PROFILE_DIR%). The first build takes a long time...
cargo build %PROFILE_ARG% -p vise-client -p vise-prepare
if errorlevel 1 (
  echo.
  echo [ERROR] Build failed. See the messages above.
  goto :end
)

echo.
echo [OK] Build finished. Binaries are in: %~dp0target\%PROFILE_DIR%\

:end
echo.
pause
