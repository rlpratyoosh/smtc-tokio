use windows::Media;
use std::sync::Mutex;

pub enum WindowsMediaEvent {
    Play,
    Pause,
    Next,
    Previous,
    Stop,
    Record,
    FastForward,
    Rewind,
    ChannelUp,
    ChannelDown,
}

pub struct WindowsMediaManager {
    event_rx: Option<Mutex<WindowsMediaEvent>>,
}

#[cfg(test)]
mod tests {
    use super::*;

}