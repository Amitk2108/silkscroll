; SilkScroll installer (Inno Setup 6.3 or newer).
;
; Build it with installer\build.ps1, which passes the version, the exe and the
; icon. Installs per user (no administrator prompt) into
; %LOCALAPPDATA%\Programs\SilkScroll. Running a newer setup upgrades the
; existing installation in place: it closes any running SilkScroll first
; (installed or portable), replaces the exe, keeps the user's settings and
; starts the new version.

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif
#ifndef ExePath
  #define ExePath "..\target\release\silkscroll.exe"
#endif

#define AppName "SilkScroll"
#define AppUrl "https://github.com/Amitk2108/silkscroll"

[Setup]
; Never change AppId: it is how Windows recognizes an existing installation.
AppId={{C5FF4B62-C1C9-4D7F-815E-FBEB76F4AB64}
AppName={#AppName}
AppVersion={#AppVersion}
AppVerName={#AppName} {#AppVersion}
AppPublisher=SilkScroll contributors
AppPublisherURL={#AppUrl}
AppSupportURL={#AppUrl}/issues
AppUpdatesURL={#AppUrl}/releases
AppCopyright=Copyright (c) 2026 SilkScroll contributors. MIT License.
VersionInfoVersion={#AppVersion}
VersionInfoProductName={#AppName}
VersionInfoDescription={#AppName} Setup
DefaultDirName={autopf}\{#AppName}
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=commandline
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
LicenseFile=..\LICENSE
WizardStyle=modern
#ifdef IconFile
SetupIconFile={#IconFile}
#endif
UninstallDisplayIcon={app}\silkscroll.exe
UninstallDisplayName={#AppName}
OutputBaseFilename=SilkScroll-Setup
Compression=lzma2/ultra64
SolidCompression=yes

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "{#ExePath}"; DestDir: "{app}"; DestName: "silkscroll.exe"; Flags: ignoreversion
Source: "..\LICENSE"; DestDir: "{app}"; DestName: "LICENSE.txt"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\{#AppName}"; Filename: "{app}\silkscroll.exe"; Comment: "Smooth scrolling for every Windows app"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\silkscroll.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\silkscroll.exe"; Description: "{cm:LaunchProgram,{#AppName}}"; Flags: nowait postinstall skipifsilent

[Code]
const
  WM_CLOSE = $0010;
  WM_APP_QUIT = $800B; { WM_APP + 11, see src/instance.rs }
  RUN_KEY = 'Software\Microsoft\Windows\CurrentVersion\Run';

{ Closes every running SilkScroll, whichever folder it runs from, and waits
  until it has exited so its exe can be replaced or removed. }
procedure StopSilkScroll();
var
  Wnd: HWND;
  I: Integer;
begin
  Wnd := FindWindowByClassName('SilkScroll.Tray');
  if Wnd <> 0 then
    PostMessage(Wnd, WM_APP_QUIT, 0, 0);
  Wnd := FindWindowByClassName('SilkScroll.Settings');
  if Wnd <> 0 then
    PostMessage(Wnd, WM_CLOSE, 0, 0);
  for I := 1 to 100 do
  begin
    if not CheckForMutexes('Local\SilkScroll.Engine,Local\SilkScroll.Settings') then
      Break;
    Sleep(100);
  end;
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
begin
  StopSilkScroll();
  Result := '';
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep = usUninstall then
    StopSilkScroll();
  if CurUninstallStep = usPostUninstall then
  begin
    RegDeleteValue(HKEY_CURRENT_USER, RUN_KEY, 'SilkScroll');
    if not UninstallSilent() then
      if MsgBox('Do you also want to delete your SilkScroll settings?', mbConfirmation, MB_YESNO or MB_DEFBUTTON2) = IDYES then
        DelTree(ExpandConstant('{userappdata}\SilkScroll'), True, True, True);
  end;
end;
