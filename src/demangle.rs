// Based on rust-lang/rust's rust-demangler.
//
// Source:
// - https://github.com/rust-lang/rust/tree/1.67.0/src/tools/rust-demangler
//
// Copyright & License:
// - https://github.com/rust-lang/rust/blob/1.67.0/COPYRIGHT
// - https://github.com/rust-lang/rust/blob/1.67.0/LICENSE-APACHE
// - https://github.com/rust-lang/rust/blob/1.67.0/LICENSE-MIT

use std::{
    io::{self, Read, Write},
    str::Lines,
};

use anyhow::Result;
use regex::Regex;
use rustc_demangle::demangle;

const REPLACE_COLONS: &str = "::";

fn create_disambiguator_re() -> Regex {
    Regex::new(r"\[[0-9a-f]{5,16}\]::").unwrap()
}

fn demangle_lines(lines: Lines<'_>) -> Vec<String> {
    let strip_crate_disambiguators = create_disambiguator_re();
    let mut demangled_lines = Vec::new();
    for mangled in lines {
        let mut demangled = demangle(mangled).to_string();
        demangled = strip_crate_disambiguators.replace_all(&demangled, REPLACE_COLONS).to_string();
        demangled_lines.push(demangled);
    }
    demangled_lines
}

pub(crate) fn run() -> Result<()> {
    let mut buffer = String::new();
    io::stdin().read_to_string(&mut buffer)?;
    let mut demangled_lines = demangle_lines(buffer.lines());
    demangled_lines.push(String::new()); // ensure a trailing newline
    io::stdout().write_all(demangled_lines.join("\n").as_bytes())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANGLED_INPUT: &str = r"
_RNvC6_123foo3bar
_RNqCs4fqI2P2rA04_11utf8_identsu30____7hkackfecea1cbdathfdh9hlq6y
_RNCNCNgCs6DXkGYLi8lr_2cc5spawn00B5_
_RNCINkXs25_NgCsbmNqQUJIY6D_4core5sliceINyB9_4IterhENuNgNoBb_4iter8iterator8Iterator9rpositionNCNgNpB9_6memchr7memrchrs_0E0Bb_
_RINbNbCskIICzLVDPPb_5alloc5alloc8box_freeDINbNiB4_5boxed5FnBoxuEp6OutputuEL_ECs1iopQbuBiw2_3std
INtC8arrayvec8ArrayVechKj7b_E
_RMCs4fqI2P2rA04_13const_genericINtB0_8UnsignedKhb_E
_RMCs4fqI2P2rA04_13const_genericINtB0_6SignedKs98_E
_RMCs4fqI2P2rA04_13const_genericINtB0_6SignedKanb_E
_RMCs4fqI2P2rA04_13const_genericINtB0_4BoolKb0_E
_RMCs4fqI2P2rA04_13const_genericINtB0_4BoolKb1_E
_RMCs4fqI2P2rA04_13const_genericINtB0_4CharKc76_E
_RMCs4fqI2P2rA04_13const_genericINtB0_4CharKca_E
_RMCs4fqI2P2rA04_13const_genericINtB0_4CharKc2202_E
_RNvNvMCs4fqI2P2rA04_13const_genericINtB4_3FooKpE3foo3FOO
_RC3foo.llvm.9D1C9369
_RC3foo.llvm.9D1C9369@@16
_RNvC9backtrace3foo.llvm.A5310EB9
_RNvNtNtNtNtCs92dm3009vxr_4rand4rngs7adapter9reseeding4fork23FORK_HANDLER_REGISTERED.0.0
_RNvB_1a
RYFG_FGyyEvRYFF_EvRYFFEvERLB_B_B_ERLRjB_B_B_
";

    const DEMANGLED_OUTPUT_NO_CRATE_DISAMBIGUATORS: &str = r"
123foo[0]::bar
utf8_idents::საჭმელად_გემრიელი_სადილი
cc::spawn::{closure#0}::{closure#0}
<core::slice::Iter<u8> as core::iter::iterator::Iterator>::rposition::<core::slice::memchr::memrchr::{closure#1}>::{closure#0}
alloc::alloc::box_free::<dyn alloc::boxed::FnBox<(), Output = ()>>
INtC8arrayvec8ArrayVechKj7b_E
<const_generic::Unsigned<11u8>>
<const_generic::Signed<152i16>>
<const_generic::Signed<-11i8>>
<const_generic::Bool<false>>
<const_generic::Bool<true>>
<const_generic::Char<'v'>>
<const_generic::Char<'\n'>>
<const_generic::Char<'∂'>>
<const_generic::Foo<_>>::foo::FOO
foo[0]
foo[0]
backtrace[0]::foo
rand::rngs::adapter::reseeding::fork::FORK_HANDLER_REGISTERED.0.0
{recursion limit reached}
{size limit reached}
";

    #[test]
    fn test_demangle_lines_no_crate_disambiguators() {
        let demangled_lines = demangle_lines(MANGLED_INPUT.lines());
        for (expected, actual) in
            DEMANGLED_OUTPUT_NO_CRATE_DISAMBIGUATORS.lines().zip(demangled_lines)
        {
            match expected {
                "{recursion limit reached}" => {
                    assert_eq!(expected, &actual[..expected.len()]);
                }
                "{size limit reached}" => {
                    assert_eq!(expected, &actual[actual.len() - expected.len()..]);
                }
                _ => {
                    assert_eq!(expected, actual);
                }
            }
        }
    }
}
