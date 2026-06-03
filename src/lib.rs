use windows::{Media, Win32};
use std::{sync::Mutex, thread};
use tokio::sync::mpsc;
use windows_strings::PCWSTR;

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
    event_rx: Mutex<Option<mpsc::UnboundedReceiver<WindowsMediaEvent>>>,
}

unsafe extern "system" fn wndproc(hwnd: Win32::Foundation::HWND, msg: u32, wparam: Win32::Foundation::WPARAM, lparam: Win32::Foundation::LPARAM) -> Win32::Foundation::LRESULT {
    unsafe { Win32::UI::WindowsAndMessaging::DefWindowProcA(hwnd, msg, wparam, lparam) }
}

impl WindowsMediaManager {
    pub fn new(class_name: String) -> Self {
        let (event_tx, mut event_rx) = mpsc::unbounded_channel::<WindowsMediaEvent>();
        let class_name = format!("{}-smtc", class_name);
        let mut class_name: Vec<u16> = class_name.encode_utf16().collect();
        class_name.push(0);

        thread::spawn(move || {
            unsafe {
                // Initialize COM Apartment
                let _ = Win32::System::WinRT::RoInitialize(Win32::System::WinRT::RO_INIT_TYPE(1));

                // Create ghost window blueprint
                let window = Win32::UI::WindowsAndMessaging::WNDCLASSW {
                    style: Win32::UI::WindowsAndMessaging::WNDCLASS_STYLES(0),
                    lpfnWndProc: Some(wndproc),
                    cbClsExtra: 0,
                    cbWndExtra: 0,
                    hInstance: Win32::Foundation::HINSTANCE::from(Win32::System::LibraryLoader::GetModuleHandleW(None).expect("Failed to create hInstance")),
                    hIcon: Default::default(),
                    hCursor: Default::default(),
                    hbrBackground: Default::default(),
                    lpszMenuName: Default::default(),
                    lpszClassName: PCWSTR::from_raw(class_name.as_ptr()),
                };
            }
        });

        let event_rx = Mutex::new(Some(event_rx));
        Self {
            event_rx,
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