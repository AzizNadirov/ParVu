//! Showing, restoring and minimising windows, with the Wayland rules that
//! make the obvious calls fail there (seen on GNOME 50).
//!
//! * A Wayland client cannot unminimise or raise its own window. Only an
//!   xdg-activation token from the user's action lets it: the launcher puts
//!   one in a second `parvu`'s environment (`XDG_ACTIVATION_TOKEN`), and the
//!   top bar's tray hands one over with `ProvideXdgActivationToken` before
//!   it sends the menu click. Without one, GTK makes up a token GNOME refuses:
//!   the window stays minimised and only a "ParVu is ready" notification
//!   shows. [`bring_forward`] uses the token when there is one, and else maps
//!   the window afresh, which GNOME shows in front like any new window.
//! * GTK ignores minimising a window that is not on screen yet, so a hidden
//!   window shown minimised popped up in front of the user's work instead.
//!   [`show_minimised`] minimises it the moment it is mapped.
//! * A window created hidden and shown later counts its size without the
//!   title bar GTK draws on Wayland; [`match_shown_size`] corrects that.

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use tauri::WebviewWindow;

/// The size of a new window.
pub const WIDTH: f64 = 1400.0;
pub const HEIGHT: f64 = 900.0;

/// Whether the app runs on Wayland (set once in `init`, on the main thread).
#[cfg(target_os = "linux")]
static WAYLAND: std::sync::OnceLock<bool> = std::sync::OnceLock::new();

fn is_wayland() -> bool {
    #[cfg(target_os = "linux")]
    {
        WAYLAND.get().copied().unwrap_or(false)
    }
    #[cfg(not(target_os = "linux"))]
    {
        false
    }
}

/// Windows waiting to be minimised the moment they appear.
static MINIMISE_ON_MAP: Mutex<PendingMinimise> = Mutex::new(PendingMinimise::new());

/// The latest activation token the tray gave us, and when.
static TRAY_TOKEN: Mutex<TokenSlot> = Mutex::new(TokenSlot::new());

/// A tray token older than this belongs to some earlier click.
const TRAY_TOKEN_MAX_AGE: Duration = Duration::from_secs(10);

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Call once at startup, on the main thread, before any window is shown.
pub fn init() {
    #[cfg(target_os = "linux")]
    {
        use gtk::glib::prelude::*;
        let wayland = gtk::gdk::Display::default().is_some_and(|d| d.type_().name() == "GdkWaylandDisplay");
        let _ = WAYLAND.set(wayland);
        tracing::debug!("windows: Wayland {wayland}");
        if wayland {
            linux::take_tray_tokens();
        }
    }
}

/// Set up a new window: call right after building it.
pub fn prepare(w: &WebviewWindow) {
    #[cfg(target_os = "linux")]
    on_main_thread(w, linux::prepare);
    #[cfg(not(target_os = "linux"))]
    let _ = w;
}

/// The activation token a launcher gave this process, to pass on to the
/// running ParVu (Wayland's `XDG_ACTIVATION_TOKEN`, X11's
/// `DESKTOP_STARTUP_ID`).
pub fn launch_token(env: impl Fn(&str) -> Option<String>) -> Option<String> {
    ["XDG_ACTIVATION_TOKEN", "DESKTOP_STARTUP_ID"].into_iter().filter_map(env).map(|t| t.trim().to_string()).find(|t| !t.is_empty())
}

/// The token the tray handed over for the click being handled, if fresh.
pub fn take_tray_token() -> Option<String> {
    lock(&TRAY_TOKEN).take(Instant::now(), TRAY_TOKEN_MAX_AGE)
}

/// Bring `w` to the front (restored if minimised or hidden). `token` is
/// the activation token of the user's action that asks for it, if any.
pub fn bring_forward(w: &WebviewWindow, token: Option<String>) {
    // The user wants it now: a pending "minimise when shown" is void.
    lock(&MINIMISE_ON_MAP).cancel(w.label());
    #[cfg(target_os = "linux")]
    {
        let wayland = is_wayland();
        on_main_thread(w, move |w| linux::bring_forward(w, token, wayland));
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = token;
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

/// Show a hidden window minimised, without taking the focus.
pub fn show_minimised(w: &WebviewWindow) -> tauri::Result<()> {
    if is_wayland() {
        // Minimising before the window is mapped does nothing on Wayland,
        // and once shown GNOME gives it the focus. The `map-event` hook of
        // `prepare` minimises it the moment it is mapped, before it is
        // drawn or focused; it does so once, so a window the user restores
        // stays restored. (GNOME places a window when it is first shown, so
        // one that starts minimised comes back at the top-left corner.)
        lock(&MINIMISE_ON_MAP).arm(w.label());
        w.show()
    } else if cfg!(target_os = "linux") {
        // On X11, GTK iconifies a window that isn't shown yet, so it appears
        // minimised without flashing on screen.
        w.minimize()?;
        w.show()
    } else {
        // Windows and macOS keep a hidden window hidden when minimising it;
        // show it without focus, then minimise.
        w.show()?;
        w.minimize()
    }
}

/// Draw the user's attention to `w` without moving it in front of their
/// work: the taskbar entry flashes or is marked urgent.
pub fn request_attention(w: &WebviewWindow) {
    if !is_wayland() {
        if let Err(e) = w.request_user_attention(Some(tauri::UserAttentionType::Critical)) {
            tracing::warn!("window {}: could not request attention: {e}", w.label());
        }
        return;
    }
    // GTK 3 has no urgency hint on Wayland. What GNOME offers instead: a
    // window that asks to be activated without the user's say-so is marked
    // as demanding attention ("ParVu is ready", highlighted in the dock)
    // and stays where it is. A window about to appear asks once mapped.
    if lock(&MINIMISE_ON_MAP).want_attention(w.label()) {
        return;
    }
    #[cfg(target_os = "linux")]
    on_main_thread(w, linux::ask_to_activate);
}

/// Give a window built hidden the size a window shown at once gets.
///
/// On Wayland the title bar is part of the window (tao draws it with GTK).
/// For a window shown as soon as it is built, GTK counts the requested
/// 1400 x 900 with the title bar; for one built hidden and shown later,
/// without it, so that window came out a title bar (47 px) taller. Asking
/// the hidden window for the height less its title bar makes both the same.
/// No-op where the window manager draws the title bar (X11, Windows, macOS).
pub fn match_shown_size(w: &WebviewWindow) {
    #[cfg(target_os = "linux")]
    on_main_thread(w, linux::match_shown_size);
    #[cfg(not(target_os = "linux"))]
    let _ = w;
}

/// Run `f` with `w` on the main thread: now when called there (GTK must be
/// used there; setup runs there), else as soon as the event loop can.
#[cfg(target_os = "linux")]
fn on_main_thread(w: &WebviewWindow, f: impl FnOnce(&WebviewWindow) + Send + 'static) {
    if gtk::is_initialized_main_thread() {
        f(w);
    } else {
        let w2 = w.clone();
        if let Err(e) = w.run_on_main_thread(move || f(&w2)) {
            tracing::warn!("windows: {e}");
        }
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use gtk::glib::translate::ToGlibPtr;
    use gtk::prelude::*;
    use tauri::WebviewWindow;

    unsafe extern "C" {
        // GDK 3.22+, in libgdk-3, which GTK links already.
        fn gdk_wayland_display_set_startup_notification_id(
            display: *mut gtk::gdk::ffi::GdkDisplay,
            startup_id: *const std::ffi::c_char,
        );
    }

    pub fn prepare(w: &WebviewWindow) {
        let Ok(g) = w.gtk_window() else { return };
        let_title_bar_buttons_click(&g);
        let label = w.label().to_string();
        g.connect_map_event(move |g, _| {
            let pending = super::lock(&super::MINIMISE_ON_MAP).take(&label);
            if let Some(attention) = pending {
                tracing::info!("window {label}: minimised as it appeared");
                g.iconify();
                if attention {
                    ask_to_activate_gtk(g);
                }
            }
            gtk::glib::Propagation::Proceed
        });
    }

    /// On Wayland, tao 0.35 draws the title bar itself: a GTK header bar
    /// inside a `GtkEventBox` with `above_child` set, whose input window lies
    /// over the bar. In a window shown at once, the minimise / maximise /
    /// close buttons end up above it and get their clicks. In a window shown
    /// later (a background start, or one mapped again to bring it forward),
    /// the event box ends up on top (seen on GNOME 50): it takes every click
    /// on those buttons, which then reaches tao's window handler and stops
    /// there, so the buttons don't respond. With the box below its child, the
    /// buttons get their own clicks; clicks elsewhere on the bar (drag,
    /// double-click) still reach the window as before. Newer tao drops this
    /// title bar (tao#1218). No-op on X11, where the window manager draws it.
    fn let_title_bar_buttons_click(g: &gtk::ApplicationWindow) {
        let Some(bar) = g.titlebar() else { return };
        if let Ok(event_box) = bar.downcast::<gtk::EventBox>() {
            event_box.set_above_child(false);
        }
    }

    pub fn ask_to_activate(w: &WebviewWindow) {
        if let Ok(g) = w.gtk_window() {
            ask_to_activate_gtk(&g);
        }
    }

    /// Without a token of the user's, GTK asks with one of its own, which
    /// GNOME declines when another app has the focus: it marks the window
    /// as demanding attention instead, leaving it minimised or behind.
    fn ask_to_activate_gtk(g: &gtk::ApplicationWindow) {
        if !g.is_active() {
            g.present();
        }
    }

    pub fn match_shown_size(w: &WebviewWindow) {
        let Ok(g) = w.gtk_window() else { return };
        let Some(bar) = g.titlebar() else { return };
        // Its header bar is shown with the window; show it now to measure it.
        bar.show_all();
        let (_, natural) = bar.preferred_height();
        if natural > 0 {
            tracing::debug!("window {}: title bar {natural} px, asking for {}x{}", w.label(), super::WIDTH, super::HEIGHT as i32 - natural);
            g.resize(super::WIDTH as i32, super::HEIGHT as i32 - natural);
        }
    }

    pub fn bring_forward(w: &WebviewWindow, token: Option<String>, wayland: bool) {
        let Ok(g) = w.gtk_window() else { return };
        if !wayland {
            // X11: the window manager restores and raises on request; a
            // launcher's startup id carries the time of the user's action.
            if let Some(id) = &token {
                g.set_startup_id(id);
            }
            g.deiconify();
            g.show_all();
            g.present();
            return;
        }
        if let Some(token) = token {
            // GTK activates the window with this token at the next present.
            let Ok(id) = std::ffi::CString::new(token) else { return };
            let display = g.display();
            unsafe { gdk_wayland_display_set_startup_notification_id(display.to_glib_none().0, id.as_ptr()) };
            tracing::info!("window {}: brought forward with an activation token", w.label());
            g.show_all();
            g.present();
            return;
        }
        if g.is_visible() && g.is_active() {
            return; // already in front, with the focus
        }
        // No token: a present() would only end in a "ParVu is ready"
        // notification, and the window may be minimised, which GTK can't
        // tell on Wayland. A window mapped afresh is shown in front, focused,
        // like any new window; GTK keeps its size and maximised state.
        tracing::info!("window {}: no activation token; mapping it again to bring it forward", w.label());
        if g.is_visible() {
            g.hide();
        }
        g.show_all();
    }

    /// Catch the activation token the top bar's tray sends before a menu
    /// click (`ProvideXdgActivationToken` of the StatusNotifierItem spec,
    /// which the appindicator library does not implement). It arrives on the
    /// session bus connection the library shares with us.
    pub fn take_tray_tokens() {
        use gtk::gio;
        let conn = match gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE) {
            Ok(c) => c,
            Err(e) => {
                tracing::debug!("tray: no session bus: {e}");
                return;
            }
        };
        conn.add_filter(|conn, msg, incoming| {
            let ours = incoming
                && msg.message_type() == gio::DBusMessageType::MethodCall
                && msg.member().as_deref() == Some("ProvideXdgActivationToken")
                && msg.interface().as_deref() == Some("org.kde.StatusNotifierItem");
            if !ours {
                return Some(msg.clone());
            }
            let token = msg.body().and_then(|b| b.try_child_value(0)).and_then(|v| v.str().map(str::to_string));
            if let Some(token) = token {
                tracing::debug!("tray: got an activation token");
                super::lock(&super::TRAY_TOKEN).put(token, std::time::Instant::now());
            }
            let _ = conn.send_message(&msg.new_method_reply(), gio::DBusSendMessageFlags::NONE);
            None
        });
    }
}

/// Windows (labels) to minimise when they appear, each with whether to
/// ask for the user's attention then.
#[derive(Debug, Default)]
pub struct PendingMinimise {
    windows: BTreeMap<String, bool>,
}

impl PendingMinimise {
    pub const fn new() -> Self {
        Self { windows: BTreeMap::new() }
    }

    /// Minimise `label` when it is next mapped.
    pub fn arm(&mut self, label: &str) {
        self.windows.entry(label.to_string()).or_insert(false);
    }

    /// The user brought it up first: leave it.
    pub fn cancel(&mut self, label: &str) {
        self.windows.remove(label);
    }

    /// Ask for attention once `label` is minimised. `false` when it isn't
    /// waiting to appear (ask now instead).
    pub fn want_attention(&mut self, label: &str) -> bool {
        self.windows.get_mut(label).map(|a| *a = true).is_some()
    }

    /// `label` was mapped: `Some(attention)` when it is to be minimised
    /// now. Once only, so a window the user restores stays restored.
    pub fn take(&mut self, label: &str) -> Option<bool> {
        self.windows.remove(label)
    }
}

/// Holds one activation token for a short while.
#[derive(Debug)]
pub struct TokenSlot {
    token: Option<(String, Instant)>,
}

impl TokenSlot {
    pub const fn new() -> Self {
        Self { token: None }
    }

    pub fn put(&mut self, token: String, now: Instant) {
        self.token = Some((token, now));
    }

    /// The token, if it came no longer than `max_age` before `now`. Either
    /// way the slot is empty afterwards: a token is good for one activation.
    pub fn take(&mut self, now: Instant, max_age: Duration) -> Option<String> {
        let (token, at) = self.token.take()?;
        (now.saturating_duration_since(at) <= max_age).then_some(token)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launch_token_prefers_wayland_and_skips_empty() {
        let env = |pairs: &'static [(&'static str, &'static str)]| {
            move |k: &str| pairs.iter().find(|(n, _)| *n == k).map(|(_, v)| v.to_string())
        };
        assert_eq!(launch_token(env(&[])), None);
        assert_eq!(launch_token(env(&[("XDG_ACTIVATION_TOKEN", "wl")])).as_deref(), Some("wl"));
        assert_eq!(launch_token(env(&[("DESKTOP_STARTUP_ID", "x11_TIME123")])).as_deref(), Some("x11_TIME123"));
        assert_eq!(
            launch_token(env(&[("XDG_ACTIVATION_TOKEN", "wl"), ("DESKTOP_STARTUP_ID", "x11")])).as_deref(),
            Some("wl")
        );
        assert_eq!(launch_token(env(&[("XDG_ACTIVATION_TOKEN", " "), ("DESKTOP_STARTUP_ID", "x11")])).as_deref(), Some("x11"));
    }

    #[test]
    fn tray_token_is_used_once_and_only_while_fresh() {
        let t0 = Instant::now();
        let max = Duration::from_secs(10);
        let mut s = TokenSlot::new();
        assert_eq!(s.take(t0, max), None);
        s.put("a".into(), t0);
        assert_eq!(s.take(t0 + Duration::from_secs(1), max).as_deref(), Some("a"));
        assert_eq!(s.take(t0 + Duration::from_secs(1), max), None);
        // A stale token (from a click nobody acted on) is dropped.
        s.put("b".into(), t0);
        assert_eq!(s.take(t0 + Duration::from_secs(11), max), None);
        assert_eq!(s.take(t0 + Duration::from_secs(11), max), None);
        // The newest token wins.
        s.put("c".into(), t0);
        s.put("d".into(), t0 + Duration::from_secs(2));
        assert_eq!(s.take(t0 + Duration::from_secs(3), max).as_deref(), Some("d"));
    }

    #[test]
    fn minimise_on_map_fires_once_and_not_after_the_user_opened_it() {
        let mut p = PendingMinimise::new();
        // Shown for an approval: minimised when it appears, once, with an
        // attention request.
        p.arm("main");
        assert!(p.want_attention("main"));
        assert_eq!(p.take("main"), Some(true));
        // Mapped again later (the user restores it, or brings it back from
        // the tray): it stays up.
        assert_eq!(p.take("main"), None);
        // Not waiting to appear: the caller asks for attention at once.
        assert!(!p.want_attention("main"));
        // A start without a tray: minimised, nothing else.
        p.arm("main");
        assert_eq!(p.take("main"), Some(false));
        // The user opened it before it appeared: not minimised.
        p.arm("main");
        p.cancel("main");
        assert_eq!(p.take("main"), None);
        // Arming again keeps a pending attention request; windows are apart.
        p.arm("a");
        p.want_attention("a");
        p.arm("a");
        p.arm("b");
        assert_eq!(p.take("c"), None);
        assert_eq!(p.take("a"), Some(true));
        assert_eq!(p.take("b"), Some(false));
    }
}
