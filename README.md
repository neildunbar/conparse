ConfigParser for Rust
=====================

This is my learning project for Rust, which will explain many of the
'eeeww!' comments which will be seen while viewing the code. It also
explains some of those comments which I have made in the code myself.

ConfigParser is designed to emulate the Python module of the same
name, which reads and writes INI style files for application
configurations. It supports:

* Merging configurations from multiple sources
* Interpolation of variable names into other option values
* Rust style line continuations, allowing very long values
* Comments
* Configurations to be read from strings, files and other Readers
* Configurations to be written to strings, files and other Writers

Comments are in the code, in Rustdoc format.

There are certainly plenty of INI readers out there for rust, most
notably [Rust-Ini](https://github.com/zonyitoo/rust-ini). I wanted one
which didn't need the parser variable to be declared mutable if I was
just reading from it, and I rather liked interpolation, since it saves
config file typing.

And because my config files often have long values for their
variables, I like having backslash line continuations.
