use std::ffi::{c_char, c_void, CString};

#[link(name = "Foundation", kind = "framework")]
#[link(name = "AppKit", kind = "framework")]
extern "C" {}

fn main() {
    unsafe {
        extern "C" {
            fn objc_getClass(name: *const c_char) -> *mut c_void;
            fn sel_registerName(name: *const c_char) -> *mut c_void;
        }

        type MsgSendId = unsafe extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void;
        type MsgSendCStrId = unsafe extern "C" fn(*mut c_void, *mut c_void, *const c_char) -> *mut c_void;
        type MsgSendIdId = unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void) -> *mut c_void;
        type MsgSendIdVoid = unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void);

        let objc_msgSend = dlsym::objc_msgSend_ptr();

        let msg_send_id: MsgSendId = std::mem::transmute(objc_msgSend);
        let msg_send_id_id: MsgSendIdId = std::mem::transmute(objc_msgSend);
        let msg_send_cstr_id: MsgSendCStrId = std::mem::transmute(objc_msgSend);
        let msg_send_id_void: MsgSendIdVoid = std::mem::transmute(objc_msgSend);

        let nsstring_class = objc_getClass(CString::new("NSString").unwrap().as_ptr());
        let string_with_utf8_sel = sel_registerName(CString::new("stringWithUTF8String:").unwrap().as_ptr());
        let display_name_cstr = CString::new("Hawk").unwrap();
        let app_name = msg_send_cstr_id(nsstring_class, string_with_utf8_sel, display_name_cstr.as_ptr());

        let app_class = objc_getClass(CString::new("NSApplication").unwrap().as_ptr());
        let shared_app_sel = sel_registerName(CString::new("sharedApplication").unwrap().as_ptr());
        let app = msg_send_id(app_class, shared_app_sel);

        // This requires dynamically setting the main menu before NSApplication runs, which GPUI does later.
    }
}

mod dlsym {
    use std::ffi::c_void;
    extern "C" {
        pub fn dlsym(handle: *mut c_void, symbol: *const std::ffi::c_char) -> *mut c_void;
    }
    pub const RTLD_DEFAULT: *mut c_void = -2isize as *mut c_void;

    pub unsafe fn objc_msgSend_ptr() -> *mut c_void {
        let name = std::ffi::CString::new("objc_msgSend").unwrap();
        dlsym(RTLD_DEFAULT, name.as_ptr())
    }
}
