[Setup]
AppId={#AppId}
AppName={#AppName}
AppVerName={#AppDisplayName}
AppPublisher=Rdg
AppPublisherURL=https://github.com/RDG-Labs/rdg
AppSupportURL=https://github.com/RDG-Labs/rdg/issues
AppUpdatesURL=https://github.com/RDG-Labs/rdg/releases
DefaultGroupName={#AppName}
DisableProgramGroupPage=yes
DisableReadyPage=yes
AllowNoIcons=yes
OutputDir={#OutputDir}
OutputBaseFilename={#AppSetupName}
Compression=lzma
SolidCompression=yes
SetupMutex={#AppMutex}Setup
CloseApplications=force
; AppMutex is delegated to the [Code] section so the mutex is released during updates.
AppMutex={code:GetAppMutex}
SetupIconFile={#ResourcesDir}\app-icon.ico
UninstallDisplayIcon={app}\rdg.exe
ChangesEnvironment=true
MinVersion=10.0.16299
SourceDir={#SourceDir}
AppVersion={#Version}
VersionInfoVersion={#Version}
ShowLanguageDialog=auto
WizardStyle=modern

DefaultDirName={autopf}\{#AppName}
PrivilegesRequired=lowest

ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[UninstallDelete]
; Delete the update staging areas left behind by the auto-updater.
Type: filesandordirs; Name: "{app}\tools"
Type: filesandordirs; Name: "{app}\updates"
Type: filesandordirs; Name: "{app}\install"
Type: filesandordirs; Name: "{app}\old"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "{#ResourcesDir}\rdg.exe"; DestDir: "{code:GetInstallDir}"; Flags: ignoreversion
Source: "{#ResourcesDir}\conpty.dll"; DestDir: "{code:GetInstallDir}"; Flags: ignoreversion
Source: "{#ResourcesDir}\tools\*"; DestDir: "{app}\tools"; Flags: ignoreversion

[Icons]
Name: "{group}\{#AppName}"; Filename: "{app}\rdg.exe"; AppUserModelID: "{#AppUserId}"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\rdg.exe"; Tasks: desktopicon; AppUserModelID: "{#AppUserId}"

[Run]
Filename: "{app}\rdg.exe"; Description: "{cm:LaunchProgram,{#AppName}}"; Flags: nowait postinstall skipifsilent

[Code]
function IsUpdating(): Boolean;
begin
  Result := SwitchHasValue('update', 'true') and WizardSilent();
end;

function GetAppMutex(Param: string): string;
begin
  if IsUpdating() then
    Result := ''
  else
    Result := ExpandConstant('{#AppMutex}');
end;

function GetInstallDir(Param: string): string;
begin
  if IsUpdating() then
    Result := ExpandConstant('{app}\install')
  else
    Result := ExpandConstant('{app}');
end;

procedure CurStepChanged(CurStep: TSetupStep);
begin
  if CurStep = ssPostInstall then
  begin
    if IsUpdating() then
      SaveStringToFile(ExpandConstant('{app}\updates\versions.txt'), '{#Version}' + #13#10, True);
  end;
end;