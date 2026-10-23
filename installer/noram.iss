#ifndef AppVersion
  #define AppVersion "0.2.0"
#endif
[Setup]
AppId={{D5100EA0-2AA5-4E99-987C-185DB3CA51AC}
AppName=NoRAM Browser
AppVersion={#AppVersion}
AppPublisher=NoRAM Browser
AppPublisherURL=https://github.com/ZitXCheat1/noram-browser
DefaultDirName={localappdata}\Programs\NoRAM Browser
DefaultGroupName=NoRAM Browser
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir=..\artifacts\installer
OutputBaseFilename=NoRAM-Setup-x64
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
CloseApplications=yes
RestartApplications=no
UninstallDisplayIcon={app}\noram-browser.exe
LicenseFile=..\LICENSE
[Files]
Source: "..\target\release\noram-browser.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\LICENSE"; DestDir: "{app}"
Source: "..\THIRD_PARTY_NOTICES.md"; DestDir: "{app}"
Source: "..\tools\browser-cookie-export\*"; DestDir: "{app}\tools\browser-cookie-export"; Flags: recursesubdirs
Source: "..\docs\IMPORT.md"; DestDir: "{app}\docs"
[Icons]
Name: "{group}\NoRAM Browser"; Filename: "{app}\noram-browser.exe"
Name: "{userdesktop}\NoRAM Browser"; Filename: "{app}\noram-browser.exe"; Tasks: desktopicon
[Tasks]
Name: desktopicon; Description: "Create a desktop shortcut"; Flags: unchecked
[Registry]
Root: HKCU; Subkey: "Software\NoRAMBrowser\Capabilities"; ValueType: string; ValueName: "ApplicationName"; ValueData: "NoRAM Browser"; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\NoRAMBrowser\Capabilities"; ValueType: string; ValueName: "ApplicationDescription"; ValueData: "Browse with native request blocking and protected sleeping tabs"
Root: HKCU; Subkey: "Software\NoRAMBrowser\Capabilities\URLAssociations"; ValueType: string; ValueName: "http"; ValueData: "NoRAMBrowser.URL"
Root: HKCU; Subkey: "Software\NoRAMBrowser\Capabilities\URLAssociations"; ValueType: string; ValueName: "https"; ValueData: "NoRAMBrowser.URL"
Root: HKCU; Subkey: "Software\RegisteredApplications"; ValueType: string; ValueName: "NoRAM Browser"; ValueData: "Software\NoRAMBrowser\Capabilities"; Flags: uninsdeletevalue
Root: HKCU; Subkey: "Software\Classes\NoRAMBrowser.URL"; ValueType: string; ValueData: "NoRAM Web Document"; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\Classes\NoRAMBrowser.URL"; ValueType: string; ValueName: "URL Protocol"; ValueData: ""
Root: HKCU; Subkey: "Software\Classes\NoRAMBrowser.URL\DefaultIcon"; ValueType: string; ValueData: "{app}\noram-browser.exe,0"
Root: HKCU; Subkey: "Software\Classes\NoRAMBrowser.URL\shell\open\command"; ValueType: string; ValueData: """{app}\noram-browser.exe"" ""%1"""
[Run]
Filename: "ms-settings:defaultapps"; Description: "Choose your default browser"; Flags: shellexec postinstall skipifsilent unchecked
Filename: "{app}\noram-browser.exe"; Description: "Launch NoRAM Browser"; Flags: nowait postinstall skipifsilent unchecked
[Code]
function InitializeSetup(): Boolean;
var RuntimeVersion: String;
begin
  Result := RegQueryStringValue(HKLM32, 'Software\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}', 'pv', RuntimeVersion)
    or RegQueryStringValue(HKCU, 'Software\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}', 'pv', RuntimeVersion);
  Result := Result and (RuntimeVersion <> '') and (RuntimeVersion <> '0.0.0.0');
  if not Result then
    MsgBox('Install the Microsoft Evergreen WebView2 Runtime, then run this installer again. Download it from https://developer.microsoft.com/microsoft-edge/webview2/', mbError, MB_OK);
end;
