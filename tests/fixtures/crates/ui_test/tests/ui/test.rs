// SPDX-License-Identifier: Apache-2.0 OR MIT

#[ui_test_test::m(compile_error!("a");)]
//~^ ERROR: a
compile_error!("b");

fn main() {}
