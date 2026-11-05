<center>

# smtc-tokio

[![Version](https://img.shields.io/badge/version-0.1.1-blue.svg)](https://github.com/rlpratyoosh/smtc-tokio)
[![Language](https://img.shields.io/badge/language-Rust-orange.svg)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/license-MIT-green.svg)](LICENSE)

</center>

A simple, thread-safe library that connects your async Rust application (`tokio`) to the Windows System Media Transport Controls (SMTC).

This crate allows your application to:
1. Receive physical media key presses (Play, Pause, Next, Previous) from the user's keyboard or headset.
2. Update the Windows volume flyout with the currently playing song's title, artist, album, and album art.
3. Show and update the timeline progress bar in the Windows UI.

It handles all the complex Windows background thread management for you, communicating safely with your Tokio application using standard `mpsc` channels.

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
smtc-tokio = "0.1.1"
tokio = { version = "1", features = ["full"] }
```

## Quick Start

Here is a basic example of how to connect to Windows and listen for media keys.

```rust
use smtc_tokio::{WindowsMediaManager, WindowsMediaEvent};

#[tokio::main]
async fn main() {
    // Connect to Windows
    let manager = WindowsMediaManager::new().expect("Failed to hook into Windows OS");
    
    // Grab the receiver channel to listen for key presses
    let mut rx = manager.take_event_rx().expect("Failed to get receiver");

    // Tell Windows what song is playing
    manager.set_metadata(
        "Never Gonna Give You Up", 
        &["Rick Astley".to_string()], 
        "Whenever You Need Somebody", 
        213000, // Total duration in milliseconds
        None,
    );
    
    // Set the state to playing so Windows routes the media keys to us
    manager.set_playback_status(true);

    // Listen for hardware media keys in your async loop
    while let Some(event) = rx.recv().await {
        match event {
            WindowsMediaEvent::Play => println!("User pressed play!"),
            WindowsMediaEvent::Pause => println!("User pressed pause!"),
            WindowsMediaEvent::Next => println!("User pressed next!"),
            WindowsMediaEvent::Previous => println!("User pressed previous!"),
            WindowsMediaEvent::SetPosition(pos_ms) => println!("User scrubbed to {}ms", pos_ms),
            _ => println!("Other media key pressed"),
        }
    }
}

```

## API Reference

### Updating the UI

You can update the Windows flyout at any time using the `manager`:

* `manager.set_metadata(title, artists, album, duration, art_url)`
* `manager.set_playback_status(is_playing)`
* `manager.set_position(position_ms)`
* `manager.set_stopped()`

## Contributing

Contributions are welcome, but **please keep them strictly limited to SMTC-related features.** This project is designed to do one thing and do it well: act as a bridge between Tokio and the Windows Media Controls. Please do not submit pull requests that add unrelated audio playback features or UI frameworks.

* **Found a bug?** Please open an Issue first, followed by a Pull Request to fix it.
* **Want to add a missing SMTC feature?** (e.g., Shuffle/Repeat toggles). Feel free to open an Issue to discuss it and submit a PR.