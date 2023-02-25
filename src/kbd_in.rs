// evdev-rs
use evdev_rs::Device;
use evdev_rs::GrabMode;
use evdev_rs::InputEvent;
use evdev_rs::ReadFlag;
use evdev_rs::ReadStatus;
use evdev_rs::enums::EventType;

use std::fs::File;
use std::path::Path;

pub struct KbdIn {
    device: Device,
}

impl KbdIn {
    pub fn new(dev_path: &Path) -> Result<Self, std::io::Error> {
        let kbd_in_file = File::open(dev_path)?;
        let mut kbd_in_dev = Device::new_from_fd(kbd_in_file)?;
        if kbd_in_dev.has(&EventType::EV_ABS) {
            // this blocks all hotkeys, including ctrl-c
            log::error!("Skip device {dev_path:?}: touchapd is not supporded");
            return Err(std::io::Error::new(std::io::ErrorKind::Other, "touchpad"));
        }

        // NOTE: This grab-ungrab-grab sequence magically
        // fix an issue I had with my Lenovo Yoga trackpad not working.
        // I honestly have no idea why this works haha.
        kbd_in_dev.grab(GrabMode::Grab)?;
        kbd_in_dev.grab(GrabMode::Ungrab)?;
        kbd_in_dev.grab(GrabMode::Grab)?;

        Ok(KbdIn { device: kbd_in_dev })
    }

    pub fn read(&self) -> Result<InputEvent, std::io::Error> {
        let (status, event) = self
            .device
            .next_event(ReadFlag::NORMAL | ReadFlag::BLOCKING)?;
        std::assert!(status == ReadStatus::Success);
        Ok(event)
    }
}
