use windows::{Foundation::TypedEventHandler, Media, Win32};
use std::{sync::Mutex};
use tokio::sync::mpsc;

pub enum WindowsMediaEvent {
    Play,
    Pause,
    Next,
    Previous,
    Stop,
}

pub struct WindowsMediaManager {
    sta_thread: Option<std::thread::JoinHandle<()>>,
    sta_thread_id: u32,
    event_rx: Mutex<Option<mpsc::UnboundedReceiver<WindowsMediaEvent>>>,
}

struct ComGuard;

impl Drop for ComGuard {
    fn drop(&mut self) {
        unsafe {
            Win32::System::WinRT::RoUninitialize();
        }
    }
}

impl Drop for WindowsMediaManager {
    fn drop(&mut self) {
        unsafe {
            let _ = Win32::UI::WindowsAndMessaging::PostThreadMessageW(
                self.sta_thread_id,
                Win32::UI::WindowsAndMessaging::WM_QUIT,
                windows::Win32::Foundation::WPARAM(0),
                windows::Win32::Foundation::LPARAM(0),
            );
        }
        if let Some(handle) = self.sta_thread.take() {
            let _ = handle.join();
        }
    }
}

impl WindowsMediaManager {
    pub fn new() -> Result<Self, String> {
        let (event_tx, event_rx) = mpsc::unbounded_channel::<WindowsMediaEvent>();
        let (id_tx, id_rx) = std::sync::mpsc::channel::<Result<u32, String>>();

        let sta_thread = std::thread::spawn(move || {
            let Ok(()) = (unsafe { Win32::System::WinRT::RoInitialize(Win32::System::WinRT::RO_INIT_SINGLETHREADED) }) else {
                let _ = id_tx.send(Err("Failed to initialize STA Com".to_string()));
                return;
            };
            let _guard = ComGuard;

            let Ok(media_player) = Media::Playback::MediaPlayer::new() else {
                let _ = id_tx.send(Err("Failed to initialize MediaPlayer".to_string()));
                return;
            };

            let Ok(command_manager) = media_player.CommandManager() else {
                let _ = id_tx.send(Err("Failed to get CommandManager".to_string()));
                return;
            };

            let Ok(()) = command_manager.SetIsEnabled(false) else {
                let _ = id_tx.send(Err("Failed to disable CommandManager".to_string()));
                return;
            };

            let Ok(smtc) = media_player.SystemMediaTransportControls() else {
                let _ = id_tx.send(Err("Failed to get SMTC".to_string()));
                return;
            };

            let Ok(()) = smtc.SetIsEnabled(true) else {
                let _ = id_tx.send(Err("Failed to enable SMTC".to_string()));
                return;
            };

            let Ok(()) = smtc.SetIsPlayEnabled(true) else {
                let _ = id_tx.send(Err("Failed to enable Play in SMTC".to_string()));
                return;
            };
            let Ok(()) = smtc.SetIsPauseEnabled(true) else {
                let _ = id_tx.send(Err("Failed to enable Pause in SMTC".to_string()));
                return;
            };
            let Ok(()) = smtc.SetIsStopEnabled(true) else {
                let _ = id_tx.send(Err("Failed to enable Stop in SMTC".to_string()));
                return;
            };
            let Ok(()) = smtc.SetIsNextEnabled(true) else {
                let _ = id_tx.send(Err("Failed to enable Next in SMTC".to_string()));
                return;
            };
            let Ok(()) = smtc.SetIsPreviousEnabled(true) else {
                let _ = id_tx.send(Err("Failed to enable Previous in SMTC".to_string()));
                return;
            };

            let Ok(_) = smtc.ButtonPressed(&TypedEventHandler::<Media::SystemMediaTransportControls, Media::SystemMediaTransportControlsButtonPressedEventArgs>::new(
                move |_, args| {
                    if let Some(args) = args.as_ref() {
                        match args.Button()? {
                            Media::SystemMediaTransportControlsButton::Play => {
                                let _ = event_tx.send(WindowsMediaEvent::Play);
                            },
                            Media::SystemMediaTransportControlsButton::Pause => {
                                let _ = event_tx.send(WindowsMediaEvent::Pause);
                            },
                            Media::SystemMediaTransportControlsButton::Next => {
                                let _ = event_tx.send(WindowsMediaEvent::Next);
                            },
                            Media::SystemMediaTransportControlsButton::Previous => {
                                let _ = event_tx.send(WindowsMediaEvent::Previous);
                            },
                            Media::SystemMediaTransportControlsButton::Stop => {
                                let _ = event_tx.send(WindowsMediaEvent::Stop);
                            },
                            _ => {}
                        }
                    }
                    Ok(())
                }
            )) else {
                let _ = id_tx.send(Err("Failed to setup handles for events in SMTC".to_string()));
                return;
            };

            unsafe {
                let mut msg = Win32::UI::WindowsAndMessaging::MSG::default();
                let _ = Win32::UI::WindowsAndMessaging::PeekMessageW( &mut msg, None, 0, 0, Win32::UI::WindowsAndMessaging::PM_NOREMOVE,);
            }

            let _ = id_tx.send(Ok( unsafe { Win32::System::Threading::GetCurrentThreadId() }));

            unsafe {
                let mut msg = Win32::UI::WindowsAndMessaging::MSG::default();
                while Win32::UI::WindowsAndMessaging::GetMessageW(&mut msg, None, 0, 0).as_bool() {
                    let _ = Win32::UI::WindowsAndMessaging::TranslateMessage(&msg);
                    Win32::UI::WindowsAndMessaging::DispatchMessageW(&msg);
                }
            }

            unsafe { Win32::System::WinRT::RoUninitialize(); }
        });

        let sta_thread_id = match id_rx.recv() {
            Ok(msg) => {
                match msg {
                    Ok(id) => id,
                    Err(e) => return Err(e),
                }
            },
            Err(_) => return Err("Failed to recieve Thread Id".to_string())
        };

        Ok(Self {
            sta_thread: Some(sta_thread),
            event_rx: Mutex::new(Some(event_rx)),
            sta_thread_id,
        })
    }

    pub fn take_event_rx(&self) -> Option<mpsc::UnboundedReceiver<WindowsMediaEvent>> {
        self.event_rx.lock().ok()?.take()
    }
}

#[cfg(test)]
mod tests {

}