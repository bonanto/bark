use crate::{cfg::*, focus, quiet, update};
use std::sync::atomic::{AtomicU32, Ordering};
use windows::{
    core::{implement, Interface, Ref, Result, BOOL, GUID, PCWSTR},
    Win32::{
        Foundation::{LPARAM, PROPERTYKEY, WPARAM},
        Media::Audio::{
            Endpoints::{
                IAudioEndpointVolume, IAudioEndpointVolumeCallback, IAudioEndpointVolumeCallback_Impl,
            },
            *,
        },
        System::{
            Com::*,
            SystemInformation::{GetLocalTime, GetTickCount64},
            Threading::{GetCurrentProcess, GetCurrentThreadId, SetProcessWorkingSetSize},
        },
        UI::WindowsAndMessaging::*,
    },
};

pub const WM_VOL: u32 = WM_APP + 1; // wparam = user gesture, lparam = percent
pub const WM_RESCAN: u32 = WM_APP + 2;
pub const WM_REBIND: u32 = WM_APP + 3;
pub const WM_APPLY: u32 = WM_APP + 4;
pub const WM_EVAL: u32 = WM_APP + 5;
pub const WM_MASTER: u32 = WM_APP + 6; // main volume changed

pub static TID: AtomicU32 = AtomicU32::new(0);
// Event context for our own writes; lets us ignore the echo.
pub static CTX: GUID = GUID::from_u128(0x7C1E4B52_9A3D_4F68_B0E2_5D81A6C3F974);

pub fn post(msg: u32, w: usize, l: isize) {
    let t = TID.load(Ordering::Acquire);
    if t != 0 {
        unsafe {
            let _ = PostThreadMessageW(t, msg, WPARAM(w), LPARAM(l));
        }
    }
}

// ---- COM callbacks: only sample state and post to the engine thread ----

#[implement(IAudioSessionEvents)]
struct SessEv {
    _p: (),
}

#[allow(non_snake_case)]
impl IAudioSessionEvents_Impl for SessEv_Impl {
    fn OnDisplayNameChanged(&self, _n: &PCWSTR, _c: *const GUID) -> Result<()> {
        Ok(())
    }
    fn OnIconPathChanged(&self, _n: &PCWSTR, _c: *const GUID) -> Result<()> {
        Ok(())
    }
    fn OnSimpleVolumeChanged(&self, v: f32, _mute: BOOL, ctx: *const GUID) -> Result<()> {
        unsafe {
            if !ctx.is_null() && *ctx == CTX {
                return Ok(());
            }
        }
        let pct = (v * 100.0).round() as i32;
        post(WM_VOL, focus::gesture() as usize, pct as isize);
        Ok(())
    }
    fn OnChannelVolumeChanged(&self, _n: u32, _a: *const f32, _c: u32, _x: *const GUID) -> Result<()> {
        Ok(())
    }
    fn OnGroupingParamChanged(&self, _g: *const GUID, _c: *const GUID) -> Result<()> {
        Ok(())
    }
    fn OnStateChanged(&self, _s: AudioSessionState) -> Result<()> {
        Ok(())
    }
    fn OnSessionDisconnected(&self, _r: AudioSessionDisconnectReason) -> Result<()> {
        post(WM_REBIND, 0, 0);
        Ok(())
    }
}

#[implement(IMMNotificationClient)]
struct DevEv {
    _p: (),
}

#[allow(non_snake_case)]
impl IMMNotificationClient_Impl for DevEv_Impl {
    fn OnDeviceStateChanged(&self, _id: &PCWSTR, _s: DEVICE_STATE) -> Result<()> {
        post(WM_REBIND, 0, 0);
        Ok(())
    }
    fn OnDeviceAdded(&self, _id: &PCWSTR) -> Result<()> {
        post(WM_REBIND, 0, 0);
        Ok(())
    }
    fn OnDeviceRemoved(&self, _id: &PCWSTR) -> Result<()> {
        post(WM_REBIND, 0, 0);
        Ok(())
    }
    fn OnDefaultDeviceChanged(&self, flow: EDataFlow, _r: ERole, _id: &PCWSTR) -> Result<()> {
        if flow == eRender {
            post(WM_REBIND, 0, 0);
        }
        Ok(())
    }
    fn OnPropertyValueChanged(&self, _id: &PCWSTR, _k: &PROPERTYKEY) -> Result<()> {
        Ok(())
    }
}

#[implement(IAudioSessionNotification)]
struct SessNote {
    _p: (),
}

#[allow(non_snake_case)]
impl IAudioSessionNotification_Impl for SessNote_Impl {
    fn OnSessionCreated(&self, _s: Ref<'_, IAudioSessionControl>) -> Result<()> {
        post(WM_RESCAN, 0, 0);
        Ok(())
    }
}

#[implement(IAudioEndpointVolumeCallback)]
struct EpCb {
    _p: (),
}

#[allow(non_snake_case)]
impl IAudioEndpointVolumeCallback_Impl for EpCb_Impl {
    fn OnNotify(&self, _n: *mut AUDIO_VOLUME_NOTIFICATION_DATA) -> Result<()> {
        post(WM_MASTER, 0, 0);
        Ok(())
    }
}

// ---- engine ----

struct Eng {
    en: IMMDeviceEnumerator,
    dev_ev: IMMNotificationClient,
    sess_note: IAudioSessionNotification,
    mgr: Option<IAudioSessionManager2>,
    ctl: Option<IAudioSessionControl>,
    ctl_ev: Option<IAudioSessionEvents>,
    vol: Option<ISimpleAudioVolume>,
    t_main: usize,
    t_rebind: usize,
    t_persist: usize,
    ep: Option<IAudioEndpointVolume>,
    ep_cb: IAudioEndpointVolumeCallback,
    muted_by_us: bool,
    mute_start: u64,
    mute_until: u64,
    t_unmute: usize,
    ticks: u32,
    n_user: u32,
    n_ding: u32,
}

fn trim() {
    unsafe {
        let _ = SetProcessWorkingSetSize(GetCurrentProcess(), usize::MAX, usize::MAX);
    }
}

fn in_window(s: i32, e: i32) -> bool {
    let t = unsafe { GetLocalTime() };
    let n = t.wHour as i32 * 60 + t.wMinute as i32;
    if s == e {
        false
    } else if s < e {
        n >= s && n < e
    } else {
        n >= s || n < e
    }
}

impl Eng {
    unsafe fn new() -> Result<Eng> {
        let en: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        let dev_ev: IMMNotificationClient = DevEv { _p: () }.into();
        let sess_note: IAudioSessionNotification = SessNote { _p: () }.into();
        en.RegisterEndpointNotificationCallback(&dev_ev)?;
        let ep_cb: IAudioEndpointVolumeCallback = EpCb { _p: () }.into();
        Ok(Eng {
            en,
            dev_ev,
            sess_note,
            mgr: None,
            ctl: None,
            ctl_ev: None,
            vol: None,
            t_main: 0,
            t_rebind: 0,
            t_persist: 0,
            ep: None,
            ep_cb,
            muted_by_us: false,
            mute_start: 0,
            mute_until: 0,
            t_unmute: 0,
            ticks: 0,
            n_user: 0,
            n_ding: 0,
        })
    }

    unsafe fn unbind(&mut self) {
        self.unmute();
        if let Some(ep) = &self.ep {
            let _ = ep.UnregisterControlChangeNotify(&self.ep_cb);
        }
        if let (Some(c), Some(e)) = (&self.ctl, &self.ctl_ev) {
            let _ = c.UnregisterAudioSessionNotification(e);
        }
        if let Some(m) = &self.mgr {
            let _ = m.UnregisterSessionNotification(&self.sess_note);
        }
        self.ctl = None;
        self.ctl_ev = None;
        self.vol = None;
        self.mgr = None;
        self.ep = None;
    }

    unsafe fn bind(&mut self) -> Result<()> {
        self.unbind();
        let dev = self.en.GetDefaultAudioEndpoint(eRender, eConsole)?;
        let mgr: IAudioSessionManager2 = dev.Activate(CLSCTX_ALL, None)?;
        mgr.RegisterSessionNotification(&self.sess_note)?;
        self.mgr = Some(mgr.clone());
        if let Ok(ep) = dev.Activate::<IAudioEndpointVolume>(CLSCTX_ALL, None) {
            if ep.RegisterControlChangeNotify(&self.ep_cb).is_ok() {
                self.ep = Some(ep);
            }
        }
        self.find_session(&mgr)
    }

    unsafe fn find_session(&mut self, mgr: &IAudioSessionManager2) -> Result<()> {
        let list = mgr.GetSessionEnumerator()?;
        let n = list.GetCount()?;
        for i in 0..n {
            let c = list.GetSession(i)?;
            let c2: IAudioSessionControl2 = c.cast()?;
            if c2.IsSystemSoundsSession().0 == 0 {
                let ev: IAudioSessionEvents = SessEv { _p: () }.into();
                c.RegisterAudioSessionNotification(&ev)?;
                let v: ISimpleAudioVolume = c.cast()?;
                self.ctl = Some(c);
                self.ctl_ev = Some(ev);
                self.vol = Some(v);
                lg!("bound system sounds session");
                return Ok(());
            }
        }
        Ok(())
    }

    // Re-enumerate only; never re-register (registration re-fires OnSessionCreated).
    unsafe fn rescan(&mut self) {
        let r = match self.mgr.clone() {
            Some(m) => self.find_session(&m),
            None => self.bind(),
        };
        if let Err(e) = r {
            lg!("rescan failed: {e}");
        }
        if self.vol.is_some() {
            self.guard("rescan");
        }
    }

    unsafe fn cur(&self) -> Option<i32> {
        let v = self.vol.as_ref()?.GetMasterVolume().ok()?;
        Some((v * 100.0).round() as i32)
    }

    unsafe fn set(&self, pct: i32) {
        if let Some(v) = &self.vol {
            let _ = v.SetMasterVolume(pct as f32 / 100.0, &CTX);
        }
    }

    // Force the session to the target (adopt current on first run).
    unsafe fn enforce(&mut self, why: &str) {
        let Some(cur) = self.cur() else { return };
        let tgt = get(K_VOLUME);
        if tgt < 0 {
            put(K_VOLUME, cur);
            save(K_VOLUME);
            lg!("adopted {cur}% as target");
            return;
        }
        if cur != tgt {
            self.set(tgt);
            if why != "apply" {
                inc(K_RESETS);
            }
            lg!("{why}: {cur}% -> {tgt}%");
        }
    }

    unsafe fn guard(&mut self, why: &str) {
        if get(K_LOCK) != 0 || get(K_VOLUME) < 0 {
            self.enforce(why);
        }
    }

    unsafe fn on_vol(&mut self, user: bool, pct: i32) {
        if get(K_LOCK) == 0 {
            return;
        }
        let tgt = get(K_VOLUME);
        if pct == tgt {
            return;
        }
        if user || tgt < 0 {
            put(K_VOLUME, pct);
            self.persist_later();
        } else {
            self.set(tgt);
            inc(K_RESETS);
            let (_, n) = focus::fg_settings();
            lg!("blocked external change {pct}% -> restored {tgt}% (fg: {n})");
        }
    }

    unsafe fn persist_later(&mut self) {
        if self.t_persist == 0 {
            self.t_persist = SetTimer(None, 0, 400, None);
        }
    }

    unsafe fn eval_quiet(&mut self) {
        let want = get(K_QUIET_MANUAL) != 0
            || (get(K_SCHED_ON) != 0 && in_window(get(K_SCHED_START), get(K_SCHED_END)));
        let active = get(K_QUIET_ACTIVE) != 0;
        if want && !active {
            if quiet::enter() {
                put(K_QUIET_ACTIVE, 1);
                save(K_QUIET_ACTIVE);
                lg!("quiet on");
            }
        } else if !want && active {
            quiet::leave();
            put(K_QUIET_ACTIVE, 0);
            save(K_QUIET_ACTIVE);
            lg!("quiet off");
        }
    }

    unsafe fn on_timer(&mut self, id: usize) {
        if id == 0 {
            return;
        }
        if id == self.t_main {
            self.ticks += 1;
            if self.ticks == 1 || self.ticks % 1440 == 0 {
                let _ = std::thread::Builder::new().stack_size(256 * 1024).spawn(update::check);
            }
            if self.ticks % 10 == 0 {
                trim();
            }
            if self.vol.is_none() {
                self.rescan();
            } else if get(K_LOCK) != 0 && !focus::gesture() {
                self.enforce("watchdog");
            }
            self.eval_quiet();
        } else if id == self.t_rebind {
            let _ = KillTimer(None, id);
            self.t_rebind = 0;
            if let Err(e) = self.bind() {
                lg!("bind failed: {e}");
            }
            self.guard("rebind");
        } else if id == self.t_persist {
            let _ = KillTimer(None, id);
            self.t_persist = 0;
            save(K_VOLUME);
            self.n_user += 1;
            lg!("target volume {}%", get(K_VOLUME));
        } else if id == self.t_unmute {
            let now = GetTickCount64();
            if now >= self.mute_start + 8000 || (now >= self.mute_until && !focus::mouse_down()) {
                let _ = KillTimer(None, id);
                self.t_unmute = 0;
                self.unmute();
            }
        }
    }

    // Main volume moved: mute System sounds until the change (and mouse release) is over.
    unsafe fn on_master(&mut self) {
        if get(K_DING) == 0 {
            return;
        }
        let Some(v) = &self.vol else { return };
        let now = GetTickCount64();
        if !self.muted_by_us {
            let was = v.GetMute().map(|b| b.as_bool()).unwrap_or(true);
            if was || v.SetMute(true, &CTX).is_err() {
                return;
            }
            self.muted_by_us = true;
            self.mute_start = now;
            self.n_ding += 1;
        }
        self.mute_until = now + 700;
        if self.t_unmute == 0 {
            self.t_unmute = SetTimer(None, 0, 100, None);
        }
    }

    unsafe fn unmute(&mut self) {
        if self.muted_by_us {
            if let Some(v) = &self.vol {
                let _ = v.SetMute(false, &CTX);
            }
            self.muted_by_us = false;
        }
    }

    unsafe fn run_loop(&mut self) {
        if let Err(e) = self.bind() {
            lg!("bind failed: {e}");
        }
        self.guard("start");
        self.eval_quiet();
        self.t_main = SetTimer(None, 0, 60_000, None);
        trim();

        let mut m = MSG::default();
        while GetMessageW(&mut m, None, 0, 0).0 > 0 {
            match m.message {
                WM_VOL => self.on_vol(m.wParam.0 != 0, m.lParam.0 as i32),
                WM_APPLY => {
                    self.enforce("apply");
                    self.persist_later();
                }
                WM_EVAL => self.eval_quiet(),
                WM_MASTER => self.on_master(),
                WM_RESCAN => self.rescan(),
                WM_REBIND => {
                    if self.t_rebind == 0 {
                        self.t_rebind = SetTimer(None, 0, 500, None);
                    }
                }
                WM_TIMER => self.on_timer(m.wParam.0),
                _ => {}
            }
        }

        for t in [self.t_main, self.t_rebind, self.t_persist, self.t_unmute] {
            if t != 0 {
                let _ = KillTimer(None, t);
            }
        }
        self.unbind();
        let _ = self.en.UnregisterEndpointNotificationCallback(&self.dev_ev);
        if get(K_QUIET_ACTIVE) != 0 {
            quiet::leave();
            put(K_QUIET_ACTIVE, 0);
            save(K_QUIET_ACTIVE);
        }
        save(K_VOLUME);
        lg!("stop");
        crate::log::session_end(get(K_RESETS), self.n_user, self.n_ding);
    }
}

pub fn run() {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let mut m = MSG::default();
        let _ = PeekMessageW(&mut m, None, WM_USER, WM_USER, PM_NOREMOVE);
        TID.store(GetCurrentThreadId(), Ordering::Release);
        match Eng::new() {
            Ok(mut e) => e.run_loop(),
            Err(err) => lg!("engine init failed: {err}"),
        }
        CoUninitialize();
    }
}