# amfi

## amfi: library to fetch latest NAV data from AMFI

It aims to extract as much information from [AMFI] latest _nav_ public data as possible.

This library can also parse data mirrors and local file copies.
See [nav_from_url](fn.nav_from_url.html) and [nav_from_file](fn.nav_from_file.html).

### Basic Usage

```ignore,rust
let navs = amfi::daily_nav();
for item in items {
    match item {
        Err(error) => warn!("{}", error),
        Ok(ref record) => println!("{:>10} {} {}", record.nav, record.date, record.name),
    }
}
```

### Cargo features
Enable [serde](https://crates.io/crates/serde) feature for serialization/deserialization support.

[AMFI]: https://www.amfiindia.com

License: MIT OR Apache-2.0
