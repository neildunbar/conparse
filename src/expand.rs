extern crate posix;
extern crate regex;

use self::regex::Regex;
use std::old_io::{IoResult,IoErrorKind,IoError};
use std::os;

//use self::posix::AsNTStr;
//use std::ffi;

pub fn foo() {
    println!("foo")
}

// can't seem to find any sort of expanduser type affair
// so crafting this temporary one for unix style systems
// it's just an clone() function for non-unix.
#[cfg(all(unix))]

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
                        _ => {

                            // disabled for the moment - all the
                            // unsafe stuff needs more work.


                        //uname => {
                            //let mut pwbuf = [0u8;4096];
                            //let mut res : usize = 0;
                            //let mut pwd = posix::pwd::passwd::new();

                            // just guessing at 4K max - routine will
                            // error if bound would be violated. In
                            // theory a retry-with-doubling would
                            // work, but too much hassle right now
                            //posix::pwd::getpwnam_r(uname.as_nt_str(), &mut pwd, &mut pwbuf, &mut res);
                            //if res == 0 {
                            //    let bytes = unsafe { ffi::c_str_to_bytes(&pwd.pw_dir as &*const i8) };
                            //    Path::new(bytes)
                            //} else {
                                Path::new("/")
                            //}
                        }
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
    use std::os;
    use expand::*;

    #[test]
    fn test_expand_homedir() {
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
    }
}

