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

pub fn get_homedir(uname : &str) -> String {
    let mut pwbuf = [0u8;4096];
    let mut res : usize = 0;
    let mut pwd = posix::pwd::passwd::new();
    let rv = posix::pwd::getpwnam_r(&uname.to_nt_str(), &mut pwd, &mut pwbuf, &mut res);
    let pw = pwd.pw_dir as *const _;
    let hd = unsafe{ ffi::c_str_to_bytes(&pw) };

    match str::from_utf8(hd) {
        Ok(hd_str) => {
            if rv == 0 {
                info!("Fetched homedir of {} as {}", uname, hd_str);
                hd_str.to_string()
            } else {
                warn!("getpwnam_r for \"{}\" returns error code: {}", uname, rv);
                "/".to_string()
            }
        },
        Err(e) => {
            warn!("pw_dir for \"{}\" is invalid UTF-8: {:?}", uname, e);
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
        
        // danger - assuming root home dir is /root
        let rp = Path::new("~root/foo.txt");
        let erp = Path::new("/root/foo.txt");
        match expand_homedir(&rp) {
            Ok(ep) => assert_eq!(ep, erp),
            Err(_) => assert!(false)
        }

    }
}

