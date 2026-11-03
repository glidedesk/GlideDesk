@echo off
rem TwinDesk — Copyright (C) 2026 inukaasith. GPL-3.0, see LICENSE.
rem Builds bin\twindesk-pad.exe (windows/gamepad.js: Remote › Real gamepad), the phone's gamepad as
rem an Xbox 360 controller through ViGEmBus, with ViGEmClient (vigem\, MIT). x64: it only talks to
rem the driver, so it also runs on ARM. Needs Visual Studio or its Build Tools with C++; without them
rem nothing is built and the Remote tab says Real gamepad isn't in this build.
setlocal
set VSWHERE=%ProgramFiles(x86)%\Microsoft Visual Studio\Installer\vswhere.exe
if not exist "%VSWHERE%" goto none
set VS=
for /f "usebackq delims=" %%i in (`"%VSWHERE%" -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath`) do set VS=%%i
if not defined VS goto none
set SRC=%~dp0
set BIN=%~dp0..\bin
set OBJ=%~dp0..\bin\pad-obj
if not exist "%BIN%" mkdir "%BIN%"
if not exist "%OBJ%" mkdir "%OBJ%"
call "%VS%\VC\Auxiliary\Build\vcvarsall.bat" x64 >nul || exit /b 1
rem A running .exe can be renamed but not overwritten: the new one is built beside it and swapped in.
cl /nologo /O2 /EHsc /MT /W3 /std:c++17 /DUNICODE /D_UNICODE /I"%SRC%vigem\include" /Fo"%OBJ%\\" ^
  "%SRC%pad.cpp" "%SRC%vigem\src\ViGEmClient.cpp" /link /OUT:"%BIN%\twindesk-pad.new.exe" setupapi.lib || exit /b 1
del /q "%BIN%\twindesk-pad.old*.exe" 2>nul
if exist "%BIN%\twindesk-pad.exe" ren "%BIN%\twindesk-pad.exe" twindesk-pad.old-%RANDOM%.exe
ren "%BIN%\twindesk-pad.new.exe" twindesk-pad.exe
rmdir /s /q "%OBJ%" 2>nul
echo built twindesk-pad.exe
exit /b 0

:none
echo note: no Visual Studio C++ tools: twindesk-pad.exe isn't built (install the Build Tools with "Desktop development with C++")
exit /b 0
