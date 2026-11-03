@echo off
rem TwinDesk — Copyright (C) 2026 inukaasith. GPL-3.0, see LICENSE.
rem Builds the virtual camera (windows/vcam.js): bin\twindesk-vcam-x64.dll and, when Visual Studio
rem has the ARM64 tools, bin\twindesk-vcam-arm64.dll (Frame Server loads the one of Windows' own
rem architecture), and bin\twindesk-vcam.exe (x64; it only adds the camera and feeds it, so it also
rem runs on ARM). Needs Visual Studio or its Build Tools with C++; without them nothing is built and
rem the Camera tab says the virtual camera isn't in this build.
setlocal
set VSWHERE=%ProgramFiles(x86)%\Microsoft Visual Studio\Installer\vswhere.exe
if not exist "%VSWHERE%" goto none
set VS=
for /f "usebackq delims=" %%i in (`"%VSWHERE%" -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath`) do set VS=%%i
if not defined VS goto none
set SRC=%~dp0
set BIN=%~dp0..\bin
set OBJ=%~dp0..\bin\vcam-obj
if not exist "%BIN%" mkdir "%BIN%"
set CFLAGS=/nologo /O2 /EHsc /MT /W3 /std:c++17 /DUNICODE /D_UNICODE
set LIBS=mfplat.lib mf.lib mfuuid.lib mfreadwrite.lib mfsensorgroup.lib ole32.lib advapi32.lib strmiids.lib

call :arch x64 x64 || exit /b 1
rem The ARM64 cross tools are optional.
if exist "%VS%\VC\Tools\MSVC" for /d %%d in ("%VS%\VC\Tools\MSVC\*") do if exist "%%d\bin\Hostx64\arm64\cl.exe" set ARM=1
if defined ARM (call :arch x64_arm64 arm64 || exit /b 1) else echo note: no ARM64 C++ tools: no twindesk-vcam-arm64.dll
rmdir /s /q "%OBJ%" 2>nul
exit /b 0

:arch
setlocal
if not exist "%OBJ%\%2" mkdir "%OBJ%\%2"
call "%VS%\VC\Auxiliary\Build\vcvarsall.bat" %1 >nul || exit /b 1
rem A loaded DLL can be renamed but not overwritten: the new one is built beside it and swapped in.
cl %CFLAGS% /LD /Fo"%OBJ%\%2\\" "%SRC%source.cpp" /link /DEF:"%SRC%source.def" /OUT:"%BIN%\twindesk-vcam-%2.new.dll" /IMPLIB:"%OBJ%\%2\source.lib" %LIBS% || exit /b 1
del /q "%BIN%\twindesk-vcam-%2.old*.dll" 2>nul
if exist "%BIN%\twindesk-vcam-%2.dll" ren "%BIN%\twindesk-vcam-%2.dll" twindesk-vcam-%2.old-%RANDOM%.dll
ren "%BIN%\twindesk-vcam-%2.new.dll" twindesk-vcam-%2.dll
del /q "%BIN%\twindesk-vcam-%2.new.*" 2>nul
if "%2"=="x64" (
  cl %CFLAGS% /Fo"%OBJ%\%2\\" "%SRC%vcam.cpp" /link /OUT:"%BIN%\twindesk-vcam.exe" %LIBS% || exit /b 1
)
echo built twindesk-vcam-%2.dll
exit /b 0

:none
echo note: no Visual Studio C++ tools: the virtual camera isn't built (install the Build Tools with "Desktop development with C++")
exit /b 0
