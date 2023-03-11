use log::error;
// evdev-rs
use evdev_rs::enums::EventType;
use evdev_rs::Device;
use evdev_rs::GrabMode;
use evdev_rs::InputEvent;
use evdev_rs::ReadFlag;
use evdev_rs::ReadStatus;
use retry::delay;
use retry::retry;
use retry::OperationResult;

use std::fs::File;
use std::path::Path;
use std::path::PathBuf;

pub struct KbdIn {
    device: Device,
    path: PathBuf,
}

impl KbdIn {
    pub fn new(dev_path: &Path) -> Result<Self, std::io::Error> {
        // retry is needed because sometimes inotify event comes earlier than
        // udev rules are applied, which leads to permission denied error
        match retry(
            delay::Exponential::from_millis(3).take(10),
            || match Self::new_inner(&dev_path) {
                Ok(kbd_in) => OperationResult::Ok(kbd_in),
                Err(err) => match err.kind() {
                    std::io::ErrorKind::PermissionDenied => OperationResult::Retry(err),
                    _ => OperationResult::Err(err),
                },
            },
        ) {
            Ok(kbd_in) => Ok(kbd_in),
            Err(err) => {
                error!("Failed to open the input keyboard device: {err}. Make sure you've added ktrl to the `input` group");
                return Err(err.error);
            }
        }
    }

    fn new_inner(dev_path: &Path) -> Result<Self, std::io::Error> {
        let kbd_in_file = File::open(dev_path)?;
        let mut kbd_in_dev = Device::new_from_fd(kbd_in_file)?;
        if kbd_in_dev.has(&EventType::EV_ABS) {
            // this blocks all hotkeys, including ctrl-c
            log::error!("Skip device {dev_path:?}: touchapd is not supporded");
            return Err(std::io::Error::new(std::io::ErrorKind::Other, "touchpad"));
        }
        if kbd_in_dev.name().unwrap_or_default() == "ktrl" {
            log::error!(
                "device {path} is our own output device: {name:?}",
                path = dev_path.display(),
                name = kbd_in_dev.name()
            );
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "ktrl output device",
            ));
        }

        // NOTE: This grab-ungrab-grab sequence magically
        // fix an issue I had with my Lenovo Yoga trackpad not working.
        // I honestly have no idea why this works haha.
        kbd_in_dev.grab(GrabMode::Grab)?;
        kbd_in_dev.grab(GrabMode::Ungrab)?;
        kbd_in_dev.grab(GrabMode::Grab)?;

        Ok(KbdIn {
            device: kbd_in_dev,
            path: dev_path.to_path_buf(),
        })
    }

    pub fn read(&self) -> Result<InputEvent, std::io::Error> {
        let (status, event) = self
            .device
            .next_event(ReadFlag::NORMAL | ReadFlag::BLOCKING)?;
        if status == ReadStatus::Success {
            Ok(event)
        } else {
            Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("read {path}: bad status", path = self.path.display()),
            ))
        }
    }
}
