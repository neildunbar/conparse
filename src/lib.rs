#![feature(core)]
#![feature(path)]
#![feature(io)]
#![feature(collections)]
#![feature(std_misc)]

//! # ConfigParser - a Python style configuration file reader/writer
//!
//! The `conparse::conparse::ConfigParser` struct implements the means
//! to read INI style configuration files from files, strings, or any
//! Reader compatible object, and to present an interface to programs
//! to read that configuration.
//!
//! The text which can be read can include comments (which are lines
//! beginning with a `;` or `#` character), and can also support Rust
//! style continuations. A line ending with a `\` character does not
//! cause the line to be consumed immediately, but prepended to the
//! next line read, and so on, until a line not ending with a `\`
//! character is found, or the end of the file is reached. Note that
//! leading whitespace is ignored on continued lines, so one can make
//! the configuration file visually appealing without changing its
//! meaning.
//!
//! The files are divided up into sections, which are started with the
//! text `[ Section_Name ]`. The section line can also have a comment
//! after its `]` character.
//!
//! Options have the form of `key : value`, where key is an identifier
//! like string (typically `A-Z`, `a-z`, `0-9`, `_`) and value is a generic
//! string. The alternative form of `key = value` is allowed. Leading
//! whitespace after the `=` or `:` characters is stripped.
//!
//! ConfigParser also supports value interpolation (like its Python
//! inspired counterpart), so that strings which have the form
//! `%(keyname)s` have the value of the option `keyname` substituted
//! into their values when read. Note that interpolation is
//! multi-leveled, meaning that the interpolated value may itself
//! contain an interpolation, and so on. When requested, the parser
//! will attempt to resolve all interpolations, and will emit an error
//! if a recursive loop is detected.
//!
//! Lastly, the application initialising a ConfigParser object can
//! supply a set of default (key, value) pairs which will be supplied
//! as values even if the configuration files do not contain those
//! values.
//!
//! ## An example configuration file
//!
//! ```ignore
//! # Config file for myapp, issued 2014-12-30
//! ;
//! ;
//! [default]
//! version = 0.1.0
//! host : myhost.mydomain.org
//! port : 10342
//! app_uri : http://%(host)s:%(port)s/v1/myapp
//! greeting : Hello, and welcome to {bs}
//!            my new application, version {bs}
//!            %(appver)s
//! # end of file
//! ```
//!
//! (Note: In the above example, the `{bs}` represents the backslash
//! character, but the Rust parser will attempt to use it itself,
//! which is unfortunate. Is there a way to embed a literal backslash
//! in the comments without it getting chomped?)
//!
//! Assuming this file was stored in `/etc/myapp/config.txt`, then the
//! ConfigParser can be initialised thus
//!
//! ```rust.{example}
//! #[macro_use] extern crate log;
//! extern crate conparse;
//!
//! use conparse::conparse::{ConfigParser,FetchError};
//!
//! fn main() {
//!     let cp = ConfigParser::from_file("/etc/myapp/config.txt",
//!                &[("port","2000"), ("db_provider","mysql")]);
//!     match cp.get("default", "app_uri") {
//!         Ok(uri) => { println!("URI is {}", uri);}
//!         Err(fe) => { error!("Cannot fetch uri from config: {:?}", fe); }
//!     }
//!     match cp.get("default", "greeting") {
//!         Ok(hello) => { println!(">>> {} <<<", hello);}
//!         Err(fe) => { error!("Cannot fetch greeting from config: {:?}", fe); }
//!     }
//!     match cp.get("default", "db_provider") {
//!         Ok(db_prov) => { println!("Using {} database", db_prov);}
//!         Err(fe) => { error!("Cannot fetch db_provider from config: {:?}", fe); }
//!     }
//! }
//! ```
//! Which should print out the text
//!
//! ```ignore
//! URI is http://myhost.mydomain.org:10342/v1/myapp
//! >>> Hello, and welcome to my new application, version 0.1.0 <<<
//! Using mysql database
//! ```
//!
//! ## Error Types
//!
//! Error types are embedded in the `FetchError` type, which has the
//! following values and meanings
//!
//! | Value | Meaning |
//! |-------|:--------|
//! | NoSuchSection | The requested section cannot be found |
//! | NoSuchOption | The requested option cannot be found |
//! | InterpolationError | An option was found, but requested an interpolation object which cannot be found |
//! | InterpolationCircularity | The requested interpolation caused a recursive loop |
//! | DuplicateSection | An attempt was made to insert a new section which already exists |
//! | InvalidLiteral | A typed option coerce failed because the text did not contain an object of that type |
//!
//! That last error is caused when using the convenience methods
//! `getuint`, `getboolean` etc, and is emitted when attempting to coerce
//! an invalidly formed string value (e.g. `frob`) into a boolean,
//! integer or float value.
//!
//! Options can be fetched in a raw string format (ie, where no
//! interpolation is attempted) by using the `get_raw` method.
//!
//! ## Multiple Sources
//!
//! An application can source a configuration from multiple sources
//! upon initialisation, by using the `from_files` (for multiple
//! files) or `from_strs` (for multiple strings). There also exists a
//! `from_readers` function which will take a vector of `Reader`
//! objects, so it should be possible to source from files and URIs at
//! the same time.
//! 
//! This can be useful when one wishes to have an application read
//! from a system distributed configuration file, but allow users to
//! have local overrides for the system configuration.
//!
//! An example:
//!
//! ```rust.{example}
//! #[macro_use] extern crate log;
//! extern crate conparse;
//!
//! use conparse::conparse::{ConfigParser,FetchError};
//!
//! fn main() {
//!     let homecfg = "~/.myapprc";
//!     let cp = ConfigParser::from_files(
//!            &["/etc/myapp/config.txt", homecfg ], &[]);
//!     match cp.get("default", "app_uri") {
//!         Ok(uri) => { println!("URI is {}", uri);}
//!         Err(fe) => { error!("Cannot fetch uri from config: {:?}", fe); }
//!     }
//! }
//! ```
//! The values will be placed in order of configuration source, with
//! keys from `~/.myapprc` replacing those from `config.txt`.
//!
//! ## Setting Configuration Values
//!  
//! It is possible to set new sections and options within a
//! configuration parser, and then write those changes out to a
//! `Writer` trait enabled object. Convenience methods for writing to
//! `String` and files are provided.
//!
//! Note that the `ConfigParser` object must be declared as `mut` to
//! allow this to happen - read only parsers can be declared
//! immutable.
//!
//! ### An example
//!
//! ```rust.example()
//! extern crate conparse;
//! #[macro_use] extern crate log;
//!
//! use conparse::conparse::ConfigParser;
//!
//! fn main() {
//!     let mut cp = ConfigParser::from_str(
//!              "[default] ; top section \n  hostname=localhost \n",
//!              &[("domain", "mydomain.org")]);
//!     match cp.set("default", "hostname", "myhost.%(domain)s") {
//!         Ok(_) => {},
//!         Err(e) => { error!("Unable to set hostname: {:?}", e);}
//!     }
//!     match cp.set("default", "port", "11313") {
//!         Ok(_) => {},
//!         Err(e) => { error!("Unable to set port: {:?}", e);}
//!     }
//!     match cp.get("default", "hostname") {
//!         Ok(h) => { println!("Hostname is: {}", h); },
//!         Err(e) => { error!("Unable to get hostname: {:?}", e);}
//!     }
//!     match cp.to_string() {
//!         Ok(s) => {println!("New config is\n{}", s);},
//!         Err(e) => { error!("Cannot write config: {:?}", e)}
//!     }
//! }
//! ```
//!
//! which should produce the output
//!
//! ```ignore
//! Hostname is : myhost.mydomain.org
//! New config is
//! [default]
//! hostname : myhost.%(domain)s
//! port : 11313
//! ```
//! 
//! Note that the written object preserves the 'raw' value (ie, no
//! interpolation is performed).
//!
//! While the `to_writer` method can be used for arbitrary `Writer`
//! output, the `to_file` convenience method can be used to simply
//! supply a file name, and the configuration data will be written to
//! that file, with default permissions and ownership.
//!
//! ## Convenience Getter Methods
//! 
//! Like the Python equivalent, some methods to parse values of
//! different types are allowed. Normally values are simply strings,
//! but sometimes one wishes booleans, or numbers.
//!
//! Values using the text `true`, `on`, `yes` or `1` can be coerced
//! into a `bool` `true` value. Values using the text `false`, `off`, `no` or
//! `0` can be coerced into a `bool` `false` value. Any other string
//! will cause an `InvalidLiteral` error to be returned. Note that
//! boolean string values are case independent. Boolean values are
//! fetched via the `getboolean` method call.
//!
//! There are two flavours of integer fetching: `getuint` and
//! `getint`, which return results containing `usize` or `isize`
//! objects respectively. This differs from the Python moduel, since
//! only `getint` is provided, but Rust's integer types are
//! considerably different from Python's.
//!
//! Lastly, the `getfloat` method can be used to coerce the string
//! into a `f64` type.
//!

#[macro_use] extern crate log;
extern crate env_logger;

pub mod conparse;
