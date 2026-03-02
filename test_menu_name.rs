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
        
        let objc_msgSend: *const () = std::mem::transmute(objc_getClass as *const ()); // fake

        let msg_send_id: MsgSendId = std::mem::transmute(objc_msgSend);
        let msg_send_id_id: MsgSendIdId = std::mem::transmute(objc_msgSend);
        let msg_send_cstr_id: MsgSendCStrId = std::mem::transmute(objc_msgSend);
        
        // Let's check compilation
    }
}
