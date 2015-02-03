extern crate posix;
extern crate regex;

use self::regex::Regex;
use std::old_io::{IoResult,IoErrorKind,IoError};
use std::os;
use std::ffi;
use std::str;
use self::posix::ToNTStr;

// can't seem to find any sort of expanduser type affair
// so crafting this temporary one for unix style systems
// it's just an clone() function for non-unix.
#[cfg(all(unix))]

// A more rust like wrapper around getpwnam_r
// because the pointer-fu was doing my head in.
pub struct Pwd {
    pub pw_name : String,
    pub pw_passwd : String,
    pub pw_uid : usize,
    pub pw_gid : usize,
    pub pw_gecos : String,
    pub pw_dir : String,
    pub pw_shell : String
}

// utility fn to cast a UTF-8 error into a generic IoError
fn utf8_error(s : &str) -> IoError {
    IoError{kind: IoErrorKind::OtherIoError,
            desc: "Invalid UTF-8 parsing",
            detail: Some(format!("Unable to parse field {}", s).to_string())}
}

pub fn getpwnam(uname : &str) -> IoResult<Pwd> {
    let mut result = Pwd {
        pw_name : String::new(), pw_passwd : String::new(),
        pw_uid : 0, pw_gid : 0, pw_gecos : String::new(),
        pw_dir : String::new(), pw_shell : String::new()
    };

    // NB: There is a bug in RHEL at least, where the ERANGE result
    // for a too short buffer is not returned, therefore this doubling
    // of buffer size may not work on RHEL/CentOS 7
    // This is RHEL bug 1099235; CentOS bug 7324.
    // Validated that it works OK on Ubuntu 14.04

    let mut pwbuf = vec![0u8;128];
    let mut res : usize = 0;
    let mut pwd = posix::pwd::passwd::new();
    loop {
        let rv = posix::pwd::getpwnam_r(&uname.to_nt_str(), &mut pwd, &mut pwbuf.as_mut_slice(), &mut res);

        if rv == 0 {
            break; // successful return
        } else if rv == posix::errno::ERANGE {
            let bsize = pwbuf.capacity() * 2;
            pwbuf.resize(bsize, 0u8);
            warn!("buffer size for getpwnam_r too small. Doubling to {}", pwbuf.capacity());
        } else {
            return Err(IoError::from_errno(rv as usize, true))
        }
    }

    result.pw_uid = pwd.pw_uid as usize;
    result.pw_gid = pwd.pw_gid as usize;

    // copy the string fields

    let pw = pwd.pw_name as *const _;
    let hd = unsafe{ ffi::c_str_to_bytes(&pw) };
    match str::from_utf8(hd) {
        Ok(hd_str) =>  result.pw_name = String::from_str(hd_str),
        Err(_) => return Err(utf8_error("pw_name"))
    }

    let pw = pwd.pw_passwd as *const _;
    let hd = unsafe{ ffi::c_str_to_bytes(&pw) };
    match str::from_utf8(hd) {
        Ok(hd_str) =>  result.pw_passwd = String::from_str(hd_str),
        Err(_) => return Err(utf8_error("pw_passwd"))
    }

    let pw = pwd.pw_gecos as *const _;
    let hd = unsafe{ ffi::c_str_to_bytes(&pw) };
    match str::from_utf8(hd) {
        Ok(hd_str) =>  result.pw_gecos = String::from_str(hd_str),
        Err(_) => return Err(utf8_error("pw_gecos"))
    }

    let pw = pwd.pw_dir as *const _;
    let hd = unsafe{ ffi::c_str_to_bytes(&pw) };
    match str::from_utf8(hd) {
        Ok(hd_str) =>  result.pw_dir = String::from_str(hd_str),
        Err(_) => return Err(utf8_error("pw_dir"))
    }

    let pw = pwd.pw_shell as *const _;
    let hd = unsafe{ ffi::c_str_to_bytes(&pw) };
    match str::from_utf8(hd) {
        Ok(hd_str) =>  result.pw_shell = String::from_str(hd_str),
        Err(_) => return Err(utf8_error("pw_shell"))
    }

    Ok(result)
}

pub fn get_homedir(uname : &str) -> String {
    match getpwnam(uname) {
        Ok(pwd) => pwd.pw_dir,
        Err(e) => {
            warn!("Unable to retrieve pwd details for {} : {}", uname, e);
            "/".to_string()
        }
    }
}

pub fn expand_homedir(p : &Path) -> IoResult<Path> {
    let u_re = match Regex::new(r"^\s*~(\w*)/(.*)$") {
        Err(_) => return Err(IoError { kind : IoErrorKind::OtherIoError,
                                       desc : "Regular expression for homedir does not compile",
                                       detail : None}),
        Ok(r) => r
    };
    
    let ps = match p.as_str() {
        Some(s) => s,
        None => ""
    };
    
    if ps == "" {
        return Err(IoError { kind : IoErrorKind::OtherIoError,
                             desc : "Unable to extract path as string",
                             detail : None})   
    }
    
    match u_re.captures(ps) {
        Some(c) => {
            match c.at(1) {
                Some(u) => {
                    let mut rp = match u {
                        "" =>  match os::homedir() {
                            Some(h) => Path::new(h),
                            None => Path::new("/") // no home dir -
                                // assume root
                        },
                        uname => Path::new(get_homedir(uname))
                    };
                    
                    match c.at(2) {
                        Some(rem) => {
                            rp.push(rem);
                            Ok(rp.clone())
                        },
                        None => {
                            warn!("Cannot get second capture group from regex match");
                            Err(IoError { kind : IoErrorKind::OtherIoError,
                                          desc : "Regular expression path capture failed",
                                          detail : None})
                        }
                    }
                },
                None => {
                    warn!("Unable to fetch username from capture group");
                    Err(IoError { kind : IoErrorKind::OtherIoError,
                                  desc : "Regular expression username capture failed",
                                  detail : None})
                }
            }
        },
        None => Ok(p.clone()) // no home dir to expand
    }
}

#[cfg(not(unix))]

pub fn expand_homedir(p : &Path) -> IoResult<Path> {
    Ok(p.clone())
}

#[cfg(test)]
#[cfg(all(unix))]

mod test {
    extern crate env_logger;

    use std::os;
    use expand::*;

    #[test]
    fn test_expand_homedir() {
        // env_logger::init().unwrap();

        let homedir = match os::homedir() {
            Some(h) => h,
            None => { Path::new("bound-to-fail")}
        };
        let mut expected = Path::new(homedir);
        expected.push("foo.txt");
        let p = Path::new("~/foo.txt");
        match expand_homedir(&p) {
            Ok(ep) => assert_eq!(ep, expected),
            Err(_) => assert!(false)
        }
        
        // danger - assuming root home dir is /root - this test could
        // fail on some platforms (well, many, actually)
        // would prefer to do a mock here for getpwnam().
        let rp = Path::new("~root/foo.txt");
        let erp = Path::new("/root/foo.txt");
        match expand_homedir(&rp) {
            Ok(ep) => assert_eq!(ep, erp),
            Err(_) => assert!(false)
        }

    }
}

