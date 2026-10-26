@echo off
setlocal
set "DISKLENS_EXE=%~dp0Livraison\DiskLens.exe"
if not exist "%DISKLENS_EXE%" set "DISKLENS_EXE=%~dp0_cache_build\release\disklens.exe"
if not exist "%DISKLENS_EXE%" set "DISKLENS_EXE=%~dp0src-tauri\target\release\disklens.exe"
if exist "%DISKLENS_EXE%" (
  start "" "%DISKLENS_EXE%"
  exit /b 0
)
echo DiskLens n'est pas encore compile en version release.
echo Consultez README.md pour les commandes de compilation.
pause
exit /b 1
