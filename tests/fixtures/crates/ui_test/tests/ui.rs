// SPDX-License-Identifier: Apache-2.0 OR MIT

use ui_test::{Config, dependencies::DependencyBuilder, run_tests};

fn main() -> ui_test::color_eyre::Result<()> {
    let mut config = Config::rustc("tests/ui");
    config.comment_defaults.base().set_custom("dependencies", DependencyBuilder::default());
    let abort_check = config.abort_check.clone();
    ctrlc::set_handler(move || abort_check.abort())?;
    run_tests(config)
}
