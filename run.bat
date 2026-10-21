@echo off
chcp 65001 >nul
setlocal EnableExtensions
cd /d "%~dp0"

rem Запуск программы. Если программа ещё не собрана - сначала собирает её.
rem После изменения исходников запустите build.bat, чтобы пересобрать.

call "%~dp0app-name.cmd"
set "EXE=%~dp0release\%APPNAME%\%EXENAME%"

if exist "%EXE%" goto :run
echo Программа ещё не собрана - запускаю сборку...
echo.
set "ZV_NOPAUSE=1"
call "%~dp0build.bat"
if errorlevel 1 goto :failed
if not exist "%EXE%" goto :failed

:run
start "" "%EXE%"
exit /b 0

:failed
echo.
echo Сборка не удалась - подробности выше и в build_log.txt.
pause
exit /b 1
