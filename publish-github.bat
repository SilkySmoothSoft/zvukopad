@echo off
chcp 65001 >nul
setlocal EnableExtensions
cd /d "%~dp0"
title Выкладка исходников на GitHub

rem ======================================================================
rem  Выкладывает ИСХОДНИКИ на GitHub (только то, что нужно для сборки -
rem  тот же набор, что pack.bat кладёт в dist\Zvukopad-src-ВЕРСИЯ).
rem
rem  Ничего не сохраняет: ни пароль/токен, ни имя, ни адрес репозитория.
rem  Глобальные настройки git и переменные Windows не меняются.
rem  Всё спрашивается заново при каждом запуске.
rem
rem  Как работает:
rem   1. pack.bat собирает чистые исходники в dist\.
rem   2. Репозиторий скачивается во временную папку (%TEMP%).
rem   3. Старые файлы там заменяются новыми, делается коммит.
rem   4. git push. Токен вводится в окне git и нигде не запоминается.
rem   5. Временная папка удаляется.
rem
rem  Готовую программу (exe) выкладывайте через Releases на сайте -
rem  см. PUBLISH-GITHUB.txt. Этот батник и инструкция сами не выкладываются.
rem ======================================================================

echo ================================================
echo   Выкладка исходников на GitHub
echo ================================================
echo.

where git >nul 2>&1
if errorlevel 1 goto :no_git

rem ---------- Вопросы ----------
set "REPO=%GHREPO%"
if defined REPO goto :have_repo
set /p "REPO=Адрес репозитория (например https://github.com/имя/zvukopad): "
if "%REPO%"=="" goto :cancel
:have_repo
rem Убираем лишнее в конце: "/" и ".git"
if "%REPO:~-1%"=="/" set "REPO=%REPO:~0,-1%"
if /i "%REPO:~-4%"==".git" set "REPO=%REPO:~0,-4%"
if /i not "%REPO:~0,19%"=="https://github.com/" goto :bad_repo

set "GHUSER=%GHLOGIN%"
if defined GHUSER goto :have_user
set /p "GHUSER=Ваш логин на GitHub: "
if "%GHUSER%"=="" goto :cancel
:have_user

set "BRANCH="
set /p "BRANCH=Ветка [Enter = main]: "
if "%BRANCH%"=="" set "BRANCH=main"

set "AUTHOR="
set /p "AUTHOR=Имя автора коммита [Enter = %GHUSER%]: "
if "%AUTHOR%"=="" set "AUTHOR=%GHUSER%"

echo Почта автора коммита. Чтобы не светить настоящую, можно указать
echo   %GHUSER%@users.noreply.github.com
set "EMAIL="
set /p "EMAIL=Почта [Enter = %GHUSER%@users.noreply.github.com]: "
if "%EMAIL%"=="" set "EMAIL=%GHUSER%@users.noreply.github.com"

rem ---------- Версия и исходники ----------
set "VER="
for /f "tokens=2 delims==" %%v in ('findstr /b /c:"version" Cargo.toml') do if not defined VER set "VER=%%v"
set VER=%VER: =%
set VER=%VER:"=%

set "MSG="
set /p "MSG=Описание изменений [Enter = Версия %VER%]: "
if "%MSG%"=="" set "MSG=Версия %VER%"

echo.
echo [1/4] Собираю чистые исходники (pack.bat)...
set "ZV_NOPAUSE=1"
call "%~dp0pack.bat" >nul
set "SRC=%~dp0dist\Zvukopad-src-%VER%"
if not exist "%SRC%\Cargo.toml" goto :no_src
echo       %SRC%

rem ---------- Временная копия репозитория ----------
set "WORK=%TEMP%\zvukopad-publish-%RANDOM%%RANDOM%"
echo.
echo [2/4] Скачиваю репозиторий во временную папку...
rem credential.helper= - git не запоминает и не подставляет сохранённые пароли.
set "CLONEURL=%REPO%.git"
if defined GHTOKEN set "CLONEURL=https://%GHUSER%:%GHTOKEN%@%REPO:https://=%.git"
git -c credential.helper= clone --quiet "%CLONEURL%" "%WORK%"
if errorlevel 1 goto :fail_clone
rem Убираем токен из настроек временной копии
git -C "%WORK%" remote set-url origin "%REPO%.git"

rem ---------- Замена файлов ----------
echo.
echo [3/4] Обновляю файлы...
pushd "%WORK%"
rem Ветка: если её ещё нет (пустой репозиторий) - создаём.
git checkout --quiet "%BRANCH%" 2>nul || git checkout --quiet -b "%BRANCH%"
rem Удаляем всё, кроме папки .git: файлы, которых больше нет в исходниках, тоже уйдут из репозитория.
for /f "delims=" %%f in ('dir /b /a') do (
    if /i not "%%f"==".git" (
        if exist "%%f\*" (rmdir /s /q "%%f") else (del /f /q "%%f")
    )
)
robocopy "%SRC%" "%WORK%" /E /NFL /NDL /NJH /NJS /NP >nul
if errorlevel 8 goto :fail_copy
rem Не выкладываем сценарии GitHub Actions
if exist "%WORK%\.github" rmdir /s /q "%WORK%\.github"
git add -A
set "CHANGES="
for /f "delims=" %%s in ('git status --porcelain') do set "CHANGES=1"
if not defined CHANGES goto :no_changes
echo       Изменения:
git status --short
echo.
git -c user.name="%AUTHOR%" -c user.email="%EMAIL%" commit --quiet -m "%MSG%"
if errorlevel 1 goto :fail_commit

rem ---------- Отправка ----------
echo.
echo [4/4] Отправляю на GitHub.
echo       Когда git спросит пароль - вставьте ТОКЕН (Personal Access Token), не пароль от сайта.
echo       При вводе символы не показываются - это нормально. Вставка: правый клик или Ctrl+V.
echo.
set "GIT_TERMINAL_PROMPT=1"
set "GCM_INTERACTIVE=never"
set "PUSHURL=https://%GHUSER%@%REPO:https://=%.git"
if defined GHTOKEN set "PUSHURL=https://%GHUSER%:%GHTOKEN%@%REPO:https://=%.git"
if defined GHTOKEN set "GIT_TERMINAL_PROMPT=0"
git -c credential.helper= push "%PUSHURL%" "HEAD:%BRANCH%"
if errorlevel 1 goto :fail_push
popd
attrib -r "%WORK%\*" /s /d >nul 2>&1
rmdir /s /q "%WORK%" 2>nul

echo.
echo ================================================
echo   Готово: исходники версии %VER% на GitHub
echo   %REPO%
echo.
echo   Готовая программа: dist\Zvukopad-%VER%-win64.zip
echo   Её выкладывают в Releases (см. PUBLISH-GITHUB.txt).
echo ================================================
start "" "%REPO%"
pause
exit /b 0

:no_changes
popd
attrib -r "%WORK%\*" /s /d >nul 2>&1
rmdir /s /q "%WORK%" 2>nul
echo.
echo Изменений нет - на GitHub уже такие же файлы. Ничего не отправлено.
pause
exit /b 0

:no_git
echo Не найден git. Установите Git for Windows с официального сайта:
echo   https://git-scm.com/download/win
echo (или портативную версию оттуда же) и запустите этот файл снова.
pause
exit /b 1

:bad_repo
echo Адрес должен начинаться с https://github.com/
pause
exit /b 1

:no_src
echo Не удалось собрать исходники - запустите pack.bat отдельно и посмотрите ошибку.
pause
exit /b 1

:fail_clone
echo.
echo Не удалось скачать репозиторий. Проверьте адрес. Если репозиторий приватный,
echo git спросит логин и токен. Создать репозиторий: https://github.com/new
pause
exit /b 1

:fail_copy
popd
echo Не удалось скопировать файлы во временную папку %WORK%
pause
exit /b 1

:fail_commit
popd
echo Не удалось сделать коммит. Временная папка оставлена для проверки: %WORK%
pause
exit /b 1

:fail_push
popd
echo.
echo Отправка не удалась. Частые причины:
echo   - вставлен пароль от сайта вместо токена;
echo   - у токена нет прав на этот репозиторий (нужно право Contents: Read and write);
echo   - на GitHub появились изменения, которых нет здесь.
echo Временная папка: %WORK%  (можно удалить)
pause
exit /b 1

:cancel
echo Отменено.
pause
exit /b 1
