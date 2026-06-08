//! This library connects your async Rust application (Tokio) to the Windows System Media Transport Controls (SMTC).
//! It lets you listen for physical media key presses (Play, Pause, Next, Previous) and update the Windows volume flyout
//! with your currently playing song's info, album art, and progress bar.
//! 
//! # Example
//! 
//! ```rust,no_run
//! use tokio;
//! use smtc_tokio::{WindowsMediaManager, WindowsMediaEvent};
//! 
//! #[tokio::main]
//! async fn main() {
//!     // Connect to Windows
//!     let manager = WindowsMediaManager::new().expect("Failed to hook into Windows OS");
//!     let mut rx = manager.take_event_rx().expect("Failed to get receiver");
//! 
//!     // Tell Windows we are playing a song so it routes keys to us
//!     manager.set_metadata(
//!         "Never Gonna Give You Up", 
//!         &["Rick Astley".to_string()], 
//!         "Whenever You Need Somebody", 
//!         213000, 
//!         Some("https://i.insider.com/602ee9ced3ad27001837f2ac?width=1000&format=jpeg&auto=webp".to_string()),
//!     );
//!     manager.set_playback_status(true);
//! 
//!     // Listen for hardware media keys
//!     while let Some(event) = rx.recv().await {
//!         match event {
//!             WindowsMediaEvent::Play => println!("User pressed play!"),
//!             WindowsMediaEvent::Pause => println!("User pressed pause!"),
//!             _ => println!("Other media key pressed"),
//!         }
//!     }
//! }
//! ```

use windows::{Foundation::{TypedEventHandler, Uri}, Media, Storage::Streams::RandomAccessStreamReference, Win32};
use windows_strings::HSTRING;
use std::{sync::Mutex};
use tokio::sync::mpsc;

/// Events that come from Windows when the user presses media keys.
pub enum WindowsMediaEvent {
    Play,
    Pause,
    Next,
    Previous,
    Stop,
    /// Fired when the user clicks or drags the timeline slider. 
    /// Returns the requested position in milliseconds.
    SetPosition(u64),
}

/// Commands we send from our app back to Windows to update the media popup.
pub enum WindowsMediaCommand {
    /// Updates the song details (title, artist, album, duration, and optional album art).
    SetMetadata {
        title: String,
        artists: Vec<String>,
        album: String,
        duration: u64,
        art_url: Option<String>,
    },
    /// Tells Windows if the music is currently playing (`true`) or paused (`false`).
    SetPlaybackStatus(bool),
    /// Updates the progress bar. Takes the current position in milliseconds.
    SetPosition(u64),
    /// Tells Windows the music has completely stopped.
    SetStopped,
}

/// The main manager that keeps the Windows media connection alive in the background.
/// 
/// As long as this struct exists, your app will control the Windows media flyout. 
/// When it gets dropped, the connection is cleanly closed.
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
    /// Creates a new media manager and connects to Windows.
    /// 
    /// This starts a safe, hidden background thread to talk to the OS. 
    /// Returns an error if Windows refuses to initialize the media controls.
    /// 
    /// # Example
    /// ```rust,no_run
    /// use smtc_tokio::WindowsMediaManager;
    /// 
    /// let manager = WindowsMediaManager::new().expect("Failed to initialize Windows COM");
    /// ```
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

            let button_pressed_tx = event_tx.clone();
            let Ok(_) = smtc.ButtonPressed(&TypedEventHandler::<Media::SystemMediaTransportControls, Media::SystemMediaTransportControlsButtonPressedEventArgs>::new(
                move |_, args| {
                    if let Some(args) = args.as_ref() {
                        match args.Button()? {
                            Media::SystemMediaTransportControlsButton::Play => {
                                let _ = button_pressed_tx.send(WindowsMediaEvent::Play);
                            },
                            Media::SystemMediaTransportControlsButton::Pause => {
                                let _ = button_pressed_tx.send(WindowsMediaEvent::Pause);
                            },
                            Media::SystemMediaTransportControlsButton::Next => {
                                let _ = button_pressed_tx.send(WindowsMediaEvent::Next);
                            },
                            Media::SystemMediaTransportControlsButton::Previous => {
                                let _ = button_pressed_tx.send(WindowsMediaEvent::Previous);
                            },
                            Media::SystemMediaTransportControlsButton::Stop => {
                                let _ = button_pressed_tx.send(WindowsMediaEvent::Stop);
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

            let playback_position_tx = event_tx.clone();
            let _ = smtc.PlaybackPositionChangeRequested(&TypedEventHandler::<Media::SystemMediaTransportControls, Media::PlaybackPositionChangeRequestedEventArgs>::new(
                move |_, args| {
                    if let Some(args) = args.as_ref() {
                        let time = args.RequestedPlaybackPosition()?;
                        // We must register this PlaybackPositionChangeRequested listener.
                        // Even if we don't actively use the event, Windows requires this listener to be present
                        // to implicitly derive that our app supports seeking, which forces the OS to draw the timeline slider.
                        let _ = playback_position_tx.send(WindowsMediaEvent::SetPosition((time.Duration / 10000) as u64));
                    }
                    Ok(())
                }
            ));

            unsafe {
                let mut msg = Win32::UI::WindowsAndMessaging::MSG::default();
                let _ = Win32::UI::WindowsAndMessaging::PeekMessageW( &mut msg, None, 0, 0, Win32::UI::WindowsAndMessaging::PM_NOREMOVE,);
            }

            let _ = id_tx.send(Ok( unsafe { Win32::System::Threading::GetCurrentThreadId() }));

            let mut cur_dur: u64 = 0;
            unsafe {
                let mut msg = Win32::UI::WindowsAndMessaging::MSG::default();
                while Win32::UI::WindowsAndMessaging::GetMessageW(&mut msg, None, 0, 0).as_bool() {
                    if msg.message == WM_USER_WAKEUP {
                        while let Ok(cmd) = command_rx.try_recv() {
                            match cmd {
                                WindowsMediaCommand::SetMetadata { title, artists, album, duration, art_url } => {
                                    cur_dur = duration;
                                    if let Ok(smtc_timeline_properties) = WindowsMediaManager::set_smtc_timeline_properties(duration, 0)  {
                                        let _ = smtc.UpdateTimelineProperties(&smtc_timeline_properties);
                                    }
                                    if let Ok(smtc_display_updater) = smtc.DisplayUpdater() {
                                        let _ = smtc_display_updater.SetType(Media::MediaPlaybackType::Music);
                                        if let Ok(music_properties) = smtc_display_updater.MusicProperties() {
                                            let artists = artists.join(", ");
                                            let _ = music_properties.SetArtist(&HSTRING::from(artists));
                                            let _ = music_properties.SetAlbumTitle(&HSTRING::from(album));
                                            let _ = music_properties.SetTitle(&HSTRING::from(title));
                                        }
                                        if let Some(art_url) = art_url
                                            && let Ok(uri) = Uri::CreateUri(&HSTRING::from(art_url))
                                            && let Ok(random_access_stream_ref) = RandomAccessStreamReference::CreateFromUri(&uri) 
                                        {
                                            let _ = smtc_display_updater.SetThumbnail(&random_access_stream_ref);
                                        }
                                        let _ = smtc_display_updater.Update();
                                    }
                                },
                                WindowsMediaCommand::SetPlaybackStatus(playing) => {
                                    let _ = if playing { smtc.SetPlaybackStatus(Media::MediaPlaybackStatus::Playing) } else {
                                        smtc.SetPlaybackStatus(Media::MediaPlaybackStatus::Paused)
                                    };
                                },
                                WindowsMediaCommand::SetPosition(pos) => {
                                    if let Ok(smtc_timeline_properties) = WindowsMediaManager::set_smtc_timeline_properties(cur_dur, pos)  {
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
                msg?
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

    /// Grabs the receiver channel so you can listen for media key presses in your Tokio loop.
    /// 
    /// Note: You can only take this once. If you call it again, it returns `None`.
    /// 
    /// # Example
    /// ```rust,no_run
    /// use smtc_tokio::WindowsMediaManager;
    /// 
    /// let manager = WindowsMediaManager::new().unwrap();
    /// let mut rx = manager.take_event_rx().expect("I have the channel now");
    /// ```
    pub fn take_event_rx(&self) -> Option<mpsc::UnboundedReceiver<WindowsMediaEvent>> {
        self.event_rx.lock().ok()?.take()
    }

    /// Updates the song details (title, artist, album, and optional album art) shown in the Windows flyout.
    /// 
    /// # Example
    /// ```rust,no_run
    /// use smtc_tokio::WindowsMediaManager;
    /// 
    /// let manager = WindowsMediaManager::new().unwrap();
    /// manager.set_metadata(
    ///     "Attention", 
    ///     &["Charlie Puth".to_string()], 
    ///     "Voicenotes", 
    ///     208000, // Duration in milliseconds
    ///     None    // URL to an image file or web link
    /// );
    /// ```
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

    /// Tells Windows if the music is currently playing (`true`) or paused (`false`).
    /// 
    /// # Example
    /// ```rust,no_run
    /// use smtc_tokio::WindowsMediaManager;
    /// 
    /// let manager = WindowsMediaManager::new().unwrap();
    /// manager.set_playback_status(true); // User just pressed Play
    /// ```
    pub fn set_playback_status(&self, is_playing: bool) {
        let _ = self.command_tx.send(WindowsMediaCommand::SetPlaybackStatus(is_playing));
        self.wake_sta_thread();
    }

    /// Updates the progress bar in the Windows media popup.
    /// 
    /// # Example
    /// ```rust,no_run
    /// use smtc_tokio::WindowsMediaManager;
    /// 
    /// let manager = WindowsMediaManager::new().unwrap();
    /// // The song is 3 minutes long, and we are exactly 1 minute in
    /// manager.set_position(60_000); 
    /// ```
    pub fn set_position(&self, position_ms: u64) {
        let _ = self.command_tx.send(WindowsMediaCommand::SetPosition(position_ms));
        self.wake_sta_thread();
    }

    /// Tells Windows that playback has completely stopped, clearing the playing state.
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
    use super::*;

    #[tokio::test]
    async fn test_manager_lifecycle() {
        let manager = WindowsMediaManager::new().expect("Should initialize successfully on a Windows machine");

        let _rx = manager.take_event_rx().expect("Receiver should be available the first time");
        assert!(manager.take_event_rx().is_none());

        manager.set_metadata(
            "Never Gonna Give You Up",
            &["Rick Astley".to_string()],
            "Whenever You Need Somebody",
            213000, 
            None,
        );

        manager.set_playback_status(true);
    }
}