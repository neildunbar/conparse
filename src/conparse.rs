extern crate regex;
extern crate core;

use self::regex::{Regex,Captures};
use self::core::num::{ParseIntError,ParseFloatError};

use std::collections::{HashMap,HashSet};
use std::collections::hash_map::{Keys,Iter,Entry};
use std::error::Error;
use std::fmt::{Display,Formatter,Debug};
use std::fmt;
use std::string::String;
use std::old_io::{Open,IoError,ReadWrite,MemWriter,MemReader,
                  BufferedReader,IoResult,IoErrorKind,File,standard_error};
use std::ascii::OwnedAsciiExt;
use std::str::FromStr;
use expand::expand_homedir;
use std::env;


pub struct InterpString {
    raw_string : String
    // maybe some fields for caching interpolated values?
}

pub type Props = HashMap<String, InterpString>;

/// A structure for storing INI style key,value pairs
/// within a set of named sections
pub struct ConfigParser {
    /// defaults - set of default values provided at construction time
    defaults: HashMap<String, String>,
    /// sections - set of mappings from Strings to HashMaps. Each
    /// internal HashMap is a mapping from a String (key name) to
    /// another String (the value of the option)
    
    sections: HashMap<String, Props>,
    s_re : Regex, // [ section ] regex
    o_re : Regex, // option key : value regex
    i_re : Regex // %(option)s interpolation regex
}

#[derive(Debug,Copy,PartialEq,Eq,Clone)]
pub enum FetchErrorKind {
    /// A requested section does not exist
    NoSuchSection,
    /// A requested option does not exit
    NoSuchOption,
    /// An attempt was made to add a section which already exists
    DuplicateSection,
    /// An interpolation refers to an option which does not exist
    InterpolationError,
    /// An interpolation chain is circular
    InterpolationCircularity,
    /// An attempt was made to translate an invalid string to another type
    InvalidLiteral
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct FetchError {
    kind: FetchErrorKind,
    description: &'static str,
    detail: Option<String>
}

impl Error for FetchError {
    fn description(&self) -> &str {
        self.description
    }
}

impl FetchError {
    pub fn new(k : FetchErrorKind, desc: &'static str, details : Option<String>) -> FetchError {
        FetchError{ kind : k, description : desc, detail : details }
    }

    pub fn kind(&self) -> FetchErrorKind{
        self.kind
    }

    pub fn detail(&self) -> Option<String> {
        self.detail.clone()
    }
}

fn fe_error(k : FetchErrorKind) -> FetchError {
    match k {
        FetchErrorKind::NoSuchSection => FetchError::new(k, "No such configuration section", None),
        FetchErrorKind::NoSuchOption => FetchError::new(k, "No such configuration option", None),
        FetchErrorKind::DuplicateSection => FetchError::new(k, "Section already exists", None),
        FetchErrorKind::InterpolationError => FetchError::new(k, "Interpolation into option failed", None),
        FetchErrorKind::InterpolationCircularity => FetchError::new(k, "Interpolation is infinitely recursive", None),
        FetchErrorKind::InvalidLiteral => FetchError::new(k, "Value cannot be parsed into desired type", None),
    }
}

impl Display for FetchError {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match *self {
            FetchError{ detail: None, description, ..} =>
                write!(f, "{}", description),
            FetchError{ detail: Some(ref details), description, ..} =>
                write!(f, "{} ({})", description, details)
        }
    }
}

impl Display for InterpString {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "{}", self.get_raw())
    }
}

impl Debug for InterpString {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "{:?}", self.get_raw())
    }
}

impl PartialEq for InterpString {
    #[inline]
    fn eq(&self, other: &InterpString) -> bool { PartialEq::eq(&self.get_raw(), &other.get_raw()) }
    #[inline]
    fn ne(&self, other: &InterpString) -> bool { PartialEq::ne(&self.get_raw(), &other.get_raw()) }
}

impl InterpString {
    pub fn new(s: &str) -> InterpString {
        InterpString{ raw_string : s.to_string() }
    }

    pub fn set(&mut self, s: &str) {
        self.raw_string = s.to_string();
    }

    pub fn get_raw(&self) -> String {
        self.raw_string.clone()
    }
    
    fn expand_one(&self, oname : &str, text : &str,
                  res : &String,
                  sec : &str, option : &str,
                  cp : &ConfigParser,
                  expanded : &mut HashSet<String>) -> Result<String, FetchError> {
        for s in expanded.iter() {
            debug!("expanded contains \"{}\"", s)
        }
        if oname == option || expanded.contains(oname) {
            warn!("Option {} has already been expanded or circular definition?", oname);
            return Err(fe_error(FetchErrorKind::InterpolationCircularity))
        }
        
        info!("Inserting {} into expanded set", oname);
        expanded.insert(oname.to_string());
        
        match cp.get_interp(sec, oname, expanded) {
            Ok(v) => Ok(res.replace(text, v.as_slice())),
            Err(e) => {
                warn!("Error in lookup for interpolation of {}:{}: {:?}",
                      sec, oname, e);
                
                Err(if e.kind() == FetchErrorKind::InterpolationCircularity {e}
                    else {fe_error(FetchErrorKind::InterpolationError)})
            }
        }
    }

    /// Interpolate any values in the string via the
    /// options inside the specified section
    pub fn get(&self, sec : &str, option : &str, cp : &ConfigParser,
               expanded : &mut HashSet<String>) -> Result<String, FetchError> {
        let mut res = self.raw_string.clone();

        loop {
            let mut done_cap = false;
            loop {
                let mut repl = res.clone();
                match cp.i_re.captures(res.as_slice()) {
                    Some(cap) => {
                        match cap.at(1) {
                            Some(t) => {
                                match cap.at(2) {
                                    Some(oname) => {
                                        match self.expand_one(oname, t,
                                                              &res,
                                                              sec,
                                                              option,
                                                              cp,
                                                              expanded) {
                                            Ok(v) => {
                                                repl = v;
                                                done_cap = true;
                                            },
                                            Err(e) => {return Err(e);}
                                        }
                                    },
                                    None => {
                                        warn!("Capture for interpolation option \
                                               found, but no matching text!");
                                    }
                                }
                            },
                            None => {
                                warn!("Capture for interpolation found, \
                                       but no matching text!");
                            } // shouldn't really happen though
                        }
                    },
                    None => {
                        break; // no more captures
                    }
                }
                
                res = repl; // replace text, ready to try again
            }

            // if we didn't do any replacements, then we exit
            if ! done_cap {
                break; // no captures to substitute
            }
        }
        Ok(res)
    }
}

pub trait ContinuationReader {
    fn read_continued_line(&mut self) -> IoResult<String>;
}

impl<T:Buffer> ContinuationReader for T {
    fn read_continued_line(&mut self) -> IoResult<String> {
        let mut result_line: String = "".to_string();
        let mut continuing = false;
        loop {
            match self.read_line() {
                Ok(l) => {
                    let tr = l.as_slice();
                    debug!("Read line: {}", tr.trim_right());
                    let ll = tr.len();

                    if ll == 0 {
                        debug!("Read line with no newline");
                        break;
                    }

                    if tr.starts_with("#") || tr.starts_with(";") {
                        // ignore comment lines
                        continue;
                    }

                    // check for line which doesn't end with newline
                    // (e.g. at end of file
                    if ! tr.ends_with("\n") {
                        let mut trl = tr;
                        if tr.ends_with("\\") {
                            // must be end of file, ending on continuation
                            // (yuck) - signal end of line, and ignore any
                            // data to this point
                            return Err(standard_error(IoErrorKind::EndOfFile))
                        }
                        if continuing {
                            trl = trl.trim_left();
                        }
                        result_line.push_str(trl);
                        break; 
                    }

                    if tr.ends_with("\\\n") {
                        let mut trl = tr[..ll-2].to_string();
                        if continuing {
                            trl = trl.trim_left().to_string();
                            // strip leading ws
                            // from continuing line
                        } else {
                            continuing = true;
                        }
                        result_line.push_str(trl.as_slice());
                    } else {
                        // no continuation - return line
                        let mut trl = tr[..ll-1].to_string();
                        if continuing {
                            trl = trl.trim_left().to_string();
                            // strip leading ws
                            // from continuing line
                        }

                        result_line.push_str(trl.as_slice());
                        break;
                    }
                },
                Err(e) => {
                    debug!("Error pushed: {:?}", e);
                    return Err(e)
                }
            }
        };
        // re-add a newline
        result_line.push('\n');
        debug!("Returning line: {}", result_line.trim_right());
        Ok(result_line)
    }
}

// this should be returning Option<(String,Option<String>)> to cover
// the case where the line is a simple "key" (without a '=' or ':')
// value to the right of it. For the moment, if we get a key without
// an attendant value, we set the value to an empty string, but this
// isn't really the appropriate thing to do.
fn get_captured_kv(c : regex::Captures) -> Option<(String,String)> {
    if c.len() < 2 {
        return None
    }
    debug!("Capture 1: {}",
          match c.at(1) {
              Some(t) => t,
              _ => "None"
          });

    debug!("Capture 2: {}",
          match c.at(2) {
              Some(t) => t,
              _ => "None"
          });

    debug!("Capture 3: {}",
          match c.at(3) {
              Some(t) => t,
              _ => "None"
          });

    match c.at(1) {
        Some(key) => match c.at(3) {
            Some(val) => Some((key.to_string(), val.to_string())),
            _ => Some((key.to_string(), String::from_str(""))),
        },
        _ => None,
    }
}

fn try_option_kv (cp : &mut ConfigParser, tl : &str, curr_sect : &String) {
    match cp.option_kv(tl) {
        Some((opt,val)) => {
            if curr_sect.is_empty() {
                warn!("Attempting to set option [{}, {}] outside of section - ignoring", opt, val);
            } else {
                let s = cp.sections.get_mut(curr_sect);

                match s {
                    Some(ohash) => {
                        ohash.insert(opt, InterpString::new(val.as_slice()));
                    },
                    None => {
                        error!("Should not get this - \
                                current section {} does not exist. Ignoring", curr_sect);
                    }
                }
            }
        },
        None => {} // do nothing
    }
}

fn from_reader_helper<T: ContinuationReader>(cp : &mut ConfigParser, r : &mut T) {
    let mut curr_sect = "".to_string();

    loop {
        match r.read_continued_line() {
            Ok(l) => {
                let tl = l.trim_right();
                match cp.section_name(tl) {
                    Some(s) => {
                        curr_sect = s.to_string();
                        if cp.sections.contains_key(s.as_slice()) {
                            continue
                        } // ignore repeat section
                        let p : HashMap<String, InterpString> = HashMap::new();
                        cp.sections.insert(s, p);
                    },
                    None => {
                        try_option_kv(cp, tl, &curr_sect);
                    }
                    
                }
            },
            Err(e) => {
                match e.kind {
                    IoErrorKind::EndOfFile => {
                    },
                    _ => {
                        error!("Reader error on parser init: {:?}", e)
                    }
                }
                break;
            }
        }
    }
}

fn abspath(p: &Path) -> IoResult<Path> {
    match p.is_absolute() {
        true => Ok(p.clone()),
        _ => match env::current_dir() {
            Ok(mut cwd) => {cwd.push(p); Ok(cwd)},
            Err(e) => Err(e)
        }
    }
}

impl ConfigParser {
    ///
    /// Creates an empty ConfigParser with default key,value pairs
    ///
    /// # Example
    ///
    /// ```
    /// use conparse::conparse::ConfigParser;
    ///
    /// let mut cp = ConfigParser::new(&[("host","localhost"), ("port","22"), ("protocol","tcp")]);
    /// ```
    ///
    pub fn new(kvdefaults : &[(&str, &str)]) -> ConfigParser {
        let mut df = HashMap::new();
        for &(k,v) in kvdefaults.iter() {
            df.insert(k.to_string(), v.to_string());
        }
        // make these regex macros once it's not experimental
        // unwrap() in init code == teh suck
        let sect_re = Regex::new(r"^\[\s*(\w+)\s*\](\s*[#;].*)?$").unwrap();
        let option_re = Regex::new(r"^(\w+)(\s*[:=]\s*(.*))?$").unwrap();
        let interp_re = Regex::new(r"(%\(\s*(\w+)\s*\)s)").unwrap();
        let sects : HashMap<String, Props> = HashMap::new();
        ConfigParser { defaults: df, sections : sects,
                       s_re: sect_re, o_re : option_re, i_re : interp_re }
    }

    //
    // Strongly suspect (ie, know) that there's way too much
    // mutability in here. Most of the time I just want to pass
    // a slice of readers into a function. The readers will be
    // mutated, but the slice itself should not be, so passing as
    // &mut[ &mut T ] seems wrong.
    //

    ///
    /// Create a new ConfigParser from a slice of `Reader`s which can
    /// implement the `ContinuationReader` trait. In theory you could
    /// pass a slice of ContinuationReader trait objects, but I need
    /// to find out how to make them implement the Sized trait
    ///
    /// # Example
    ///
    /// ```
    /// #![feature(path)]
    /// #![feature(io)]
    /// use conparse::conparse::{ConfigParser};
    /// use std::old_io::{BufferedReader,File};
    /// use std::old_path::Path;
    ///
    /// fn open_files(fs : &[ &str ]) -> Vec<BufferedReader<File>> {
    ///     fs.iter().filter_map(|&p| match File::open(&Path::new(p)) {
    ///         Ok(f) => Some(BufferedReader::new(f)),
    ///         Err(_) => None
    ///     }).collect()
    /// }
    ///
    /// let mut of = open_files(&["/etc/myapp/myconfig.txt",
    ///                           "/usr/local/etc/myapp/extra-config.txt"]);
    /// let mut mof : Vec<&mut BufferedReader<File>> = of.iter_mut().collect();
    /// let cp = ConfigParser::from_readers(mof.as_mut_slice(), &[("host", "localhost")]);
    /// ```
    ///
    pub fn from_readers<T: ContinuationReader>(rs : &mut[ &mut T ],
                                               kvdefaults : &[(&str, &str)]) -> ConfigParser {
        let mut cp = ConfigParser::new(kvdefaults);
        for r in rs.iter_mut() {
            from_reader_helper(&mut cp, *r)
        }
        cp
    }

    ///
    /// Create a new ConfigParser from a string specification
    ///
    /// # Example
    ///
    /// ```
    /// use conparse::conparse::ConfigParser;
    ///
    /// let cp = ConfigParser::from_str(
    ///          "[myapp]\n log_level = DEBUG", &[("log_level","WARN")]);
    /// ```
    ///
    pub fn from_str(s: &str, kvdefaults : &[(&str, &str)]) -> ConfigParser {
        let mut v = MemReader::new(s.as_bytes().to_vec());
        ConfigParser::from_readers(&mut[&mut v], kvdefaults)
    }

    ///
    /// Create a new ConfigParser from a list of string contents
    ///
    /// # Example
    ///
    /// ```
    /// use conparse::conparse::ConfigParser;
    ///
    /// let cp = ConfigParser::from_strs(&["[myapp]\n log_level = DEBUG",
    ///                 "[global]\ngreeting = Hello\n"], &[("log_level","INFO")]);
    /// ```
    ///
    pub fn from_strs(ss: &[ &str ], kvdefaults : &[(&str, &str)]) -> ConfigParser {

        let mut v = vec![];
        for s in ss.iter() {
            v.push(MemReader::new(s.as_bytes().to_vec()));
        }
        let mut v1 : Vec<&mut MemReader> = v.iter_mut().collect();
        ConfigParser::from_readers(v1.as_mut_slice(), kvdefaults)
    }


    ///
    /// Create a new ConfigParser from reading a list of files
    ///
    /// # Example
    ///
    /// ```
    /// use conparse::conparse::ConfigParser;
    ///
    /// let cp = ConfigParser::from_files(&["/etc/myapp/config.txt",
    ///                       "~/.myapp.cfg"], &[("log_level","INFO")]);
    /// ```
    ///
    pub fn from_files(ss : &[ &str ], kvdefaults : &[(&str, &str)]) -> ConfigParser {
        let mut v = vec![];
        for s in ss.iter() {
            let p = Path::new(*s);
            let exp_p = match expand_homedir(&p) {
                Ok(ep) => ep,
                Err(e) => {
                    error!("Cannot expand user homedir of {} : {}", p.display(), e);
                    p.clone()
                }
            };
            let abs_p = match abspath(&exp_p) {
                Ok(ap) => ap,
                Err(e) => {
                    error!("Cannot make absolute directory of {} : {}", p.display(), e);
                    exp_p.clone()
                }
            };

            match File::open(&abs_p) {
                Ok(f) => {
                    v.push(BufferedReader::new(f))
                },
                Err(e) => {
                    error!("Cannot open path {} for config: {:?}", *s, e);
                }
            }
        }
        let mut v1 : Vec<&mut BufferedReader<File>>  = v.iter_mut().collect();
        
        ConfigParser::from_readers(v1.as_mut_slice(), kvdefaults)
    }

    ///
    /// Create a new ConfigParser from reading a file
    ///
    /// # Example
    ///
    /// ```
    /// use conparse::conparse::ConfigParser;
    ///
    /// let cp = ConfigParser::from_file("/etc/myapp/config.txt", &[("log_level","INFO")]);
    /// ```
    ///
    pub fn from_file(s : &str, kvdefaults : &[(&str, &str)]) -> ConfigParser {
        ConfigParser::from_files(&[ s ], kvdefaults)
    }

    pub fn to_writer(&self, w: &mut Writer) -> IoResult<()> {
        let mut ss : Vec<&String> = self.sections().collect();
        ss.sort();

        for s in ss.iter() {
            match write!(w, "[{}]\n", s) {
                Ok(_) => {} // continue
                Err(_) =>
                    return Err(
                        IoError { 
                            kind: IoErrorKind::ResourceUnavailable,
                            desc: "Internal ConfigParser write error",
                            detail:
                            Some("Internal ConfigParser error: \
                                  section not found during writing section"
                                 .to_string())})
            }
            match self.options(s.as_slice()) {
                Ok(o_raw) => {
                    // want to sort the options
                    let mut o : Vec<(&String,&InterpString)> = o_raw.collect();
                    o.sort_by(|&(k1,_), &(k2,_)| k1.cmp(k2));

                    for &(k,v) in o.iter() {
                        match write!(w, "{} : {}\n", k, v) {
                            Ok(_) => {},
                            Err(_) =>
                                return Err(
                                    IoError {
                                        kind: IoErrorKind::ResourceUnavailable,
                                        desc: "Internal ConfigParser write error",
                                        detail:
                                        Some("Internal ConfigParser error: \
                                              option not found during writing"
                                             .to_string())})
                        }
                    }
                },
                Err(_) =>
                    return Err(IoError { kind: IoErrorKind::ResourceUnavailable,
                                         desc: "Internal ConfigParser write error",
                                         detail:
                                         Some("Internal ConfigParser error: \
                                               unable to find options during writing"
                                              .to_string())})
            }
            // blank line at end of each section
            match write!(w, "\n") {
                Ok(_) => {} // continue
                Err(_) =>
                    return Err(IoError { kind: IoErrorKind::ResourceUnavailable,
                                         desc: "Internal ConfigParser write error",
                                         detail: Some("Internal ConfigParser \
                                                       error during writing"
                                                      .to_string())})
            }            
        }
        Ok(()) // return success unit val
    }

    // convenience method for spitting to file
    pub fn to_file(&self, fpath: &str) -> IoResult<()> {
        let p = Path::new(fpath);
        match File::open_mode(&p, Open, ReadWrite) {
            Ok(mut f) => self.to_writer(&mut f),
            Err(e) => {
                error!("Unable to write to file {} : {}", fpath, e);
                Err(e)
            }
        }
    }

    // convenience method for spitting to a string
    pub fn to_string(&self) -> IoResult<String> {
        let mut w = MemWriter::new();
        match self.to_writer(&mut w) {
            Ok(_) => {
                let s = String::from_utf8(w.into_inner());
                match s {
                    Ok(ret) => Ok(ret),
                    Err(_) =>
                        return Err(
                            IoError {
                                kind: IoErrorKind::ResourceUnavailable,
                                desc: "Internal ConfigParser write error",
                                detail: Some("Internal ConfigParser error during UTF-8 translation"
                                             .to_string())})
                }
            },
            Err(e) => {
                error!("Unable to write to string : {}", e);
                Err(e)
            }
        }
    }

    fn section_name(&self, s: &str) -> Option<String> {
        match self.s_re.captures(s.trim()) {
            Some(c) =>
                match c.at(1) {
                    Some(cs) => Some(cs.to_string()),
                    _ => None
                },
            _ => None
        }
    }


    fn option_kv(&self, s: &str) -> Option<(String,String)> {
        match self.o_re.captures(s.trim()) {
            Some(c) => {
                get_captured_kv(c)},
            _ => None
        }
    }

    ///
    /// Adds an (empty) section to a mutable `ConfigParser` object.
    /// If the section already exists, a
    /// `FetchError::DuplicateSection` error is returned, otherwise
    /// `()` is returned as an `Ok` value.
    ///
    /// # Example
    ///
    /// ```
    /// use conparse::conparse::ConfigParser;
    ///
    /// let mut cp = ConfigParser::new(&[]);
    /// assert!(cp.add_section("foo").is_ok());
    /// assert!(cp.add_section("foo").is_err()); // duplicating section
    /// ```
    pub fn add_section(&mut self, s : &str) -> Result<(), FetchError> {
        let sec = s.to_string();
        match self.sections.entry(sec) {
            Entry::Occupied(_) => Err(fe_error(FetchErrorKind::DuplicateSection)),
            Entry::Vacant(v) => {
                v.insert(HashMap::new());
                Ok(())
            }
        }
    }

    /// Zaps a section from a `ConfigParser`. If the section does not
    /// exist, a `FetchError::NoSuchSection` is returned as error.
    /// Otherwise `()` is returned as an `Ok` value. Note that this
    /// deletes all options contained within the section.
    ///
    /// # Example
    ///
    /// ```
    /// use conparse::conparse::ConfigParser;
    /// 
    /// let mut cp = ConfigParser::from_str("[foo]\n\
    ///    bar = quux\n\
    ///    \n\
    ///    [wibble]\n\
    ///    bar2 = quux2\n", &[]);
    /// assert!(cp.has_section("foo"));
    /// cp.remove_section("foo");
    /// assert!(! cp.has_section("foo"));
    /// assert!(cp.get("foo", "bar").is_err()); // NoSuchSection
    /// ```
    pub fn remove_section(&mut self, s : &str) -> Result<(), FetchError> {
        match self.sections.remove(s) {
            Some(_) => Ok(()),
            None => Err(fe_error(FetchErrorKind::NoSuchSection))
        }
    }

    ///
    /// Returns a boolean as to whether a given section exists or not
    ///
    /// # Example
    /// ```
    /// use conparse::conparse::ConfigParser;
    ///
    /// let cp = ConfigParser::from_str("[foo]\n[bar]\n", &[]);
    /// assert!(cp.has_section("foo"));
    /// assert!(! cp.has_section("quux"));
    /// ```
    pub fn has_section(&self, s : &str) -> bool {
        self.sections.contains_key(s)
    }

    ///
    /// sets an option to a given value, inside a given section
    /// if the section does not exist, it will be created. If the
    /// option within the section already exists, it will be
    /// overwritten. Returns no result.
    ///
    /// # Example
    ///
    /// ```
    /// use conparse::conparse::ConfigParser;
    ///
    /// let mut cp = ConfigParser::new(&[]);
    /// cp.set("mysection", "myoption", "myvalue" );
    /// ```
    ///
    pub fn set(&mut self, section: &str, option: &str, value: &str) -> () {
        match self.sections.entry(section.to_string()) {
            Entry::Occupied(mut o) => {
                o.get_mut().insert(option.to_string(), InterpString::new(value));
            },
            Entry::Vacant(v) => {
                let mut opts = HashMap::new();
                opts.insert(option.to_string(), InterpString::new(value));
                v.insert(opts);
            }
        }
    }

    ///
    /// Deletes an option from a given section
    /// If the option does not exist, `FetchError::NoSuchOption` is returned as error
    /// If the section does not exist, `FetchError::NoSuchSection` is
    /// returned as error
    /// On successful completion, `()` is returned as `Ok` value
    ///
    /// # Example
    ///
    /// ```
    /// use conparse::conparse::ConfigParser;
    ///
    /// let mut cp = ConfigParser::new(&[]);
    ///
    /// cp.set("foosection", "baroption", "quuxval");
    /// assert!(cp.get("foosection", "baroption").is_ok());
    /// cp.remove_option("foosection", "baroption");
    /// assert!(cp.get("foosection", "baroption").is_err());
    /// ```
    pub fn remove_option(&mut self, section : &str, option: &str) -> Result<(),FetchError> {
        match self.sections.get_mut(section) {
            Some(opts) => {
                match opts.remove(option) {
                    Some(_) => Ok(()),
                    None => Err(fe_error(FetchErrorKind::NoSuchOption))
                }
            },
            None => Err(fe_error(FetchErrorKind::NoSuchSection))
        }
    }

    fn get_default(&self, option: &str, fe: FetchErrorKind) -> Result<String, FetchError> {
        match self.defaults.get(option) {
            Some(v) => Ok(v.clone()),
            None => Err(fe_error(fe))
        }
    }

    ///
    /// Fetches a given option from a given section, and returns the
    /// value of the option *without* interpolation. If the section
    /// does not exist `FetchError::NoSuchSection` is returned as
    /// error; if the option does not exist,
    /// `FetchError::NoSuchOption` is returned as error. Otherwise the
    /// raw option is returned as a `String`.
    ///
    /// # Example
    ///
    /// ```
    /// use conparse::conparse::ConfigParser;
    ///
    /// let cp =
    /// ConfigParser::from_str("[foo]\n\
    ///     bar=quux\n\
    ///     bar2=%(bar)s is fun\n", &[]);
    ///
    /// let mut b2 = cp.get("foo", "bar2");
    /// assert!(b2.is_ok() && b2.unwrap().as_slice() == "quux is fun"); // interpolate
    /// let mut b2 = cp.get_raw("foo", "bar2");
    /// assert!(b2.is_ok() && b2.unwrap().as_slice() == "%(bar)s is fun");
    /// // no interpolation with get_raw
    /// ```
    pub fn get_raw(&self, section: &str, option: &str) -> Result<String, FetchError> {
        match self.sections.get(section) {
            Some(opts) => match opts.get(option) {
                Some(v) => Ok(v.get_raw()),
                None => self.get_default(option, FetchErrorKind::NoSuchOption)
            },
            None => self.get_default(option, FetchErrorKind::NoSuchSection)
        }
    }

    ///
    /// Returns true if section `section` contains an option `option`
    /// (or if there is a default option called `option`). If the
    /// section does not exist, `FetchError::NoSuchSection` is
    /// returned as an error.
    ///
    /// # Example
    ///
    /// ```
    /// use conparse::conparse::ConfigParser;
    ///
    /// let cp = ConfigParser::from_str(
    ///      "[foo]\n\
    ///       bar : quux\n", &[]);
    /// let ho = cp.has_option("foo", "bar");
    /// assert!(ho.is_ok() && ho.unwrap() == true);
    /// let ho2 = cp.has_option("foo", "missing");
    /// assert!(ho2.is_ok() && ho2.unwrap() == false);
    /// ```
    pub fn has_option(&self, section: &str, option: &str) -> Result<bool, FetchError> {
        match self.sections.get(section) {
            Some(opts) => Ok(opts.contains_key(option) || self.defaults.contains_key(option)),
            None => Err(fe_error(FetchErrorKind::NoSuchSection))
        }
    }

    fn get_interp(&self, section: &str, option: &str,
                  expanded : &mut HashSet<String>) -> Result<String, FetchError> {
        match self.sections.get(section) {
            Some(opts) => match opts.get(option) {
                Some(v) => v.get(section, option, self, expanded),
                None => self.get_default(option, FetchErrorKind::NoSuchOption)
            },
            None => self.get_default(option, FetchErrorKind::NoSuchSection)
        }
    }

    pub fn get(&self, section: &str, option: &str) -> Result<String, FetchError> {
        let mut expanded : HashSet<String> = HashSet::new();
        self.get_interp(section, option, &mut expanded)
    }

    // Now I wish Rust had default param values - having a boolean
    // 'raw' would be handy here, to avoid the attempt to interpolate.
    pub fn getboolean(&self, section: &str, option: &str) -> Result<bool, FetchError> {
        let trues = vec!["true","yes","on","1",""];
        let falses = vec!["false", "no", "off", "0"];

        // note that empty string counts as true, ie, so we can
        // have things like getboolean("foo", "skip_init") return
        // true if we just have "skip_init" in config string,
        // not necessarily "skip_init = true"
        match self.get(section, option) {
            Err(e) => Err(e),
            Ok(v) => {
                let lv = v.into_ascii_lowercase();
                for &t in trues.iter() {
                    if t == lv {
                        return Ok(true)
                    }
                }
                for &f in falses.iter() {
                    if f == lv {
                        return Ok(false)
                    }
                }
                Err(fe_error(FetchErrorKind::InvalidLiteral))
            }
        }
    }

    pub fn getuint(&self, section: &str, option: &str) -> Result<usize, FetchError> {
        match self.get(section, option) {
            Err(e) => Err(e),
            Ok(v) => {
                if v == "" {
                    // empty string counts as a '1' value
                    Ok(1)
                } else {
                    let m : Result<usize,ParseIntError> = FromStr::from_str(v.as_slice());
                    match m {
                        Ok(u) => Ok(u),
                        Err(_) => Err(fe_error(FetchErrorKind::InvalidLiteral))
                    }
                }
            }
        }
    }

    pub fn getint(&self, section: &str, option: &str) -> Result<isize, FetchError> {
        match self.get(section, option) {
            Err(e) => Err(e),
            Ok(v) => {
                if v == "" {
                    Ok(1)
                } else {
                    let m : Result<isize,ParseIntError> = FromStr::from_str(v.as_slice());
                    match m {
                        Ok(i) => Ok(i),
                        Err(_) => Err(fe_error(FetchErrorKind::InvalidLiteral))
                    }
                }
            }
        }
    }

    pub fn getfloat(&self, section: &str, option: &str) -> Result<f64, FetchError> {
        match self.get(section, option) {
            Err(e) => Err(e),
            Ok(v) => {
                if v == "" {
                    Ok(1.0f64)
                } else {
                    let m : Result<f64,ParseFloatError> = FromStr::from_str(v.as_slice());
                    match m {
                        Ok(i) => Ok(i),
                        Err(_) => Err(fe_error(FetchErrorKind::InvalidLiteral))
                    }
                }
            }
        }
    }

    pub fn sections(&self) -> Keys<String,Props> {
        self.sections.keys()
    }

    pub fn options(&self, section: &str) -> Result<Iter<String,InterpString>, FetchError> {
        match self.sections.get(section) {
            Some(opts) =>  Ok(opts.iter()),
            None=> Err(fe_error(FetchErrorKind::NoSuchSection))
        }
    }
}

#[cfg(test)]

mod test {
    extern crate env_logger;

    use conparse::*;
    use std::old_io::{MemReader,IoErrorKind,TempDir,File,Open,ReadWrite,IoResult};
    use std::str::from_utf8;

    #[test]
    fn check_default() {
        env_logger::init().unwrap();

        let rp = ConfigParser::new(&[( "t1", "v1"), ("t2", "v2")]);
        assert!(rp.defaults.contains_key("t1"));
        assert_eq!(rp.defaults.get("t1").unwrap().as_slice(), "v1");
        assert_eq!(rp.defaults.len(), 2)
    }

    #[test]
    fn set_option() {
        let mut rp = ConfigParser::new(&[( "t1", "v1"), ("t2", "v2")]);
        rp.set("global", "t1", "sv1");
        assert_eq!(rp.get("global", "t1").ok().unwrap(), "sv1");
        assert_eq!(rp.get("global", "t2").ok().unwrap(), "v2");
        let mut r = rp.get("no-section", "t3");
        assert!(r.is_err() && r.err().unwrap().kind() == FetchErrorKind::NoSuchSection);
        r = rp.get("global", "t3");
        assert!(r.is_err() && r.err().unwrap().kind() == FetchErrorKind::NoSuchOption);
    }

    #[test]
    fn read_strings() {
        let tinput = "One \\\n\\\n     Two\n\n#comment \nThree\nFour";
        let mut v = MemReader::new(tinput.as_bytes().to_vec());
        assert!(! v.eof());
        let br = v.read_continued_line();
        assert_eq!(br.unwrap().as_slice().trim(), "One Two");
        let br = v.read_continued_line();
        assert_eq!(br.unwrap().as_slice().trim(), "");
        let br = v.read_continued_line();
        assert_eq!(br.unwrap().as_slice().trim(), "Three");
        let br = v.read_continued_line();
        assert_eq!(br.unwrap().as_slice().trim(), "Four");
        let br = v.read_continued_line();
        assert_eq!(br.err().unwrap().kind, IoErrorKind::EndOfFile);

    }

    #[test]
    fn read_iterated_strings() {
        let cp = ConfigParser::from_strs( &["foo = quux\n  [sec1] \nfoo =  bar",
                                            "[sec2]\nfoo : wibble"], &[]);
        assert!(cp.sections.contains_key("sec1"));
        assert!(cp.sections.contains_key("sec2"));
        let ocs = cp.sections.get("sec1");
        assert!(ocs.is_some());
        let cs = ocs.unwrap();
        assert!(cs.contains_key("foo"));
        let ocv = cs.get("foo");
        assert!(ocv.is_some());
        let cv = ocv.unwrap();
        assert_eq!("bar", cv.get_raw().as_slice());
        let ocv2 = cp.get("sec2", "foo");
        assert!(ocv2.is_ok());
        let cv2 = ocv2.unwrap();
        assert_eq!("wibble", cv2.as_slice());
    }

    #[test]
    fn read_sections() {
        let cp = ConfigParser::from_str("foo = quux\n  [Zulu] \n\
                 foo =  bar\n  [ Alpha ] \nfoo : wibble", &[]);
        let mut ks : Vec<&String> = cp.sections().collect();
        ks.sort();

        assert_eq!(ks, vec![&"Alpha", &"Zulu"]);
    }

    #[test]
    fn read_options() {
        let cp = ConfigParser::from_str(
            "foo = quux\n  \
             [Zulu] \n\
             foo =  bar\n\
             [ Alpha ] \n\
             foo : wibble\n\
             standalone\n\
             \n\
             bar = quux  ", &[]);
        let os = cp.options("NotHere");
        assert!(os.is_err() && os.err().unwrap().kind() == FetchErrorKind::NoSuchSection);
        let os2 = cp.options("Alpha");
        assert!(os2.is_ok());
        assert!(cp.has_option("Alpha", "standalone").unwrap());
        let mut opts : Vec<(&String,&InterpString)> = os2.unwrap().collect();
        opts.sort_by(|&(k1,_),&(k2,_)| k1.cmp(k2));

        let ev = [("bar","quux"), ("foo","wibble"), ("standalone", "")];
        assert_eq!(opts.len(), ev.len());
        for (&(k1,v1),&(k2,v2)) in opts.iter().zip(ev.iter()) {
            assert_eq!(k1.as_slice(),k2);
            assert_eq!(v1.get_raw().as_slice(),v2);
        }
    }

    // utility function to create a temp directory
    // which can be closed afterwards to zap it.

    fn new_tmp_dir() -> IoResult<Box<TempDir>> {
        match TempDir::new("conparse") {
            Ok(dir) => Ok(Box::new(dir)),
            Err(e) => {
                error!("Cannot create temporary directory: {}", e);
                Err(e)
            }
        }
    }

    // utility function to create a temporary directory and a file
    // within it containing the contents of 's', and return the tuple
    // of (TempDir,path-to-file). On closing the temp dir, the
    // contents are deleted.
    
    fn write_file<'a>(s: &'a str, fname: &'a str) -> IoResult<(Box<TempDir>,Path)> {
        let tmpdir = new_tmp_dir().unwrap();
        let mut tmppath = Path::new(tmpdir.path());
        tmppath.push(fname);
        match File::open_mode(&tmppath, Open, ReadWrite) {
            Ok(mut f) => {
                match f.write_str(s) {
                    Ok(_) => {Ok((tmpdir,tmppath))},
                    Err(e) => {error!("Failed to write to file {} : {}", tmppath.display(), e);
                               Err(e)
                    }
                }
            },
            Err(e) => {
                error!("Unable to write to temporary file {} : {}", tmppath.display(), e);
                Err(e)
            }
        }
    }

    #[test]
    fn read_file() {
        let rtp = write_file("foo = quux\n  [Zulu] \n\
             foo =  bar\n  [ Alpha ] \nfoo : wibble", "test_rf.ini");
        assert!(rtp.is_ok());
        let (td,tp) = rtp.unwrap();
        info!("Written config file to {}", tp.display());
        let cp = ConfigParser::from_file(tp.as_str().unwrap(), &[]);
        assert_eq!(cp.get("Zulu", "foo").unwrap(), "bar");
        assert_eq!(cp.get("Alpha", "foo").unwrap(), "wibble");
        assert!(td.close().is_ok());
    }

    #[test]
    fn test_write() {
        let cp = ConfigParser::from_str("foo = quux\n  [Zulu] \nfoo =  bar\n\
                  a_quuxly = barly\n  [ Alpha ] ; alpha section\nfoo : wibble", &[]);

        let mut w = Vec::new();
        match cp.to_writer(&mut w) {
            Ok(_) => {
                let out = from_utf8(w.as_slice()).unwrap();
                assert_eq!(out, "[Alpha]\nfoo : wibble\n\n[Zulu]\na_quuxly : barly\nfoo : bar\n\n")
            },
            Err(_) => assert!(false)
        }
    }

    #[test]
    fn test_read_write_file() {
        let rtp = write_file("foo = quux\n  [Zulu] \nfoo =  bar\n\
                  [ Alpha ] \nfoo : wibble\nquux : wibble2", "test_rw1.ini");
        assert!(rtp.is_ok());
        let (td,tp) = rtp.unwrap();
        info!("Written config file to {}", tp.display());
        let cp1 = ConfigParser::from_file(tp.as_str().unwrap(), &[]);
        let mut newpath = Path::new(td.path());
        newpath.push("test_rw2.ini");
        match cp1.to_file(newpath.as_str().unwrap()) {
            Ok(_) => {info!("Written imported configuration to file {}", newpath.display());},
            Err(_) => {assert!(false)}
        }
        let cp2 = ConfigParser::from_file(newpath.as_str().unwrap(), &[]);
        assert!(td.close().is_ok());
        // now validate that cp1 and cp2 are identical
        let mut sec1 : Vec<&String> = cp1.sections().collect();
        let mut sec2 : Vec<&String> = cp2.sections().collect();
        sec1.sort();
        sec2.sort();
        assert_eq!(sec1, sec2);
        for s in sec1.iter() {
            let mut o1 : Vec<(&String,&InterpString)> =
                cp1.options(s.as_slice()).unwrap().collect();
            let mut o2 : Vec<(&String,&InterpString)> =
                cp2.options(s.as_slice()).unwrap().collect();
            o1.sort_by(|&(k1,_),&(k2,_)| k1.cmp(k2));
            o2.sort_by(|&(k1,_),&(k2,_)| k1.cmp(k2));
            assert_eq!(o1, o2);
        }
    }

    #[test]
    fn test_write_to_string() {
        let cp = ConfigParser::from_str("foo = quux\n  [Zulu] ; Zulu section\n \
                                         foo =  bar\n  [ Alpha ] \n`
                                         foo : wibble\n\nbar = quux  ", &[]);
        match cp.to_string() {
            Ok(s) => assert_eq!(s, "[Alpha]\nbar : quux\nfoo : wibble\n\n[Zulu]\nfoo : bar\n\n"),
            Err(_) => assert!(false)
        }
    }

    #[test]
    fn test_null_interp() {
        let cp = ConfigParser::from_str("foo = quux\n  [Zulu] \nfoo =  bar\n\
                a_quuxly = barly\n  [ Alpha ] ; alpha section\nfoo : wibble", &[]);

        match cp.get("Alpha", "foo") {
            Ok(v) => assert_eq!(v, "wibble"),
            Err(_) => assert!(false)
        }
        match cp.get("No-Such-Section", "foo") {
            Ok(_) => assert!(false),
            Err(e) => assert_eq!(e.kind(), FetchErrorKind::NoSuchSection)
        }
        match cp.get("Alpha", "No-Such-Option") {
            Ok(_) => assert!(false),
            Err(e) => assert_eq!(e.kind(), FetchErrorKind::NoSuchOption)
        }
    }

    #[test]
    fn test_simple_interp() {
        let cp = ConfigParser::from_str("[Section1]\nfoo =  My %(frob)s\nfrob : Option\n\
              double : %(frob)s %(frob)s\nquux : The %(bar)s", &[("bar", "wibble")]);
        match cp.get("Section1", "frob") {
            Ok(v) => assert_eq!(v, "Option"),
            Err(_) => assert!(false)
        }
        match cp.get("Section1", "foo") {
            Ok(v) => assert_eq!(v, "My Option"),
            Err(_) => assert!(false)
        }
        match cp.get("Section1", "double") {
            Ok(v) => assert_eq!(v, "Option Option"),
            Err(_) => assert!(false)
        }
        match cp.get("Section1", "quux") {
            Ok(v) => assert_eq!(v, "The wibble"),
            Err(_) => assert!(false)
        }
    }

    #[test]
    fn test_bad_interp() {
        let cp = ConfigParser::from_str("[Section1]\nfoo =  My %(nofrob)s\nfrob : Option\n", &[]);
        match cp.get("Section1", "frob") {
            Ok(v) => assert_eq!(v, "Option"),
            Err(_) => assert!(false)
        }
        match cp.get("Section1", "foo") {
            Ok(_) => assert!(false),
            Err(e) => assert_eq!(e.kind(), FetchErrorKind::InterpolationError)
        }
    }

    #[test]
    fn test_multi_level_interp() {
        let cp = ConfigParser::from_str("[Section1]\nfoo =  My %(frob)s\n\
                       frob : Option\nquux : This is %(foo)s text\n", &[]);
        match cp.get("Section1", "frob") {
            Ok(v) => assert_eq!(v, "Option"),
            Err(_) => assert!(false)
        }
        match cp.get("Section1", "quux") {
            Ok(v) => assert_eq!(v, "This is My Option text"),
            Err(_) => assert!(false)
        }
    }

    #[test]
    fn test_circular_interp() {
        let cp = ConfigParser::from_str("[Section1]\na : x%(b)sy\nb : x%(c)sy\nc: x%(a)sy\n", &[]);
        match cp.get("Section1", "c") {
            Ok(_) => assert!(false),
            Err(e) => assert_eq!(e.kind(), FetchErrorKind::InterpolationCircularity)
        }
    }

    #[test]
    fn test_section_manipulation() {
        let mut cp = ConfigParser::new(&[]);

        assert!(cp.add_section("foo").is_ok());
        match cp.add_section("foo") {
            Ok(_) => assert!(false),
            Err(e) => assert_eq!(e.kind(), FetchErrorKind::DuplicateSection)
        }
        assert!(cp.has_section("foo"));
        assert!(cp.remove_section("foo").is_ok());
        assert!(! cp.has_section("foo"));
        match cp.remove_section("foo") {
            Ok(_) => assert!(false),
            Err(e) => assert_eq!(e.kind(), FetchErrorKind::NoSuchSection)
        }
    }

    #[test]
    fn test_option_manipulation() {
        let mut cp = ConfigParser::new(&[]);
        assert!(cp.add_section("foo").is_ok());
        cp.set("foo", "bar", "quux");
        assert!(cp.has_option("foo", "bar").unwrap());
        assert!(! cp.has_option("foo", "wibble").unwrap());
        match cp.remove_option("foo", "bar") {
            Ok(_) => assert!(! cp.has_option("foo", "bar").unwrap()),
            Err(_) => assert!(false)
        }
        match cp.remove_option("foo", "bar") {
            Ok(_) => assert!(false),
            Err(e) => assert_eq!(e.kind(), FetchErrorKind::NoSuchOption)
        }
    }

    #[test]
    fn test_num_parsing() {
        let cp = ConfigParser::from_str(
            "[global]\n\
             t1 : 123456\n\
             t2 : -1234\n\
             t3 : not-a-good-number\n\
             t4 : 12E+99\n\
             t5",
            &[]);

        // unsigned tests first
        match cp.getuint("global","t1") {
            Ok(u) => assert_eq!(u, 123456),
            Err(_) => assert!(false)
        }
        match cp.getuint("global","t2") {
            Ok(_) => assert!(false),
            Err(e) => assert_eq!(e.kind(), FetchErrorKind::InvalidLiteral)
        }
        match cp.getuint("global","t3") {
            Ok(_) => assert!(false),
            Err(e) => assert_eq!(e.kind(), FetchErrorKind::InvalidLiteral)
        }
        match cp.getuint("global","t4") {
            Ok(_) => assert!(false),
            Err(e) => assert_eq!(e.kind(), FetchErrorKind::InvalidLiteral)
        }
        match cp.getuint("global","t5") {
            Ok(u) => assert_eq!(u, 1),
            Err(_) => assert!(false)
        }

        // now signed
        match cp.getint("global","t1") {
            Ok(i) => assert_eq!(i, 123456),
            Err(_) => assert!(false)
        }
        match cp.getint("global","t2") {
            Ok(i) => assert_eq!(i, -1234),
            Err(_) => assert!(false)
        }
        match cp.getint("global","t3") {
            Ok(_) => assert!(false),
            Err(e) => assert_eq!(e.kind(), FetchErrorKind::InvalidLiteral)
        }
        match cp.getuint("global","t4") {
            Ok(_) => assert!(false),
            Err(e) => assert_eq!(e.kind(), FetchErrorKind::InvalidLiteral)
        }
        match cp.getint("global","t5") {
            Ok(i) => assert_eq!(i, 1),
            Err(_) => assert!(false)
        }

        // now float
        match cp.getfloat("global","t1") {
            Ok(f) => assert_eq!(f, 123456.0),
            Err(_) => assert!(false)
        }
        match cp.getfloat("global","t2") {
            Ok(f) => assert_eq!(f, -1234.0),
            Err(_) => assert!(false)
        }
        match cp.getfloat("global","t3") {
            Ok(_) => assert!(false),
            Err(e) => assert_eq!(e.kind(), FetchErrorKind::InvalidLiteral)
        }
        match cp.getfloat("global","t4") {
            Ok(f) => assert_eq!(f, 12E+99f64),
            Err(_) => assert!(false)
        }
        match cp.getfloat("global","t5") {
            Ok(f) => assert_eq!(f, 1.0f64),
            Err(_) => assert!(false)
        }
    }

    #[test]
    fn test_bool_parsing() {
        let cp = ConfigParser::from_str(
            "[global]\n\
             t1 : true\n\
             t2 : yes\n\
             t3 : false\n\
             t4 : no\n\
             t5 : 0\n\
             t6 : 1\n\
             t7\n",
            &[]);

        // check that we got all the options we thought we
        // should have
        for t in &["t1", "t2", "t3", "t4", "t5", "t6", "t7"] {
            match cp.has_option("global", t) {
                Ok(r) => assert!(r),
                _ => assert!(false)
            }
        }

        // true tests
        for t in &["t1", "t2", "t6", "t7"] {
            match cp.getboolean("global", t) {
                Ok(r) => assert!(r),
                _ => assert!(false)
            }
        }

        // false tests
        for t in &["t3", "t4", "t5"] {
            match cp.getboolean("global", t) {
                Ok(r) => assert!(!r),
                _ => assert!(false)
            }
        }
    }
}
