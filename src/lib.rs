//! # amfi: library to fetch latest NAV data from AMFI
//!
//! It aims to extract as much information from [AMFI] latest _nav_ public data as possible.
//!
//! This library can also parse data mirrors and local file copies.
//! See [nav_from_url](fn.nav_from_url.html) and [nav_from_file](fn.nav_from_file.html).
//!
//! ## Basic Usage
//!
//! ```ignore,rust
//! let navs = amfi::daily_nav();
//! for item in items {
//!     match item {
//!         Err(error) => warn!("{}", error),
//!         Ok(ref record) => println!("{:>10} {} {}", record.nav, record.date, record.name),
//!     }
//! }
//! ```
//!
//! ## Cargo features
//! Enable [serde](https://crates.io/crates/serde) feature for serialization/deserialization support.
//!
//! [AMFI]: https://www.amfiindia.com

use chrono::NaiveDate;
use derive_builder::Builder;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
use std::convert::AsRef;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Read};
use std::path::Path;
use synom::{
    alt, call, do_parse, map, named, option, tag, take_until, terminated, tuple, tuple_parser,
    IResult,
};

const BASE_URL: &str = "http://portal.amfiindia.com/spages/NAVAll.txt";

#[derive(Debug, Builder)]
#[builder(setter(into), private)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
/// Net Asset Value Record
pub struct NavRecord {
    /// Scheme Code
    pub code: u32,
    /// ISIN Growth/Divdend Payout
    pub isin: Option<String>,
    /// ISIN Divdend Reinvestment
    pub isin_dr: Option<String>,
    /// Scheme Name
    pub name: String,
    /// Net Asset Value (NAV)
    pub nav: f64,
    /// NAV Date
    pub date: NaiveDate,
    /// Asset Management Company (AMC)
    pub amc: String,
    /// Category
    pub category: String,
    /// Scheme
    pub scheme: Option<String>,
    /// Fund Maturity (Open/Close Ended)
    pub maturity: Option<FundMaturity>,
    /// Plan (Regular/Direct)
    pub plan: FundPlan,
    /// Option (Growth/Monthly Dividend Payout etc.)
    pub option: Option<String>,
}

/// Error type
#[derive(Debug)]
pub enum Error {
    /// Error from IO operation
    IoError(io::Error),
    /// Error from reqwest library
    ReqwestError(reqwest::Error),
    /// Error from Builder parser
    BuilderError(String),
    /// Error from Synom parser combinator
    SynomError(String),
    /// HTTP Error from server
    HttpError(reqwest::StatusCode),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match *self {
            Error::IoError(ref err) => write!(f, "IO error: {}", err),
            Error::ReqwestError(ref err) => write!(f, "Reqwest error: {}", err),
            Error::BuilderError(ref err) => write!(f, "Builder error: {}", err),
            Error::SynomError(ref err) => write!(f, "Synom error: Error parsing line `{}`", err),
            Error::HttpError(ref err) => write!(f, "Http error: {}.", err.as_str()),
        }
    }
}

impl std::error::Error for Error {
    fn description(&self) -> &str {
        match *self {
            Error::IoError(ref err) => err.description(),
            Error::ReqwestError(ref err) => err.description(),
            Error::BuilderError(ref err) => err.as_str(),
            Error::SynomError(ref err) => err.as_str(),
            Error::HttpError(ref err) => err.as_str(),
        }
    }

    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match *self {
            Error::IoError(ref err) => Some(err),
            Error::ReqwestError(ref err) => Some(err),
            Error::HttpError(..) | Error::BuilderError(..) | Error::SynomError(..) => None,
        }
    }
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Error::IoError(e)
    }
}

impl From<reqwest::Error> for Error {
    fn from(e: reqwest::Error) -> Self {
        Error::ReqwestError(e)
    }
}

type Result<T> = std::result::Result<T, Error>;

named!(
    parse_isin -> Option<String>,
    map!(
        alt!( alphanumeric | tag!("---") | tag!("-") ),
        |isin: &str| {
            match isin {
                "-" | "---" => None,
                s => Some(s.to_string())
            }
        }
    )
);

named!(
    parse_name -> (String, FundPlan, Option<String>),
    map!(
        take_until!(";"),
        |s: &str| {
            let name = s.trim().to_string();
            let plan = if name.to_uppercase().find("DIRECT").is_some() {
                FundPlan::Direct
            } else {
                FundPlan::Regular
            };
            let option = None;
            (name, plan, option)
        }
    )
);

fn custom_seperator(input: &str) -> IResult<&str, ()> {
    let mut pos = 0;
    for ch in input.chars() {
        if ch.is_whitespace() || ch == ';' {
            pos += 1;
        } else {
            break;
        }
    }
    IResult::Done(&input[pos..], ())
}

fn alphanumeric(input: &str) -> IResult<&str, &str> {
    let mut pos = 0;
    for ch in input.chars() {
        if ch.is_alphanumeric() {
            pos += 1;
        } else {
            break;
        }
    }

    if pos > 0 {
        IResult::Done(&input[pos..], &input[..pos])
    } else {
        IResult::Error
    }
}

fn digit(input: &str) -> IResult<&str, u32> {
    let mut pos = 0;
    for ch in input.chars() {
        if ch.is_digit(10) {
            pos += 1;
        } else {
            break;
        }
    }

    if pos > 0 {
        IResult::Done(&input[pos..], input[..pos].parse::<u32>().unwrap())
    } else {
        IResult::Error
    }
}

fn double(input: &str) -> IResult<&str, f64> {
    let mut pos = 0;
    let mut seen_decimal_sign = false;
    for ch in input.chars() {
        if ch.is_digit(10) {
            pos += 1;
        } else if !seen_decimal_sign && ch == '.' {
            seen_decimal_sign = true;
            pos += 1;
        } else {
            break;
        }
    }

    if pos > 0 {
        IResult::Done(&input[pos..], input[..pos].parse::<f64>().unwrap())
    } else {
        IResult::Error
    }
}

fn date(input: &str) -> IResult<&str, chrono::NaiveDate> {
    if let Some(slice) = input.get(..11) {
        if let Ok(dt) = chrono::NaiveDate::parse_from_str(slice, "%d-%b-%Y") {
            IResult::Done(&input[11..], dt)
        } else {
            IResult::Error
        }
    } else {
        IResult::Error
    }
}

named!(
    parse_record -> NavRecordBuilder,
    do_parse!(
        code: digit >>
        custom_seperator >>
        isin: parse_isin >>
        custom_seperator >>
        isin_dr: parse_isin >>
        custom_seperator >>
        name_plan: parse_name >>
        custom_seperator >>
        nav: double >>
        custom_seperator >>
        date: date >>
        ({
            let mut rb = NavRecordBuilder::default();
            let (name, plan, option) = name_plan;
            rb.code(code)
                .isin(isin)
                .isin_dr(isin_dr)
                .name(name)
                .plan(plan)
                .option(option)
                .nav(nav)
                .date(date);
            rb
        })
    )
);

named!(
    parse_scheme -> (Option<FundMaturity>, Option<String>, String),
    do_parse!(
        maturity: take_until!("(") >>
        tag!("(") >>
        scheme: option!( terminated!( take_until!(" - "), tag!(" - ") ) ) >>
        category: take_until!(")") >>
        ({
            let muc = maturity.trim().to_uppercase();
            let maturity = if muc.starts_with("CLOSE") {
                Some(FundMaturity::CloseEnded)
            } else if muc.starts_with("OPEN") {
                Some(FundMaturity::OpenEnded)
            } else {
                None
            };
            (
                maturity,
                scheme.map(|s: &str| s.to_string()),
                category.to_string()
            )
        })
    )
);

/// Parses NAV data from [AMFI](https://www.amfiindia.com) portal
///
/// Primary access method for latest data. See [example](index.html#basic-usage)
pub fn daily_nav() -> Result<NavRecordIterator<reqwest::Response>> {
    nav_from_url(BASE_URL)
}

/// Parses NAV data from provided `url`
///
/// Parse NAV data from any mirror site providing same data format.
pub fn nav_from_url<T: AsRef<str>>(url: T) -> Result<NavRecordIterator<reqwest::Response>> {
    let response = reqwest::get(url.as_ref())?;
    if response.status().is_success() {
        Ok(NavRecordIterator::new(response))
    } else {
        Err(Error::HttpError(response.status()))
    }
}

/// Parses NAV data from local file
///
/// Parse NAV data from local copy in same data format.
pub fn nav_from_file<P: AsRef<Path>>(path: P) -> Result<NavRecordIterator<File>> {
    let file = File::open(path)?;
    Ok(NavRecordIterator::new(file))
}

enum LineType {
    Record,
    Amc,
    Scheme,
    Blank,
    Header,
}

/// Open/Closed Funds
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum FundMaturity {
    /// Open Ended Funds
    OpenEnded,
    /// Close Ended Funds
    CloseEnded,
}

/// Fund Plans are identified on best effort basis. By default plans are Regular.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum FundPlan {
    /// Regular Plan
    Regular,
    /// Direct Plan
    Direct,
}

/// Iterator over [`NavRecord`](NavRecord)
pub struct NavRecordIterator<T> {
    reader: BufReader<T>,
    amc: String,
    category: String,
    scheme: Option<String>,
    maturity: Option<FundMaturity>,
    buf: String,
    bailout: bool,
}

impl<T: Read> NavRecordIterator<T> {
    fn new(response: T) -> Self {
        NavRecordIterator {
            reader: BufReader::new(response),
            amc: String::new(),
            category: String::new(),
            scheme: None,
            buf: String::new(),
            bailout: false,
            maturity: None,
        }
    }
    fn line_type(&self) -> LineType {
        let mut lt = LineType::Blank;
        if self.buf.starts_with("Scheme") {
            lt = LineType::Header;
        } else if self.buf.find(";").is_some() {
            lt = LineType::Record;
        } else if self.buf.find("Ended Scheme").is_some() {
            lt = LineType::Scheme;
        } else if !self.buf.trim().is_empty() {
            lt = LineType::Amc;
        }
        lt
    }
}

impl<T: Read> Iterator for NavRecordIterator<T> {
    type Item = Result<NavRecord>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut item = None;

        while !self.bailout && item.is_none() {
            self.buf.clear();
            match self.reader.read_line(&mut self.buf) {
                Ok(0) => {
                    break;
                }
                Err(e) => {
                    item = Some(Err(e.into()));
                    break;
                }
                _ => match self.line_type() {
                    LineType::Record => {
                        item = Some(match parse_record(&self.buf.trim()) {
                            IResult::Done(_rem, mut rb) => rb
                                .maturity(self.maturity.clone())
                                .amc(self.amc.clone())
                                .scheme(self.scheme.clone())
                                .category(self.category.clone())
                                .build()
                                .map_err(Error::BuilderError),
                            IResult::Error => Err(Error::SynomError(self.buf.trim().to_string())),
                        })
                    }
                    LineType::Scheme => {
                        match parse_scheme(&self.buf.trim()) {
                            IResult::Done(_, (maturity, scheme, category)) => {
                                self.maturity = maturity;
                                self.scheme = scheme;
                                self.category = category;
                            }
                            IResult::Error => {
                                self.bailout = true;
                                item = Some(Err(Error::SynomError(self.buf.clone())));
                            }
                        };
                    }
                    LineType::Amc => {
                        self.amc = self.buf.trim().to_string();
                    }
                    LineType::Blank | LineType::Header => (),
                },
            }
        }
        item
    }
}
#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
