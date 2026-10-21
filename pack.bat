@echo off
chcp 65001 >nul
setlocal EnableExtensions
cd /d "%~dp0"
call "%~dp0app-name.cmd"
title Упаковка: %APPTITLE%

rem ======================================================================
rem  Готовит в папке dist два чистых комплекта и zip-архив каждого:
rem    dist\НАЗВАНИЕ-src-ВЕРСИЯ\    - исходники: только то, что нужно для сборки
rem    dist\НАЗВАНИЕ-ВЕРСИЯ-win64\  - готовая программа: распаковал и запустил
rem  Не копируется: target, .toolchain, release, dist, логи, временные файлы.
rem  Личные данные пользователя здесь не хранятся, поэтому в архивы они не попадут.
rem ======================================================================

set "VER="
for /f "tokens=2 delims==" %%v in ('findstr /b /c:"version" Cargo.toml') do if not defined VER set "VER=%%v"
set VER=%VER: =%
set VER=%VER:"=%
if "%VER%"=="" set "VER=0.0.0"

set "SRC=dist\%APPNAME%-src-%VER%"
set "BIN=dist\%APPNAME%-%VER%-win64"
set "XD=target .toolchain release dist __pycache__ .git .vs .idea"
set "XF=*.pyc *.log *.tmp *.bak build_log.txt"

echo ================================================
echo   Упаковка: %APPTITLE% %VER%
echo ================================================
echo.

if not exist dist mkdir dist

rem ---------- Исходники ----------
echo [1/2] Исходники: %SRC%
if exist "%SRC%" rmdir /s /q "%SRC%"
mkdir "%SRC%"
for %%d in (app crates plugins docs locales assets tools .cargo .github) do (
    if exist "%%d" robocopy "%%d" "%SRC%\%%d" /E /XD %XD% /XF %XF% /NFL /NDL /NJH /NJS /NP >nul
)
for %%f in (Cargo.toml Cargo.lock rustfmt.toml README.md CHANGELOG.md CHANGELOG.en.md SOURCES.md LICENSE .gitignore .gitattributes app-name.cmd build.bat run.bat pack.bat) do (
    if exist "%%f" copy /y "%%f" "%SRC%\" >nul
)
if not exist "%SRC%\Cargo.lock" goto :fail_src
call :zip "%SRC%"
if errorlevel 1 goto :fail_zip
echo       готово: %SRC%.zip

rem ---------- Программа ----------
echo.
echo [2/2] Программа: %BIN%
if not exist "release\%APPNAME%\%EXENAME%" goto :no_build
if exist "%BIN%" rmdir /s /q "%BIN%"
robocopy "release\%APPNAME%" "%BIN%" /E /XD __pycache__ /XF %XF% /NFL /NDL /NJH /NJS /NP >nul
if errorlevel 8 goto :fail_bin
call :zip "%BIN%"
if errorlevel 1 goto :fail_zip
echo       готово: %BIN%.zip
goto :done

:no_build
echo       Программа ещё не собрана - запустите build.bat, затем pack.bat ещё раз.
echo       Исходники упакованы.

:done
echo.
echo ================================================
echo   Всё в папке dist
echo ================================================
if "%ZV_NOPAUSE%"=="1" exit /b 0
start "" "%~dp0dist"
pause
exit /b 0

:zip
if exist "%~1.zip" del "%~1.zip"
rem tar.exe встроен в Windows 10 (с версии 1803); -a выбирает формат по расширению .zip
tar.exe -a -c -f "%~1.zip" -C "%~dp1." "%~nx1"
exit /b %errorlevel%

:fail_src
echo ОШИБКА: исходники скопировались не полностью.
if not "%ZV_NOPAUSE%"=="1" pause
exit /b 1

:fail_bin
echo ОШИБКА: не удалось скопировать программу. Возможно, она запущена - закройте её.
if not "%ZV_NOPAUSE%"=="1" pause
exit /b 1

:fail_zip
echo ОШИБКА: не удалось создать zip-архив.
if not "%ZV_NOPAUSE%"=="1" pause
exit /b 1
