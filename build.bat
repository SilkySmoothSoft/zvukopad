@echo off
chcp 65001 >nul
setlocal EnableExtensions
cd /d "%~dp0"
call "%~dp0app-name.cmd"
title Сборка: %APPTITLE%

rem ======================================================================
rem  Сборка программы из исходников (название - в app-name.cmd).
rem  Результат: папка release\%APPNAME% (%EXENAME% + plugins + README.txt).
rem
rem  Используются только стандартные программы Windows 10/11:
rem  curl.exe (скачивание и замер скорости), certutil (проверка контрольной суммы).
rem  PowerShell и временные скрипты не используются.
rem
rem  * Если Rust уже установлен - используется он.
rem  * Если нет - предлагается поставить Rust в папку проекта (.toolchain),
rem    без прав администратора и без изменений в системе.
rem  * Библиотеки скачиваются только при первой сборке (или после обновления),
rem    с самого быстрого сервера. Дальше сборка идёт без интернета.
rem
rem  Принудительно выбрать сервер:  set ZV_MIRROR=official   (или rsproxy, ustc, tuna)
rem  Все адреса серверов перечислены в SOURCES.md.
rem ======================================================================

rem Так exe называет cargo (имя из app\Cargo.toml); в папку release он кладётся как %EXENAME%.
set "CARGO_EXE=zvukopad.exe"
set "TOOLCHAIN=stable-x86_64-pc-windows-gnu"
set "TARGET=x86_64-pc-windows-gnu"
set "LOCALTC=%~dp0.toolchain"
set "OUT=%~dp0release\%APPNAME%"
set "MIRRORF=%LOCALTC%\crates-mirror.cmd"
set "ZV_CARGO_ARGS="

rem ---- Серверы с библиотеками (индекс crates.io) ----
set "C_official=https://index.crates.io/"
set "C_rsproxy=https://rsproxy.cn/index/"
set "C_ustc=https://mirrors.ustc.edu.cn/crates.io-index/"
set "C_tuna=https://mirrors.tuna.tsinghua.edu.cn/crates.io-index/"
rem ---- Серверы с компилятором Rust: D = RUSTUP_DIST_SERVER, R = RUSTUP_UPDATE_ROOT ----
set "D_official=https://static.rust-lang.org"
set "R_official=https://static.rust-lang.org/rustup"
set "D_rsproxy=https://rsproxy.cn"
set "R_rsproxy=https://rsproxy.cn/rustup"
set "D_ustc=https://mirrors.ustc.edu.cn/rust-static"
set "R_ustc=https://mirrors.ustc.edu.cn/rust-static/rustup"
set "D_tuna=https://mirrors.tuna.tsinghua.edu.cn/rustup"
set "R_tuna=https://mirrors.tuna.tsinghua.edu.cn/rustup/rustup"
set "MIRRORS=official rsproxy ustc tuna"
set "RUSTUP_INIT=dist/x86_64-pc-windows-gnu/rustup-init.exe"

set "HAVE_CURL=1"
where curl.exe >nul 2>&1 || set "HAVE_CURL=0"

echo ================================================
echo   Сборка: %APPTITLE%
echo ================================================
echo.

rem ---------- 1. Rust ----------
if exist "%LOCALTC%\cargo\bin\cargo.exe" goto :use_local
where cargo >nul 2>&1
if not errorlevel 1 goto :have_rust
if exist "%USERPROFILE%\.cargo\bin\cargo.exe" goto :use_user
goto :install_rust

:use_local
set "RUSTUP_HOME=%LOCALTC%\rustup"
set "CARGO_HOME=%LOCALTC%\cargo"
set "PATH=%LOCALTC%\cargo\bin;%PATH%"
echo [1/4] Rust: из папки проекта .toolchain
goto :have_rust

:use_user
set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"
goto :have_rust

:install_rust
echo Rust не найден.
echo Его можно поставить прямо в папку проекта - в .toolchain, около 600 МБ.
echo Права администратора не нужны, в системе ничего не меняется.
echo Удалить потом - просто удалить папку .toolchain.
echo Или установите Rust сами с https://rustup.rs и запустите build.bat снова.
echo.
if "%HAVE_CURL%"=="0" goto :no_curl
choice /c YN /n /m "Установить Rust в папку проекта? Y - да, N - нет: "
if errorlevel 2 goto :no_rust
call :pick_rust_mirror
call :install_rustup
if errorlevel 1 goto :fail_rust
goto :use_local

:have_rust
for /f "delims=" %%v in ('cargo --version 2^>nul') do echo [1/4] %%v

rem Внимание: переменную RUSTC использовать нельзя - cargo считает её путём к компилятору.
rem Без rustup используем cargo как есть (например, Rust установлен другим способом).
set "CARGO=cargo"
set "RUSTC_CMD=rustc"
rustup --version >nul 2>&1
if errorlevel 1 goto :toolchain_ok
set "CARGO=cargo +%TOOLCHAIN%"
set "RUSTC_CMD=rustc +%TOOLCHAIN%"
rustup toolchain list | findstr /c:"%TOOLCHAIN%" >nul
if not errorlevel 1 goto :toolchain_ok
echo       Ставлю компилятор для Windows без Visual Studio: %TOOLCHAIN%
if "%HAVE_CURL%"=="1" call :pick_rust_mirror
rustup toolchain install %TOOLCHAIN% --profile minimal
if errorlevel 1 goto :fail_rust

:toolchain_ok

rem ---------- dlltool ----------
rem Библиотеки Windows (windows-link) требуют программу dlltool.exe (и ассемблер as.exe).
rem В PATH добавляется ОТДЕЛЬНАЯ папка .toolchain\dlltool только с этими двумя файлами:
rem если добавить целиком чужую папку bin (w64devkit, MSYS2), Rust возьмёт оттуда
rem чужой компоновщик ld/gcc, и сборка сломается (ошибка "cannot find -lgcc_eh").
set "DLLDIR=%LOCALTC%\dlltool"
rem Если dlltool положили в .toolchain\bin - переносим нужные файлы в отдельную папку.
if exist "%DLLDIR%\dlltool.exe" goto :dlltool_have_local
for %%f in ("%LOCALTC%\bin\dlltool.exe" "%LOCALTC%\bin\as.exe") do if exist "%%~f" (
    if not exist "%DLLDIR%" mkdir "%DLLDIR%"
    copy /y "%%~f" "%DLLDIR%\" >nul
)
:dlltool_have_local
if exist "%DLLDIR%\dlltool.exe" set "PATH=%DLLDIR%;%PATH%"
if exist "%DLLDIR%\dlltool.exe" goto :dlltool_ok
rem Распакованный w64devkit в обычных местах - берём из него только dlltool.exe и as.exe.
for %%d in ("%~dp0w64devkit\bin" "%LOCALTC%\w64devkit\bin" "%USERPROFILE%\w64devkit\bin" "%USERPROFILE%\Downloads\w64devkit\bin" "C:\w64devkit\bin") do (
    if not exist "%DLLDIR%\dlltool.exe" if exist "%%~d\dlltool.exe" if exist "%%~d\as.exe" (
        if not exist "%DLLDIR%" mkdir "%DLLDIR%"
        copy /y "%%~d\dlltool.exe" "%DLLDIR%\" >nul
        copy /y "%%~d\as.exe" "%DLLDIR%\" >nul
        echo       dlltool взят из %%~d
    )
)
if exist "%DLLDIR%\dlltool.exe" set "PATH=%DLLDIR%;%PATH%"
if exist "%DLLDIR%\dlltool.exe" goto :dlltool_ok
where dlltool.exe >nul 2>&1
if not errorlevel 1 goto :dlltool_ok
set "SYSROOT="
for /f "delims=" %%r in ('%RUSTC_CMD% --print sysroot 2^>nul') do set "SYSROOT=%%r"
set "DLLTOOL="
call :find_dlltool
if not defined DLLTOOL rustup component add rust-mingw --toolchain %TOOLCHAIN% >nul 2>&1
if not defined DLLTOOL call :find_dlltool
if not defined DLLTOOL goto :no_dlltool
set "PATH=%DLLTOOL%;%PATH%"
echo       dlltool: %DLLTOOL%
:dlltool_ok

rem ---------- Пути с этого компьютера не попадают в exe ----------
rem Папка проекта превращается в "zvukopad", папка пользователя - в "~".
set "ZV_ROOT=%~dp0"
set "ZV_ROOT=%ZV_ROOT:~0,-1%"
set "ZV_REMAP=--config "build.rustflags=['--remap-path-prefix=%USERPROFILE%=~','--remap-path-prefix=%ZV_ROOT%=zvukopad']""
rem ---------- 2. Библиотеки ----------
rem Выбранный сервер запоминается в .toolchain\crates-mirror.cmd: cargo хранит скачанное
rem отдельно для каждого сервера, поэтому все команды должны использовать один и тот же.
if not "%ZV_MIRROR%"=="" del "%MIRRORF%" >nul 2>&1
if exist "%MIRRORF%" call "%MIRRORF%"
rem Если в Cargo.toml поменяли версию программы - записываем её и в Cargo.lock.
rem --workspace трогает только пакеты самой программы, версии библиотек не меняются.
%CARGO% update --workspace --offline %ZV_CARGO_ARGS% >nul 2>&1
echo.
echo [2/4] Проверяю библиотеки...
%CARGO% fetch --locked --offline --target %TARGET% %ZV_CARGO_ARGS% >nul 2>&1
if not errorlevel 1 goto :fetched
echo       Нужно скачать библиотеки.
if exist "%MIRRORF%" goto :fetch_online
call :pick_crates_mirror
if exist "%MIRRORF%" call "%MIRRORF%"
:fetch_online
%CARGO% update --workspace %ZV_CARGO_ARGS% >nul 2>&1
%CARGO% fetch --locked --target %TARGET% %ZV_CARGO_ARGS%
if errorlevel 1 goto :fail_fetch
:fetched
echo       Библиотеки на месте.

rem ---------- 3. Сборка ----------
echo.
echo [3/4] Компиляция. Первая сборка занимает несколько минут, следующие - секунды.
%CARGO% build --release --locked --offline %ZV_CARGO_ARGS% %ZV_REMAP%
if errorlevel 1 goto :fail_build

rem ---------- 4. Папка с готовой программой ----------
echo.
echo [4/4] Собираю папку release\%APPNAME%
if exist "%OUT%" rmdir /s /q "%OUT%"
mkdir "%OUT%"
copy /y "target\release\%CARGO_EXE%" "%OUT%\%EXENAME%" >nul
if errorlevel 1 goto :fail_copy
robocopy "plugins" "%OUT%\plugins" /E /XD __pycache__ /XF *.pyc *.log /NFL /NDL /NJH /NJS /NP >nul
if errorlevel 8 goto :fail_copy
copy /y "docs\README-user.txt" "%OUT%\README.txt" >nul
copy /y "docs\README-user.en.txt" "%OUT%\README.en.txt" >nul

if exist "build_log.txt" del "build_log.txt" >nul 2>&1
echo.
echo ================================================
echo   Готово!  Программа: release\%APPNAME%\%EXENAME%
echo   Запуск: run.bat
echo   Эту папку можно копировать на любой компьютер с Windows 10/11.
echo ================================================
echo.
if not "%ZV_NOPAUSE%"=="1" pause
exit /b 0


rem =====================================================================
rem  Подпрограммы
rem =====================================================================

rem ---- Поиск dlltool.exe внутри Rust -> DLLTOOL (папка) ----
:find_dlltool
if "%SYSROOT%"=="" exit /b 0
for /f "delims=" %%d in ('where /r "%SYSROOT%" dlltool.exe 2^>nul') do if not defined DLLTOOL set "DLLTOOL=%%~dpd"
exit /b 0

rem ---- Скорость скачивания с сервера, байт/с -> PROBE_SPEED (0, если недоступен) ----
rem Качаем не больше 1 МБ и не дольше 8 секунд. curl выдаёт целое число (иногда с ".000").
:probe
set "PROBE_SPEED=0"
for /f "delims=" %%s in ('curl.exe -s -f -L -o nul -m 10 -w "%%{speed_download}" "%~1" 2^>nul') do set "PROBE_SPEED=%%s"
for /f "delims=.," %%a in ("%PROBE_SPEED%") do set "PROBE_SPEED=%%a"
set /a PROBE_KB=PROBE_SPEED/1024
exit /b 0

rem ---- Ни один сервер не ответил: показываем, что говорит curl ----
:probe_diag
echo       Проверка не удалась ни для одного сервера. Ответ curl:
curl.exe -sS -o nul -m 10 "%~1"
echo       Попробую скачать через cargo напрямую с официального сервера.
exit /b 0

rem ---- Печатает результат замера и запоминает лучший сервер ----
:report
if %PROBE_SPEED% LEQ 0 (echo         %1: недоступен) else (echo         %1: %PROBE_KB% КБ/с)
if %PROBE_SPEED% GTR %BEST_SPEED% set "BEST=%1"
if %PROBE_SPEED% GTR %BEST_SPEED% set "BEST_SPEED=%PROBE_SPEED%"
exit /b 0

rem ---- Выбор сервера библиотек -> файл %MIRRORF% ----
:pick_crates_mirror
if not exist "%LOCALTC%" mkdir "%LOCALTC%"
set "BEST=official"
if not "%ZV_MIRROR%"=="" (
    set "BEST=%ZV_MIRROR%"
    echo       Сервер задан вручную: %ZV_MIRROR%
    goto :write_crates_mirror
)
if "%HAVE_CURL%"=="0" goto :write_crates_mirror
echo       Выбираю самый быстрый сервер:
set "BEST_SPEED=0"
for %%m in (%MIRRORS%) do call :probe_crates %%m
if "%BEST_SPEED%"=="0" call :probe_diag "%C_official%config.json"
echo       Выбран: %BEST%
:write_crates_mirror
set "IDX="
call set "IDX=%%C_%BEST%%%"
if "%IDX%"=="" set "BEST=official"
if "%IDX%"=="" set "IDX=%C_official%"
if "%BEST%"=="official" (
    >"%MIRRORF%" echo set "ZV_CARGO_ARGS="
) else (
    >"%MIRRORF%" echo set "ZV_CARGO_ARGS=--config source.crates-io.replace-with='zvmirror' --config source.zvmirror.registry='sparse+%IDX%'"
)
exit /b 0

:probe_crates
rem Файл индекса популярной библиотеки - одинаковый путь на всех серверах.
call set "U=%%C_%1%%"
call :probe "%U%wi/nd/windows-sys"
call :report %1
exit /b 0

rem ---- Выбор сервера Rust -> RUSTUP_DIST_SERVER, RUSTUP_UPDATE_ROOT ----
:pick_rust_mirror
set "RUST_MIRROR=official"
if not "%ZV_MIRROR%"=="" (
    set "RUST_MIRROR=%ZV_MIRROR%"
    goto :set_rust_mirror
)
echo       Выбираю самый быстрый сервер Rust:
set "BEST=official"
set "BEST_SPEED=0"
for %%m in (%MIRRORS%) do call :probe_rust %%m
set "RUST_MIRROR=%BEST%"
echo       Выбран: %RUST_MIRROR%
:set_rust_mirror
set "RUSTUP_DIST_SERVER="
call set "RUSTUP_DIST_SERVER=%%D_%RUST_MIRROR%%%"
call set "RUSTUP_UPDATE_ROOT=%%R_%RUST_MIRROR%%%"
if "%RUSTUP_DIST_SERVER%"=="" set "RUSTUP_UPDATE_ROOT=%R_official%"
if "%RUSTUP_DIST_SERVER%"=="" set "RUSTUP_DIST_SERVER=%D_official%"
exit /b 0

:probe_rust
call set "U=%%R_%1%%"
call :probe "%U%/%RUSTUP_INIT%.sha256"
call :report %1
exit /b 0

rem ---- Установка rustup в .toolchain с проверкой SHA-256 ----
:install_rustup
set "INIT=%LOCALTC%\rustup-init.exe"
set "SUMF=%LOCALTC%\rustup-init.sha256"
if not exist "%LOCALTC%" mkdir "%LOCALTC%"
echo       Скачиваю установщик Rust: %RUSTUP_UPDATE_ROOT%/%RUSTUP_INIT%
curl.exe -f -L --progress-bar -o "%INIT%" "%RUSTUP_UPDATE_ROOT%/%RUSTUP_INIT%"
if errorlevel 1 exit /b 1
rem Контрольная сумма - с официального сервера; если он недоступен - с выбранного.
curl.exe -s -f -L -m 15 -o "%SUMF%" "%R_official%/%RUSTUP_INIT%.sha256"
if errorlevel 1 curl.exe -s -f -L -m 15 -o "%SUMF%" "%RUSTUP_UPDATE_ROOT%/%RUSTUP_INIT%.sha256"
if errorlevel 1 goto :sum_failed
set "EXPECTED="
for /f "usebackq tokens=1" %%h in ("%SUMF%") do if not defined EXPECTED set "EXPECTED=%%h"
set "ACTUAL="
for /f "skip=1 delims=" %%h in ('certutil -hashfile "%INIT%" SHA256') do if not defined ACTUAL set "ACTUAL=%%h"
set "ACTUAL=%ACTUAL: =%"
if /i not "%ACTUAL%"=="%EXPECTED%" goto :sum_mismatch
echo       Контрольная сумма SHA-256 совпала.
set "RUSTUP_HOME=%LOCALTC%\rustup"
set "CARGO_HOME=%LOCALTC%\cargo"
echo       Устанавливаю Rust (несколько минут)...
"%INIT%" -y --no-modify-path --profile minimal --default-host x86_64-pc-windows-gnu --default-toolchain stable
set "RC=%errorlevel%"
del "%INIT%" "%SUMF%" >nul 2>&1
exit /b %RC%

:sum_failed
echo       Не удалось получить контрольную сумму.
del "%INIT%" >nul 2>&1
exit /b 1

:sum_mismatch
echo       Контрольная сумма НЕ совпала - файл удалён, установка отменена.
del "%INIT%" "%SUMF%" >nul 2>&1
exit /b 1


rem =====================================================================
rem  Ошибки
rem =====================================================================

:no_dlltool
echo.
echo ОШИБКА: не найдена программа dlltool.exe - она нужна для библиотек Windows,
echo а в этой установке Rust её нет. Самый простой выход:
echo   1. Скачайте w64devkit: https://github.com/skeeto/w64devkit/releases
echo      (файл w64devkit-x64-...exe - это самораспаковывающийся архив).
echo   2. Распакуйте его в папку проекта (получится папка w64devkit рядом с build.bat)
echo      или в C:\w64devkit - build.bat сам возьмёт оттуда только dlltool.exe и as.exe.
echo      Можно и вручную: скопируйте ТОЛЬКО эти два файла из w64devkit\bin
echo      в папку .toolchain\dlltool внутри проекта. Папку w64devkit\bin в PATH не добавляйте.
echo   3. Запустите build.bat снова.
echo Путь к Rust для справки: %SYSROOT%
goto :fail_end

:no_curl
echo В этой Windows нет curl.exe (он есть в Windows 10 начиная с версии 1803).
echo Установите Rust вручную с https://rustup.rs и запустите build.bat снова.
goto :fail_end

:no_rust
echo.
echo Без Rust собрать программу нельзя. Установите Rust с https://rustup.rs
echo или запустите build.bat ещё раз и согласитесь на установку в папку проекта.
goto :fail_end

:fail_rust
echo.
echo ОШИБКА: не удалось установить Rust. Проверьте интернет и запустите build.bat ещё раз.
goto :fail_end

:fail_fetch
rem Сервер мог перестать работать - в следующий раз выберем заново.
del "%MIRRORF%" >nul 2>&1
echo.
echo ОШИБКА: не удалось скачать библиотеки. Проверьте интернет и запустите build.bat ещё раз -
echo уже скачанное не пропадёт. Можно выбрать сервер вручную: set ZV_MIRROR=official
goto :fail_end

:fail_build
echo.
echo ОШИБКА компиляции. Сохраняю подробности в build_log.txt ...
%CARGO% build --release --locked --offline --color never %ZV_CARGO_ARGS% %ZV_REMAP% > build_log.txt 2>&1
echo Пришлите файл build_log.txt тому, кто помогает с программой.
goto :fail_end

:fail_copy
echo.
echo ОШИБКА: не удалось скопировать файлы в release\%APPNAME%.
echo Возможно, программа сейчас запущена - закройте её и повторите.
goto :fail_end

:fail_end
if not "%ZV_NOPAUSE%"=="1" pause
exit /b 1
