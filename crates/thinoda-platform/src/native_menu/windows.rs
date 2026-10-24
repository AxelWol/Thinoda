use std::{
    cell::{Cell, RefCell},
    collections::HashSet,
    io,
    ptr::null_mut,
};
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, WPARAM},
    UI::{
        Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
        WindowsAndMessaging::{
            AppendMenuW, CreateMenu, CreatePopupMenu, DestroyMenu, DrawMenuBar, GetMenu,
            MF_CHECKED, MF_GRAYED, MF_POPUP, MF_SEPARATOR, MF_STRING, SetMenu, WM_COMMAND,
            WM_ENTERMENULOOP, WM_EXITMENULOOP, WM_NCDESTROY,
        },
    },
};

use super::MenuItem;

const SUBCLASS: usize = 0x54484D45;

struct Events {
    live: Cell<bool>,
    tracking: Cell<bool>,
    allowed: RefCell<HashSet<u16>>,
    pending: RefCell<Vec<u16>>,
    wake: Box<dyn Fn()>,
}

/// Owns a window's menus on its event-loop thread. It never calls application
/// code from a Win32 callback; commands are drained at the next application frame.
pub struct WindowMenu {
    hwnd: HWND,
    menu: OwnedMenu,
    events: Box<Events>,
}

struct OwnedMenu(windows_sys::Win32::UI::WindowsAndMessaging::HMENU);

impl Drop for OwnedMenu {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: this handle is either unattached or detached before drop.
            unsafe { DestroyMenu(self.0) };
        }
    }
}

fn wide(text: &str) -> Vec<u16> {
    // Ampersands in document names are literal, not accidental access keys.
    text.replace('&', "&&")
        .replace('\0', "")
        .encode_utf16()
        .chain([0])
        .collect()
}

fn enabled_ids(items: &[MenuItem]) -> HashSet<u16> {
    let mut ids = HashSet::new();
    for item in items {
        match item {
            MenuItem::Action {
                id, enabled: true, ..
            } => {
                ids.insert(*id);
            }
            MenuItem::Submenu {
                items,
                enabled: true,
                ..
            } => ids.extend(enabled_ids(items)),
            _ => {}
        }
    }
    ids
}

fn build(items: &[MenuItem], popup: bool, ids: &mut HashSet<u16>) -> io::Result<OwnedMenu> {
    // SAFETY: native creation has no borrowed inputs.
    let menu = OwnedMenu(unsafe {
        if popup {
            CreatePopupMenu()
        } else {
            CreateMenu()
        }
    });
    if menu.0.is_null() {
        return Err(io::Error::last_os_error());
    }
    for item in items {
        let (flags, id, label, child) = match item {
            MenuItem::Action {
                id,
                label,
                enabled,
                checked,
            } => {
                if *id == 0 || !ids.insert(*id) {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "Duplicate or zero menu ID",
                    ));
                }
                (
                    MF_STRING
                        | if *enabled { 0 } else { MF_GRAYED }
                        | if *checked { MF_CHECKED } else { 0 },
                    usize::from(*id),
                    label.as_str(),
                    None,
                )
            }
            MenuItem::Submenu {
                label,
                enabled,
                items,
            } => {
                let child = build(items, true, ids)?;
                (
                    MF_POPUP | if *enabled { 0 } else { MF_GRAYED },
                    child.0 as usize,
                    label.as_str(),
                    Some(child),
                )
            }
            MenuItem::Separator => (MF_SEPARATOR, 0, "", None),
            MenuItem::Caption(text) => (MF_STRING | MF_GRAYED, 0, text.as_str(), None),
        };
        let label = wide(label);
        // SAFETY: label is NUL-terminated; parent and optional child are live.
        if unsafe { AppendMenuW(menu.0, flags, id, label.as_ptr()) } == 0 {
            return Err(io::Error::last_os_error());
        }
        if let Some(child) = child {
            // The parent now owns and recursively destroys the child.
            std::mem::forget(child);
        }
    }
    Ok(menu)
}

unsafe extern "system" fn callback(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _: usize,
    data: usize,
) -> LRESULT {
    // SAFETY: the Box stays at a stable address until the subclass is removed,
    // including when WM_NCDESTROY runs before the application is dropped.
    let events = unsafe { &*(data as *const Events) };
    if message == WM_NCDESTROY {
        events.live.set(false);
        unsafe { RemoveWindowSubclass(hwnd, Some(callback), SUBCLASS) };
    } else if message == WM_ENTERMENULOOP {
        events.tracking.set(true);
    } else if message == WM_EXITMENULOOP {
        events.tracking.set(false);
        (events.wake)();
    } else if message == WM_COMMAND && lparam == 0 && wparam >> 16 == 0 {
        let id = wparam as u16;
        if events.allowed.borrow().contains(&id) {
            events.pending.borrow_mut().push(id);
            (events.wake)();
            return 0;
        }
    }
    // SAFETY: forwards unhandled messages to the original procedure chain.
    unsafe { DefSubclassProc(hwnd, message, wparam, lparam) }
}

impl WindowMenu {
    /// The supplied handle must remain on the calling event-loop thread.
    ///
    /// # Safety
    /// `hwnd` is a valid window owned by the current thread. The wake callback
    /// must not panic or synchronously re-enter menu management.
    pub unsafe fn attach(
        hwnd: HWND,
        items: &[MenuItem],
        wake: impl Fn() + 'static,
    ) -> io::Result<Self> {
        if hwnd.is_null() || !unsafe { GetMenu(hwnd) }.is_null() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Window already has a menu or is null",
            ));
        }
        let mut allowed = HashSet::new();
        let menu = build(items, false, &mut allowed)?;
        let events = Box::new(Events {
            live: Cell::new(true),
            tracking: Cell::new(false),
            allowed: RefCell::new(enabled_ids(items)),
            pending: RefCell::new(Vec::new()),
            wake: Box::new(wake),
        });
        if unsafe {
            SetWindowSubclass(
                hwnd,
                Some(callback),
                SUBCLASS,
                &*events as *const Events as usize,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        if unsafe { SetMenu(hwnd, menu.0) } == 0 {
            unsafe { RemoveWindowSubclass(hwnd, Some(callback), SUBCLASS) };
            return Err(io::Error::last_os_error());
        }
        let owner = Self { hwnd, menu, events };
        if unsafe { DrawMenuBar(hwnd) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(owner)
    }

    pub fn replace(&mut self, items: &[MenuItem]) -> io::Result<()> {
        if !self.events.live.get() {
            return Err(io::Error::new(
                io::ErrorKind::NotConnected,
                "Menu window was destroyed",
            ));
        }
        if self.is_tracking() || !self.events.pending.borrow().is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "Close menus and drain menu commands before replacing",
            ));
        }
        let mut allowed = HashSet::new();
        let menu = build(items, false, &mut allowed)?;
        if unsafe { SetMenu(self.hwnd, menu.0) } == 0 {
            return Err(io::Error::last_os_error());
        }
        self.menu = menu;
        *self.events.allowed.borrow_mut() = enabled_ids(items);
        if unsafe { DrawMenuBar(self.hwnd) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    pub fn drain(&self) -> Vec<u16> {
        std::mem::take(&mut *self.events.pending.borrow_mut())
    }

    pub fn is_tracking(&self) -> bool {
        self.events.tracking.get()
    }
}

impl Drop for WindowMenu {
    fn drop(&mut self) {
        if self.events.live.get() {
            // SAFETY: this non-Send owner stays on the window thread; never
            // detach a replacement menu installed by another component.
            unsafe {
                RemoveWindowSubclass(self.hwnd, Some(callback), SUBCLASS);
                if GetMenu(self.hwnd) == self.menu.0 {
                    SetMenu(self.hwnd, null_mut());
                    DrawMenuBar(self.hwnd);
                }
            }
        } else {
            // DestroyWindow owns destruction of its attached menu.
            self.menu.0 = null_mut();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DestroyWindow, GetMenuItemCount, GetMenuState, GetSubMenu, IsMenu,
        MF_BYPOSITION, SendMessageW, WS_OVERLAPPEDWINDOW,
    };

    struct TestWindow(HWND);
    impl TestWindow {
        fn new() -> Self {
            let class = wide("STATIC");
            let title = wide("Thinoda menu fixture");
            let hwnd = unsafe {
                CreateWindowExW(
                    0,
                    class.as_ptr(),
                    title.as_ptr(),
                    WS_OVERLAPPEDWINDOW,
                    0,
                    0,
                    400,
                    300,
                    null_mut(),
                    null_mut(),
                    null_mut(),
                    null_mut(),
                )
            };
            assert!(!hwnd.is_null(), "{}", io::Error::last_os_error());
            Self(hwnd)
        }
    }
    impl Drop for TestWindow {
        fn drop(&mut self) {
            unsafe { DestroyWindow(self.0) };
        }
    }

    fn fixture(enabled: bool) -> Vec<MenuItem> {
        vec![MenuItem::Submenu {
            label: "File & maps".into(),
            enabled: true,
            items: vec![
                MenuItem::Action {
                    id: 1,
                    label: "New".into(),
                    enabled,
                    checked: false,
                },
                MenuItem::Separator,
                MenuItem::Action {
                    id: 2,
                    label: "Save".into(),
                    enabled: false,
                    checked: false,
                },
            ],
        }]
    }

    #[test]
    fn native_window_menu_dispatches_once_and_replaces_without_leaks() {
        let window = TestWindow::new();
        let mut menu = unsafe { WindowMenu::attach(window.0, &fixture(true), || {}) }.unwrap();
        let first = menu.menu.0;
        assert_eq!(unsafe { GetMenu(window.0) }, first);
        assert_eq!(unsafe { GetMenuItemCount(first) }, 1);
        assert_eq!(unsafe { GetMenuItemCount(GetSubMenu(first, 0)) }, 3);
        unsafe {
            SendMessageW(window.0, WM_COMMAND, 1, 0);
            SendMessageW(window.0, WM_COMMAND, 2, 0);
            SendMessageW(window.0, WM_COMMAND, 1 | (1 << 16), 0);
            SendMessageW(window.0, WM_COMMAND, 99, 0);
        }
        assert_eq!(
            menu.replace(&fixture(false)).unwrap_err().kind(),
            io::ErrorKind::WouldBlock
        );
        assert_eq!(menu.drain(), [1]);
        assert!(menu.drain().is_empty());
        menu.replace(&fixture(false)).unwrap();
        assert_eq!(unsafe { IsMenu(first) }, 0);
        unsafe { SendMessageW(window.0, WM_COMMAND, 1, 0) };
        assert!(menu.drain().is_empty());
        let final_menu = menu.menu.0;
        drop(menu);
        assert!(unsafe { GetMenu(window.0) }.is_null());
        assert_eq!(unsafe { IsMenu(final_menu) }, 0);
    }

    #[test]
    fn invalid_replacement_keeps_attached_menu_and_ids() {
        let window = TestWindow::new();
        let mut menu = unsafe { WindowMenu::attach(window.0, &fixture(true), || {}) }.unwrap();
        let original = menu.menu.0;
        let duplicate = vec![
            MenuItem::Action {
                id: 1,
                label: "One".into(),
                enabled: true,
                checked: false,
            },
            MenuItem::Action {
                id: 1,
                label: "Two".into(),
                enabled: true,
                checked: false,
            },
        ];
        assert_eq!(
            menu.replace(&duplicate).unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
        assert_eq!(unsafe { GetMenu(window.0) }, original);
        unsafe { SendMessageW(window.0, WM_COMMAND, 1, 0) };
        assert_eq!(menu.drain(), [1]);
    }

    #[test]
    fn destroying_window_removes_callback_before_menu_owner_drops() {
        let window = TestWindow::new();
        let mut menu = unsafe { WindowMenu::attach(window.0, &fixture(true), || {}) }.unwrap();
        drop(window);
        assert!(!menu.events.live.get());
        assert_eq!(
            menu.replace(&fixture(true)).unwrap_err().kind(),
            io::ErrorKind::NotConnected
        );
        drop(menu);
    }

    #[test]
    fn menu_tracking_keeps_the_presented_snapshot_alive_until_exit() {
        let window = TestWindow::new();
        let mut menu = unsafe { WindowMenu::attach(window.0, &fixture(true), || {}) }.unwrap();
        let original = menu.menu.0;
        unsafe { SendMessageW(window.0, WM_ENTERMENULOOP, 0, 0) };
        assert!(menu.is_tracking());
        assert_eq!(
            menu.replace(&fixture(false)).unwrap_err().kind(),
            io::ErrorKind::WouldBlock
        );
        assert_eq!(unsafe { GetMenu(window.0) }, original);
        unsafe { SendMessageW(window.0, WM_EXITMENULOOP, 0, 0) };
        assert!(!menu.is_tracking());
        menu.replace(&fixture(false)).unwrap();
    }

    #[test]
    fn submenu_enabled_state_is_owned_by_app_not_inferred_from_disabled_children() {
        let window = TestWindow::new();
        let mut menu = unsafe { WindowMenu::attach(window.0, &fixture(false), || {}) }.unwrap();
        assert_eq!(
            unsafe { GetMenuState(menu.menu.0, 0, MF_BYPOSITION) } & MF_GRAYED,
            0
        );
        let mut items = fixture(true);
        let MenuItem::Submenu { enabled, .. } = &mut items[0] else {
            unreachable!()
        };
        *enabled = false;
        menu.replace(&items).unwrap();
        assert_ne!(
            unsafe { GetMenuState(menu.menu.0, 0, MF_BYPOSITION) } & MF_GRAYED,
            0
        );
        unsafe { SendMessageW(window.0, WM_COMMAND, 1, 0) };
        assert!(menu.drain().is_empty());
    }
}
