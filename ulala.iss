; Ulala installer (Inno Setup 6). Build: cargo build --release, then
;   "%LOCALAPPDATA%\Programs\Inno Setup 6\ISCC.exe" ulala.iss
; -> target\installer\ulala-setup.exe. The exe is self-contained (icons embedded by build.rs); settings live in
; %LOCALAPPDATA%\ulala and are left alone on uninstall.

; Keep in step with Cargo.toml.
#define Version "0.1.0"

[Setup]
AppId={{6C1F7E2A-3D5B-4E8A-9B1C-0A2D4F6E8B13}
AppName=Ulala
AppVersion={#Version}
AppPublisher=kachi-dev
AppPublisherURL=https://github.com/kachi-dev/ulala-sim
DefaultDirName={autopf}\Ulala
DefaultGroupName=Ulala
DisableProgramGroupPage=yes
; Per-user by default (no UAC prompt); the user can still pick all-users.
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir=target\installer
OutputBaseFilename=ulala-setup
SetupIconFile=assets\img\icon.ico
UninstallDisplayIcon={app}\ulala.exe
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern

[Tasks]
Name: desktopicon; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"

[Files]
Source: "target\release\ulala.exe"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\Ulala"; Filename: "{app}\ulala.exe"
Name: "{autodesktop}\Ulala"; Filename: "{app}\ulala.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\ulala.exe"; Description: "{cm:LaunchProgram,Ulala}"; Flags: nowait postinstall skipifsilent
