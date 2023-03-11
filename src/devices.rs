use inotify::{Inotify, WatchMask};
use log::error;
use regex::Regex;
use std::{
    collections::{HashSet, VecDeque},
    io,
    iter::FromIterator,
    path::PathBuf,
    sync::{Arc, Mutex},
};

pub struct Devices {
    // enable inotify for new devices
    watch: bool,
    // watch only for events on files that were specified from command line
    watch_initial_only: bool,
    pub devices: HashSet<PathBuf>,
    root_dir: PathBuf,
    inotify: Inotify,
    events: VecDeque<PathBuf>,
}

pub type DevicesArc = Arc<Mutex<Devices>>;

lazy_static::lazy_static! {
    static ref DEVICES_RE: Regex = Regex::new(r"event\d+").unwrap();
}

impl Devices {
    pub fn new(watch: bool, devices: Vec<PathBuf>) -> Result<DevicesArc, io::Error> {
        let root_dir = PathBuf::from("/dev/input");
        let mut inotify = Inotify::init()?;
        if watch {
            if let Err(err) = inotify.add_watch(&root_dir, WatchMask::CREATE) {
                error!("failed to add watch: {err}");
                return Err(err);
            }
        }
        let (devices_set, watch_initial_only) = if devices.is_empty() {
            (Devices::get_all_input_devices()?, false)
        } else {
            (HashSet::from_iter(devices), true)
        };
        Ok(Arc::new(Mutex::new(Self {
            devices: devices_set,
            watch_initial_only: watch_initial_only,
            watch: watch,
            inotify: inotify,
            root_dir: root_dir,
            events: VecDeque::new(),
        })))
    }

    pub fn get_all_input_devices() -> io::Result<HashSet<PathBuf>> {
        let mut result = HashSet::new();
        for entry in std::fs::read_dir("/dev/input/")? {
            let entry = entry?;
            let path = entry.path();
            if DEVICES_RE.is_match(
                path.file_name()
                    .unwrap_or_default()
                    .to_str()
                    .unwrap_or_default(),
            ) {
                result.insert(path);
            }
        }
        Ok(result)
    }

    pub fn watch(&mut self) -> io::Result<PathBuf> {
        if !self.watch {
            panic!("watch() called but not enabled")
        }
        if let Some(event) = self.events.pop_back() {
            return Ok(event);
        }
        loop {
            let mut buf = [0; 1024];
            let events = self.inotify.read_events_blocking(&mut buf)?;
            for event in events {
                let file_name = event.name.unwrap_or_default().to_str().unwrap_or_default();
                if file_name.is_empty() {
                    continue;
                }
                let mut device_path = self.root_dir.clone();
                device_path.push(file_name);
                if self.watch_initial_only && !self.devices.contains(&device_path) {
                    continue;
                }
                if DEVICES_RE.is_match(file_name) {
                    self.events.push_front(device_path);
                }
            }
            if let Some(event) = self.events.pop_back() {
                return Ok(event);
            }
        }
    }
}
