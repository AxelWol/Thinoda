use super::MacMenuModel;
use std::{
    ffi::{CStr, c_char, c_void},
    io,
    marker::PhantomData,
    ptr::NonNull,
    rc::Rc,
};

unsafe extern "C" {
    fn thinoda_menu_attach(
        json: *const c_char,
        wake: unsafe extern "C" fn(*mut c_void),
        context: *mut c_void,
        result: *mut *mut c_void,
        error: *mut c_char,
        length: usize,
    ) -> i32;
    fn thinoda_menu_replace(
        handle: *mut c_void,
        json: *const c_char,
        error: *mut c_char,
        length: usize,
    ) -> i32;
    fn thinoda_menu_drain(handle: *mut c_void, output: *mut u16, capacity: usize) -> usize;
    fn thinoda_menu_detach(handle: *mut c_void);
}

struct Wake(Box<dyn Fn()>);

/// AppKit-owned global menus. All methods and destruction stay on the main thread.
pub struct ApplicationMenu {
    handle: NonNull<c_void>,
    _wake: Box<Wake>,
    _main_thread: PhantomData<Rc<()>>,
}

unsafe extern "C" fn wake(context: *mut c_void) {
    // SAFETY: the Box is stable and the native owner clears its callback before release.
    (unsafe { &*context.cast::<Wake>() }.0)();
}

fn error_message(error: &[c_char]) -> io::Error {
    // SAFETY: the zero-initialized buffer is always terminated by snprintf.
    io::Error::other(
        unsafe { CStr::from_ptr(error.as_ptr()) }
            .to_string_lossy()
            .into_owned(),
    )
}

impl ApplicationMenu {
    /// Attach on the AppKit main thread. `wake` must not panic or re-enter menu management.
    pub fn attach(model: &MacMenuModel, repaint: impl Fn() + 'static) -> io::Result<Self> {
        let json = model.encode()?;
        let mut wake_context = Box::new(Wake(Box::new(repaint)));
        let mut handle = std::ptr::null_mut();
        let mut error = [0; 512];
        // SAFETY: all buffers and the boxed callback live through the call and menu lifetime.
        let result = unsafe {
            thinoda_menu_attach(
                json.as_ptr(),
                wake,
                (&mut *wake_context as *mut Wake).cast(),
                &mut handle,
                error.as_mut_ptr(),
                error.len(),
            )
        };
        if result < 0 {
            return Err(error_message(&error));
        }
        let handle = NonNull::new(handle)
            .ok_or_else(|| io::Error::other("AppKit returned no menu owner"))?;
        Ok(Self {
            handle,
            _wake: wake_context,
            _main_thread: PhantomData,
        })
    }

    /// Returns false while tracking or when a queued command still owns the old model.
    pub fn replace(&mut self, model: &MacMenuModel) -> io::Result<bool> {
        let json = model.encode()?;
        let mut error = [0; 512];
        // SAFETY: owned handle, main-thread affinity and bounded error buffer.
        let result = unsafe {
            thinoda_menu_replace(
                self.handle.as_ptr(),
                json.as_ptr(),
                error.as_mut_ptr(),
                error.len(),
            )
        };
        if result < 0 {
            Err(error_message(&error))
        } else {
            Ok(result == 1)
        }
    }

    pub fn drain(&self) -> Vec<u16> {
        let mut result = Vec::new();
        loop {
            let mut batch = [0; 64];
            // SAFETY: live owner and a buffer with the declared capacity.
            let count = unsafe {
                thinoda_menu_drain(self.handle.as_ptr(), batch.as_mut_ptr(), batch.len())
            };
            result.extend_from_slice(&batch[..count]);
            if count < batch.len() {
                return result;
            }
        }
    }
}

impl Drop for ApplicationMenu {
    fn drop(&mut self) {
        // SAFETY: native callback is disabled before the stable wake Box is dropped.
        unsafe { thinoda_menu_detach(self.handle.as_ptr()) };
    }
}
