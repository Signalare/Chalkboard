// SPDX-License-Identifier: MIT OR Apache-2.0

//! `cb-template validate FILE...` — checks template files and prints every
//! problem with its location. Exits non-zero if any file is invalid.

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some((command, files)) = args.split_first() else {
        eprintln!("usage: cb-template validate FILE...");
        return ExitCode::from(2);
    };
    if command != "validate" || files.is_empty() {
        eprintln!("usage: cb-template validate FILE...");
        return ExitCode::from(2);
    }
    let mut ok = true;
    for file in files {
        match std::fs::read(file) {
            Err(e) => {
                println!("{file}: cannot read: {e}");
                ok = false;
            }
            Ok(bytes) => match chalkboard_template::validate(&bytes) {
                Ok(t) => println!("{file}: ok ({})", t.name),
                Err(issues) => {
                    ok = false;
                    for issue in issues {
                        println!("{file}: {issue}");
                    }
                }
            },
        }
    }
    if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
