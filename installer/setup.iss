#ifndef AppVersion
  #define AppVersion "0.2.0"
#endif
#ifndef PackageDir
  #define PackageDir "..\dist\FujiRecipeStudio"
#endif
#ifndef OutputPath
  #define OutputPath "..\dist\installer"
#endif

[Setup]
AppId={{FB65651D-A047-47A0-A170-04A9DA899D07}
AppName=Fuji Recipe Studio
AppVersion={#AppVersion}
AppPublisher=Stefan Radovanovic
AppPublisherURL=https://github.com/stefanradovanovic/fuji-recipe-studio
AppSupportURL=https://github.com/stefanradovanovic/fuji-recipe-studio/issues
AppUpdatesURL=https://github.com/stefanradovanovic/fuji-recipe-studio/releases/latest
VersionInfoVersion={#AppVersion}.0
DefaultDirName={localappdata}\Programs\Fuji Recipe Studio
DefaultGroupName=Fuji Recipe Studio
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
OutputDir={#OutputPath}
OutputBaseFilename=FujiRecipeStudio-Setup-{#AppVersion}-x64
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
LicenseFile={#PackageDir}\LICENSE
UninstallDisplayIcon={app}\FujiRecipeStudio.exe
UninstallDisplayName=Fuji Recipe Studio
CloseApplications=yes
RestartApplications=no
SetupLogging=yes

[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"; GroupDescription: "Shortcuts:"; Flags: unchecked

[Files]
Source: "{#PackageDir}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{group}\Fuji Recipe Studio"; Filename: "{app}\FujiRecipeStudio.exe"; WorkingDir: "{app}"
Name: "{autodesktop}\Fuji Recipe Studio"; Filename: "{app}\FujiRecipeStudio.exe"; WorkingDir: "{app}"; Tasks: desktopicon

[Run]
Filename: "{app}\FujiRecipeStudio.exe"; Description: "Launch Fuji Recipe Studio"; Flags: nowait postinstall skipifsilent; Check: NotUpdater
Filename: "{app}\FujiRecipeStudio.exe"; Flags: nowait; Check: IsUpdater

[Code]
function IsUpdater: Boolean;
begin
  Result := ExpandConstant('{param:RESTARTAPP|0}') = '1';
end;

function NotUpdater: Boolean;
begin
  Result := not IsUpdater;
end;
