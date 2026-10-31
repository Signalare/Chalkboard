// SPDX-License-Identifier: MIT OR Apache-2.0

//! `chalkboard-agent`: discovery, replication, HTTP API and web app, HDMI-CEC,
//! time, provisioning and updates (SPEC §3).

use chalkboard_agent::banner;

fn main() {
    println!("{}", banner());
}

#[cfg(test)]
mod tests {
    use super::banner;

    #[test]
    fn banner_names_the_binary_and_version() {
        assert_eq!(
            banner(),
            format!("chalkboard-agent {}", env!("CARGO_PKG_VERSION"))
        );
    }
}
