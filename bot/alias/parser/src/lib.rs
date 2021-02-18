use libc::c_char;
use std::default::Default;
use std::ffi::{CStr, CString};

mod parser;

const PREFIX: &str = "g!alias";

#[repr(C)]
pub struct ParseResult {
    pub ok: bool,
    pub data_available: bool,
    pub data: ParseData,
    pub error_msg: *const c_char,
}

impl Default for ParseResult {
    fn default() -> Self {
        ParseResult {
            ok: false,
            data_available: false,
            data: ParseData {
                prefix: 0 as _,
                sub_command: 0 as _,
                args: 0 as _,
                args_length: 0,
            },
            error_msg: 0 as _,
        }
    }
}

#[repr(C)]
pub struct ParseData {
    pub prefix: *const c_char,
    pub sub_command: *const c_char,
    pub args: *const (), // pointer to Box<Vec<CString>>
    pub args_length: u32,
}

#[no_mangle]
pub unsafe extern "C" fn free_parse_result(result: ParseResult) {
    let tryfree_cstr = |x: *mut i8| {
        if x != 0 as _ {
            CString::from_raw(x as *mut i8);
        }
    };

    tryfree_cstr(result.error_msg as _);
    tryfree_cstr(result.data.prefix as _);
    tryfree_cstr(result.data.sub_command as _);

    if result.data.args != 0 as _ {
        Box::from_raw(result.data.args as *mut Vec<CString>);
    }
}

/// ptr must be the pointer of parse function returned
/// or cause undefined behavior.
#[no_mangle]
pub unsafe extern "C" fn args_get_at(ptr: *const (), pos: usize) -> *const c_char {
    let ptr: *mut Vec<CString> = ptr as _;
    let slice = Box::from_raw(ptr);

    let result = match slice.get(pos) {
        Some(e) => e.as_ptr(),
        None => 0 as _,
    };

    std::mem::forget(slice);
    result
}

#[no_mangle]
pub extern "C" fn parse(text_raw: *const c_char) -> ParseResult {
    let text = unsafe { CStr::from_ptr(text_raw) }.to_str();

    if let Err(e) = text {
        eprintln!("failed to construct str from c_char: {}", e);
        return ParseResult::default();
    }

    let cstring = |x| CString::new(x).unwrap().into_raw() as *const c_char;
    let result = parser::parse(text.unwrap());

    match result {
        Ok(Some(data)) => {
            let prefix = cstring(data.prefix);
            let sub_command = data.sub_command.map(cstring).unwrap_or(0 as _);

            let args = data
                .args
                .iter()
                .map(|x| CString::new(*x).unwrap())
                .collect::<Vec<_>>();

            let args: Box<Vec<CString>> = Box::new(args);
            let args_length: u32 = args.len() as _;
            println!("len: {}", args_length);
            let args_ptr: *const Vec<CString> = Box::into_raw(args);

            ParseResult {
                ok: true,
                data_available: true,
                data: ParseData {
                    prefix,
                    sub_command,
                    args: args_ptr as *const (),
                    args_length,
                },
                error_msg: 0 as _,
            }
        }

        Ok(None) => ParseResult {
            ok: true,
            data_available: false,
            ..ParseResult::default()
        },

        Err(msg) => ParseResult {
            ok: false,
            data_available: false,
            error_msg: cstring(&msg),
            ..ParseResult::default()
        },
    }
}
