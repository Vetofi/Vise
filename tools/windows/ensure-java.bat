@echo off
set "VISE_JDK=%~dp0..\..\.local\jdk"
where javac >nul 2>nul
if errorlevel 1 goto :download
set "JV="
for /f "tokens=2" %%v in ('javac -version 2^>^&1') do set "JV=%%v"
set "JM="
for /f "delims=." %%m in ("%JV%") do set "JM=%%m"
if not defined JM goto :download
if %JM% GEQ 25 exit /b 0

:download
if exist "%VISE_JDK%\bin\javac.exe" goto :use
echo JDK 25 not found. Downloading it (one time, about 200 MB)...
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0fetch-jdk.ps1" -Out "%VISE_JDK%"
if errorlevel 1 goto :fail

:use
set "JAVA_HOME=%VISE_JDK%"
set "PATH=%VISE_JDK%\bin;%PATH%"
exit /b 0

:fail
echo [ERROR] Could not download the JDK. Check your internet connection.
exit /b 1
