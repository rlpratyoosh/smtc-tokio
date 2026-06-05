use windows::{Foundation::{TypedEventHandler, Uri}, Media, Storage::Streams::RandomAccessStreamReference, Win32};
use windows_strings::HSTRING;
use std::{sync::Mutex};
use tokio::sync::mpsc;

pub enum WindowsMediaEvent {
    Play,
    Pause,
    Next,
    Previous,
    Stop,
}

pub enum WindowsMediaCommand {
    SetMetadata {
        title: String,
        artists: Vec<String>,
        album: String,
        duration: u64,
        art_url: Option<String>,
    },
    SetPlaybackStatus(bool), // true for playing false for paused
    SetPosition(u64, u64), // Song duration, Position (in millis)
    SetStopped,
}

pub struct WindowsMediaManager {
    sta_thread: Option<std::thread::JoinHandle<()>>,
    sta_thread_id: u32,
    event_rx: Mutex<Option<mpsc::UnboundedReceiver<WindowsMediaEvent>>>,
    command_tx: std::sync::mpsc::Sender<WindowsMediaCommand>,
}

const WM_USER_WAKEUP: u32 = Win32::UI::WindowsAndMessaging::WM_APP + 1;

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
        let (command_tx, command_rx) = std::sync::mpsc::channel::<WindowsMediaCommand>();
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
                    if msg.message == WM_USER_WAKEUP {
                        while let Ok(cmd) = command_rx.try_recv() {
                            match cmd {
                                WindowsMediaCommand::SetMetadata { title, artists, album, duration, art_url } => {
                                    if let Ok(smtc_display_updater) = smtc.DisplayUpdater() {
                                        let _ = smtc_display_updater.SetType(Media::MediaPlaybackType::Music);
                                        if let Ok(music_properties) = smtc_display_updater.MusicProperties() {
                                            let artists = artists.join(", ");
                                            let _ = music_properties.SetArtist(&HSTRING::from(artists));
                                            let _ = music_properties.SetAlbumTitle(&HSTRING::from(album));
                                            let _ = music_properties.SetTitle(&HSTRING::from(title));
                                        }
                                        if let Some(art_url) = art_url {
                                            if let Ok(uri) = Uri::CreateUri(&HSTRING::from(art_url)){
                                                if let Ok(random_access_stream_ref) = RandomAccessStreamReference::CreateFromUri(&uri) {
                                                    let _ = smtc_display_updater.SetThumbnail(&random_access_stream_ref);
                                                }
                                            }
                                        }
                                        let _ = smtc_display_updater.Update();
                                    }
                                    if let Ok(smtc_timeline_properties) = WindowsMediaManager::set_smtc_timeline_properties(duration, 0)  {
                                        let _ = smtc.UpdateTimelineProperties(&smtc_timeline_properties);
                                    }
                                },
                                WindowsMediaCommand::SetPlaybackStatus(playing) => {
                                    let _ = if playing { smtc.SetPlaybackStatus(Media::MediaPlaybackStatus::Playing) } else {
                                        smtc.SetPlaybackStatus(Media::MediaPlaybackStatus::Paused)
                                    };
                                },
                                WindowsMediaCommand::SetPosition(dur, pos) => {
                                    if let Ok(smtc_timeline_properties) = WindowsMediaManager::set_smtc_timeline_properties(dur, pos)  {
                                        let _ = smtc.UpdateTimelineProperties(&smtc_timeline_properties);
                                    }
                                },
                                WindowsMediaCommand::SetStopped => {
                                    let _ = smtc.SetPlaybackStatus(Media::MediaPlaybackStatus::Stopped);
                                }
                            }
                        }
                    }
                    let _ = Win32::UI::WindowsAndMessaging::TranslateMessage(&msg);
                    Win32::UI::WindowsAndMessaging::DispatchMessageW(&msg);
                }
            }
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
            command_tx
        })
    }

    fn set_smtc_timeline_properties(dur: u64, pos: u64) -> Result<Media::SystemMediaTransportControlsTimelineProperties, windows::core::Error> {
        let smtc_timeline_properties = Media::SystemMediaTransportControlsTimelineProperties::new()?;
        let dur = (dur as i64) * 10000; // In 100 nano seconds
        let pos = (pos as i64) * 10000;
        let _ = smtc_timeline_properties.SetStartTime(windows::Foundation::TimeSpan { Duration: 0 });
        let _ = smtc_timeline_properties.SetMinSeekTime(windows::Foundation::TimeSpan { Duration: 0 });
        let _ = smtc_timeline_properties.SetPosition(windows::Foundation::TimeSpan { Duration: pos });
        let _ = smtc_timeline_properties.SetMaxSeekTime(windows::Foundation::TimeSpan { Duration: dur });
        let _ = smtc_timeline_properties.SetEndTime(windows::Foundation::TimeSpan { Duration: dur });
        Ok(smtc_timeline_properties)
    }

    pub fn take_event_rx(&self) -> Option<mpsc::UnboundedReceiver<WindowsMediaEvent>> {
        self.event_rx.lock().ok()?.take()
    }

    pub fn set_metadata( &self, title: &str, artists: &[String], album: &str, duration: u64, art_url: Option<String>) {
        let _ = self.command_tx.send(WindowsMediaCommand::SetMetadata {
            title: title.to_string(),
            artists: artists.to_vec(),
            album: album.to_string(),
            duration,
            art_url
        });
        self.wake_sta_thread();
    }

    pub fn set_playback_status(&self, is_playing: bool) {
        let _ = self.command_tx.send(WindowsMediaCommand::SetPlaybackStatus(is_playing));
        self.wake_sta_thread();
    }

    pub fn set_position(&self, duration_ms: u64, position_ms: u64) {
        let _ = self.command_tx.send(WindowsMediaCommand::SetPosition(duration_ms, position_ms));
        self.wake_sta_thread();
    }

    pub fn set_stopped(&self) {
        let _ = self.command_tx.send(WindowsMediaCommand::SetStopped);
        self.wake_sta_thread();
    }

    fn wake_sta_thread(&self) {
        unsafe {
            let _ = Win32::UI::WindowsAndMessaging::PostThreadMessageW(
                self.sta_thread_id,
                WM_USER_WAKEUP,
                Win32::Foundation::WPARAM(0),
                Win32::Foundation::LPARAM(0),
            );
        }
    }

}

#[cfg(test)]
mod tests {

}