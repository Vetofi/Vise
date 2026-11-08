@echo off
where cargo >nul 2>nul
if not errorlevel 1 exit /b 0
if exist "%USERPROFILE%\.cargo\bin\cargo.exe" (
  set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"
  exit /b 0
)
echo Rust not found. Downloading and installing it (one time, a few minutes)...
powershell -NoProfile -ExecutionPolicy Bypass -Command "$ProgressPreference='SilentlyContinue'; Invoke-WebRequest 'https://win.rustup.rs/x86_64' -OutFile ($env:TEMP + '\rustup-init.exe')"
if errorlevel 1 (
  echo [ERROR] Could not download rustup. Check your internet connection.
  exit /b 1
)
"%TEMP%\rustup-init.exe" -y --default-toolchain stable --profile minimal
if errorlevel 1 (
  echo [ERROR] Rust installation failed.
  exit /b 1
)
set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"
exit /b 0
