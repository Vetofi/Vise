@echo off

cd /d "%~dp0"
title Vise
set "PROFILE_ARG=--release"
set "SERVER_JAR=%~dp0.local\server.jar"
set "MC_DIR=%APPDATA%\.minecraft"

call "%~dp0tools\windows\ensure-msvc.bat"
if errorlevel 1 goto :end
call "%~dp0tools\windows\ensure-rust.bat"
if errorlevel 1 goto :end

if exist "%~dp0.local\vise-client\26.3\vise-resources.json" goto :run

echo First run: preparing game resources...
call "%~dp0tools\windows\ensure-java.bat"
if errorlevel 1 goto :end

if exist "%MC_DIR%\versions\26.3\26.3.jar" goto :have_mc
set "MC_DIR=%~dp0.local\mojang"
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0tools\windows\fetch-minecraft.ps1" -Dir "%MC_DIR%"
if errorlevel 1 (
  echo [ERROR] Could not download the Minecraft files. Check your internet connection.
  goto :end
)

:have_mc
if exist "%SERVER_JAR%" goto :have_server
echo Downloading the 26.3 server.jar from Mojang...
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0tools\windows\fetch-server-jar.ps1" -Out "%SERVER_JAR%"
if errorlevel 1 (
  echo [ERROR] Could not download server.jar. Check your internet connection.
  goto :end
)

:have_server
cargo run --release -p vise-prepare -- --prepare --server-jar "%SERVER_JAR%" --minecraft-dir "%MC_DIR%"
if errorlevel 1 (
  echo [ERROR] Preparing resources failed. See the messages above.
  goto :end
)
cargo run --release -p vise-prepare -- sounds --minecraft-dir "%MC_DIR%"

:run
echo Starting Vise. The first build takes a long time...
set "MOD_ARGS="
if exist "%~dp0mods\ruby.vmod" set MOD_ARGS=--dev-mod "%~dp0mods\ruby.vmod"
cargo run %PROFILE_ARG% -p vise-client -- %MOD_ARGS%
if errorlevel 1 echo [ERROR] The game stopped with an error. See the messages above.

:end
echo.
pause
