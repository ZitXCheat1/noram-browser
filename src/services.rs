use crate::Action;
use anyhow::Result;
use serde_json::json;
use std::{
    os::windows::process::CommandExt,
    path::PathBuf,
    process::Command,
    sync::mpsc::{self, Sender},
    thread,
    time::Duration,
};
use tao::event_loop::EventLoopProxy;
use windows::{
    core::w,
    Win32::{
        System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER},
        UI::Shell::{
            FileOpenDialog, IFileOpenDialog, FOS_FORCEFILESYSTEM, FOS_PICKFOLDERS,
            SIGDN_FILESYSPATH,
        },
    },
};

pub fn profile_writer(
    profile: PathBuf,
    proxy: EventLoopProxy<Action>,
) -> (Sender<Vec<u8>>, thread::JoinHandle<()>) {
    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    let thread = thread::spawn(move || {
        while let Ok(mut bytes) = rx.recv() {
            let mut disconnected = false;
            loop {
                match rx.recv_timeout(Duration::from_millis(120)) {
                    Ok(next) => bytes = next,
                    Err(mpsc::RecvTimeoutError::Disconnected) => {
                        disconnected = true;
                        break;
                    }
                    Err(_) => break,
                }
            }
            let tmp = profile.join("browser.json.tmp");
            let result = std::fs::write(&tmp, bytes)
                .and_then(|_| std::fs::rename(&tmp, profile.join("browser.json")));
            if let Err(e) = result {
                let _ = proxy.send_event(Action::Message(format!("Could not save profile: {e}")));
            }
            if disconnected {
                break;
            }
        }
    });
    (tx, thread)
}
pub fn pick_folder() -> Result<Option<PathBuf>> {
    unsafe {
        let dialog: IFileOpenDialog =
            CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER)?;
        dialog.SetOptions(FOS_PICKFOLDERS | FOS_FORCEFILESYSTEM)?;
        dialog.SetTitle(w!("Choose an unpacked extension folder"))?;
        if let Err(e) = dialog.Show(None) {
            if e.code().0 as u32 == 0x800704c7 {
                return Ok(None);
            }
            return Err(e.into());
        }
        let item = dialog.GetResult()?;
        let path = webview2_com::take_pwstr(item.GetDisplayName(SIGDN_FILESYSPATH)?);
        Ok(Some(PathBuf::from(path)))
    }
}
fn system_exe(name: &str) -> PathBuf {
    PathBuf::from(std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into()))
        .join("System32")
        .join(name)
}
pub fn vpn_refresh(proxy: EventLoopProxy<Action>) {
    thread::spawn(move || {
        let output=Command::new(system_exe("WindowsPowerShell\\v1.0\\powershell.exe"))
            .creation_flags(0x08000000).args(["-NoProfile","-NonInteractive","-Command",r#"[Console]::OutputEncoding=[System.Text.UTF8Encoding]::new();try{$items=@(Get-VpnConnection -ErrorAction Stop);ConvertTo-Json -Compress -Depth 3 -InputObject @($items|Select-Object Name,@{Name='ConnectionStatus';Expression={$_.ConnectionStatus.ToString()}},@{Name='TunnelType';Expression={$_.TunnelType.ToString()}})}catch{[Console]::Error.WriteLine($_.Exception.Message);exit 1}"#]).output();
        let value = match output {
            Ok(out) if out.status.success() => {
                match serde_json::from_slice::<serde_json::Value>(&out.stdout) {
                    Ok(profiles) => json!({"loaded":true,"profiles":profiles,"error":null}),
                    Err(e) => {
                        json!({"loaded":true,"profiles":[],"error":format!("Could not read VPN profiles: {e}")})
                    }
                }
            }
            Ok(_) => {
                json!({"loaded":true,"profiles":[],"error":"Windows VPN profiles could not be read. Open Windows VPN settings to configure a connection."})
            }
            Err(e) => json!({"loaded":true,"profiles":[],"error":e.to_string()}),
        };
        let _ = proxy.send_event(Action::Vpn(value));
    });
}
pub fn vpn_action(name: String, connect: bool, proxy: EventLoopProxy<Action>) {
    thread::spawn(move || {
        if name.starts_with(['-', '/']) {
            let _ = proxy.send_event(Action::Message(
                "This profile name cannot be passed safely to Windows VPN.".into(),
            ));
            return;
        }
        let mut command = Command::new(system_exe("rasdial.exe"));
        command.creation_flags(0x08000000).arg(&name);
        if !connect {
            command.arg("/disconnect");
        }
        let message=match command.output(){Ok(output) if output.status.success()=>"Windows VPN command completed. Checking connection status…".into(),Ok(output)=>format!("Windows VPN returned error {}. Configure the server and saved credentials in Windows VPN settings.",output.status.code().unwrap_or(-1)),Err(e)=>e.to_string()};
        let _ = proxy.send_event(Action::Message(message));
        vpn_refresh(proxy);
    });
}
pub fn vpn_settings() -> Result<()> {
    // A fixed OS settings URI; no shell or user-controlled command text.
    unsafe {
        let result = windows::Win32::UI::Shell::ShellExecuteW(
            None,
            w!("open"),
            w!("ms-settings:network-vpn"),
            None,
            None,
            windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL,
        );
        anyhow::ensure!(
            (result.0 as isize) > 32,
            "Windows VPN settings could not be opened"
        );
    }
    Ok(())
}

pub fn pick_import_folder() -> Result<Option<PathBuf>> {
    pick_import(true)
}
pub fn pick_import_file() -> Result<Option<PathBuf>> {
    pick_import(false)
}
fn pick_import(folder: bool) -> Result<Option<PathBuf>> {
    unsafe {
        let dialog: IFileOpenDialog =
            CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER)?;
        let mut options = FOS_FORCEFILESYSTEM | windows::Win32::UI::Shell::FOS_FILEMUSTEXIST;
        if folder {
            options |= FOS_PICKFOLDERS;
        }
        dialog.SetOptions(options)?;
        dialog.SetTitle(w!("Choose browser data to preview"))?;
        if let Err(e) = dialog.Show(None) {
            if e.code().0 as u32 == 0x800704c7 {
                return Ok(None);
            }
            return Err(e.into());
        }
        Ok(Some(PathBuf::from(webview2_com::take_pwstr(
            dialog.GetResult()?.GetDisplayName(SIGDN_FILESYSPATH)?,
        ))))
    }
}
