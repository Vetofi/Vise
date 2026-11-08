@echo off
set "VSWHERE=%ProgramFiles(x86)%\Microsoft Visual Studio\Installer\vswhere.exe"
if exist "%VSWHERE%" (
  for /f "usebackq delims=" %%i in (`"%VSWHERE%" -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath`) do exit /b 0
)
echo C++ build tools not found. Installing Visual Studio Build Tools (one time, big download, accept the admin prompt)...
where winget >nul 2>nul
if errorlevel 1 (
  echo [ERROR] winget is not available. Install "Build Tools for Visual Studio" with the
  echo         "Desktop development with C++" workload from https://visualstudio.microsoft.com/downloads/
  exit /b 1
)
winget install --id Microsoft.VisualStudio.2022.BuildTools -e --accept-package-agreements --accept-source-agreements --override "--passive --wait --norestart --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
exit /b 0
