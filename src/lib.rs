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
    _media_player: Media::Playback::MediaPlayer,
    event_rx: Mutex<Option<mpsc::UnboundedReceiver<WindowsMediaEvent>>>,
}

impl WindowsMediaManager {
    pub fn new() -> Self {
        let (event_tx, event_rx) = mpsc::unbounded_channel::<WindowsMediaEvent>();

        unsafe {
            let _ = Win32::System::WinRT::RoInitialize(Win32::System::WinRT::RO_INIT_TYPE(1));
        }

        let media_player = Media::Playback::MediaPlayer::new().expect("Failed to create media player");
        media_player.CommandManager().expect("Failed to get command_manager").SetIsEnabled(false).expect("Failed to set command manager false");
        let smtc = media_player.SystemMediaTransportControls().expect("Failed to get smtc");

        let _ = smtc.SetIsEnabled(true);

        let _ = smtc.SetIsPlayEnabled(true);
        let _ = smtc.SetIsPauseEnabled(true);
        let _ = smtc.SetIsStopEnabled(true);
        let _ = smtc.SetIsNextEnabled(true);
        let _ = smtc.SetIsPreviousEnabled(true);

        let _ = smtc.ButtonPressed(&TypedEventHandler::<Media::SystemMediaTransportControls, Media::SystemMediaTransportControlsButtonPressedEventArgs>::new(
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
        ));

        Self {
            _media_player: media_player,
            event_rx: Mutex::new(Some(event_rx)),
        }
    }

    pub fn take_event_rx(&self) -> Option<mpsc::UnboundedReceiver<WindowsMediaEvent>> {
        self.event_rx.lock().ok()?.take()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
}