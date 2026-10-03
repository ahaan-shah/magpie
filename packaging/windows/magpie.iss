; Magpie installer (Inno Setup 6.3+). Built by scripts/package-windows.ps1:
;   iscc /DVersion=0.2.1 /DSrcX64=...\magpie.exe /DSrcArm64=...\magpie.exe magpie.iss
;
; One installer for x64 and ARM64 PCs: it installs the native build for the
; machine. Installs per user by default (no admin prompt) into
; %LOCALAPPDATA%\Programs\Magpie; "Install for all users" is offered too.
; Upgrades install in place (same AppId) and close a running Magpie first.
; Uninstalling leaves your data (in %APPDATA%\magpie) alone.

#ifndef Version
  #define Version "0.0.0"
#endif
#ifndef SrcX64
  #define SrcX64 "..\..\target\x86_64-pc-windows-msvc\dist\magpie.exe"
#endif
#ifndef SrcArm64
  #define SrcArm64 "..\..\target\aarch64-pc-windows-msvc\dist\magpie.exe"
#endif
#ifndef OutDir
  #define OutDir "..\..\dist"
#endif

[Setup]
; Never change AppId: it's how Windows recognises an upgrade.
AppId={{06DA674B-CF91-47D1-8E12-B948514C4EF3}
AppName=Magpie
AppVersion={#Version}
AppVerName=Magpie {#Version}
AppPublisher=Ahaan Shah
AppPublisherURL=https://github.com/ahaan-shah/magpie
AppSupportURL=https://github.com/ahaan-shah/magpie/issues
AppUpdatesURL=https://github.com/ahaan-shah/magpie/releases
VersionInfoVersion={#Version}
VersionInfoProductName=Magpie
DefaultDirName={autopf}\Magpie
DefaultGroupName=Magpie
DisableProgramGroupPage=yes
DisableDirPage=auto
DisableReadyPage=yes
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog commandline
ArchitecturesAllowed=x64compatible or arm64
ArchitecturesInstallIn64BitMode=x64compatible or arm64
MinVersion=10.0.17763
OutputDir={#OutDir}
OutputBaseFilename=Magpie-windows-setup
SetupIconFile=..\..\crates\app\assets\magpie.ico
UninstallDisplayIcon={app}\magpie.exe
UninstallDisplayName=Magpie
WizardStyle=modern
Compression=lzma2/max
SolidCompression=yes
CloseApplications=force
RestartApplications=no
SetupLogging=yes

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "{#SrcX64}"; DestDir: "{app}"; DestName: "magpie.exe"; Check: not IsArm64; Flags: ignoreversion
Source: "{#SrcArm64}"; DestDir: "{app}"; DestName: "magpie.exe"; Check: IsArm64; Flags: ignoreversion
Source: "..\..\LICENSE"; DestDir: "{app}"; DestName: "LICENSE.txt"; Flags: ignoreversion
Source: "..\..\README.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\THIRD-PARTY-LICENSES.txt"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\Magpie"; Filename: "{app}\magpie.exe"; Comment: "Personal finance tracker"
Name: "{autodesktop}\Magpie"; Filename: "{app}\magpie.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\magpie.exe"; Description: "{cm:LaunchProgram,Magpie}"; Flags: nowait postinstall skipifsilent
