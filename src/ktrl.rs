use evdev_rs::enums::EventType;
use log::{error, info};

use std::convert::TryFrom;
use std::fs::read_to_string;
use std::path::PathBuf;

use std::sync::Arc;
use std::sync::Mutex;

use crate::actions::TapDanceMgr;
use crate::actions::TapHoldMgr;
use crate::actions::TapModMgr;
use crate::cfg;
use crate::effects::key_event_to_fx_val;
use crate::effects::perform_effect;
use crate::effects::StickyState;
use crate::keys::KeyEvent;
use crate::layers::LayersManager;
use crate::KbdIn;
use crate::KbdOut;

#[cfg(feature = "sound")]
use crate::effects::Dj;

pub struct KtrlArgs {
    pub kbd_path: Vec<PathBuf>,
    pub config_path: PathBuf,
    pub assets_path: PathBuf,
    pub ipc_port: usize,
    pub ipc_msg: Option<String>,
    pub notify_port: usize,
}

pub struct Ktrl {
    pub kbd_in_path: Vec<PathBuf>,
    pub kbd_out: KbdOut,
    pub l_mgr: LayersManager,
    pub th_mgr: TapHoldMgr,
    pub td_mgr: TapDanceMgr,
    pub tm_mgr: TapModMgr,
    pub sticky: StickyState,

    #[cfg(feature = "sound")]
    pub dj: Dj,
}

impl Ktrl {
    pub fn new(args: KtrlArgs) -> Result<Self, std::io::Error> {
        let kbd_out = match KbdOut::new() {
            Ok(kbd_out) => kbd_out,
            Err(err) => {
                error!("Failed to open the output uinput device. Make sure you've added ktrl to the `uinput` group");
                return Err(err);
            }
        };

        let cfg_str = read_to_string(args.config_path)?;
        let cfg = cfg::parse(&cfg_str);
        let mut l_mgr = LayersManager::new(
            &cfg.layers,
            &cfg.layer_aliases,
            &cfg.layer_profiles,
            #[cfg(feature = "notify")]
            args.notify_port,
        )?;
        l_mgr.init();

        let th_mgr = TapHoldMgr::new(cfg.tap_hold_wait_time);
        let td_mgr = TapDanceMgr::new(cfg.tap_dance_wait_time);
        let tm_mgr = TapModMgr::new();
        let sticky = StickyState::new();

        #[cfg(feature = "sound")]
        let dj = Dj::new(&args.assets_path);

        Ok(Self {
            kbd_in_path: args.kbd_path,
            kbd_out,
            l_mgr,
            th_mgr,
            td_mgr,
            tm_mgr,
            sticky,
            #[cfg(feature = "sound")]
            dj,
        })
    }

    pub fn new_arc(args: KtrlArgs) -> Result<Arc<Mutex<Self>>, std::io::Error> {
        Ok(Arc::new(Mutex::new(Self::new(args)?)))
    }

    //
    // TODO:
    // ----
    // Refactor this to unicast if special key,
    // and broadcast if regular tap key.
    //
    fn handle_key_event(&mut self, event: &KeyEvent) -> Result<(), std::io::Error> {
        // Handle TapHold action keys
        let th_out = self.th_mgr.process(&mut self.l_mgr, event);
        if let Some(th_fx_vals) = th_out.effects {
            for fx_val in th_fx_vals {
                perform_effect(self, fx_val)?
            }
        }

        if th_out.stop_processing {
            return Ok(());
        }

        let td_out = self.td_mgr.process(&mut self.l_mgr, event);
        if let Some(td_fx_vals) = td_out.effects {
            for fx_val in td_fx_vals {
                perform_effect(self, fx_val)?
            }
        }

        if td_out.stop_processing {
            return Ok(());
        }

        let te_out = self.tm_mgr.process(&self.l_mgr, event);
        if let Some(te_fx_vals) = te_out.effects {
            for fx_val in te_fx_vals {
                perform_effect(self, fx_val)?
            }
        }

        if !te_out.stop_processing {
            let leftover_fx_val = key_event_to_fx_val(&self.l_mgr, event);
            perform_effect(self, leftover_fx_val)?;
        }

        Ok(())
    }

    pub fn event_loop(ktrl: Arc<Mutex<Self>>) -> Result<(), std::io::Error> {
        info!("Ktrl: Entering the event loop");

        let kbd_in_paths: Vec<PathBuf>;
        {
            let ktrl = ktrl.lock().expect("Failed to lock ktrl (poisoned)");
            kbd_in_paths = ktrl.kbd_in_path.clone();
        }

        let threads = kbd_in_paths.iter().map(|kbd_in_path| {
            let ktrl = ktrl.clone();
            let kbd_in_path = kbd_in_path.clone();
            std::thread::spawn(move || {
                Self::event_loop_for_path(ktrl, kbd_in_path)
            })
        }).collect::<Vec<_>>();

        let mut res = Ok(());
        for thread in threads {
            if let Err(err) = thread.join().unwrap() {
                res = Err(err);
            }
        }

        res
    }

    fn event_loop_for_path(ktrl: Arc<Mutex<Self>>, kbd_path: PathBuf) -> Result<(), std::io::Error> {
        let kbd_in = match KbdIn::new(&kbd_path) {
            Ok(kbd_in) => kbd_in,
            Err(err) => {
                error!("Failed to open the input keyboard device: {err}. Make sure you've added ktrl to the `input` group");
                return Err(err);
            }
        };
        loop {
            let in_event = kbd_in.read()?;
            // TODO maybe use channel instead of locking for each event?
            let mut ktrl = ktrl.lock().expect("Failed to lock ktrl (poisoned)");
            if !(
                in_event.event_type == EventType::EV_SYN
                || in_event.event_type == EventType::EV_MSC
                || in_event.event_type == EventType::EV_REL
            ) {
                log::debug!("event {:?}", in_event);
            }

            // Pass-through non-key events
            let key_event = match KeyEvent::try_from(in_event.clone()) {
                Ok(ev) => ev,
                _ => {
                    ktrl.kbd_out.write(in_event)?;
                    continue;
                }
            };

            ktrl.handle_key_event(&key_event)?;
        }
    }
}
