use amfi::NavRecordIterator;
use std::env;
use std::io::Read;
use std::path::PathBuf;

fn parse<T: Read>(items: NavRecordIterator<T>) -> Result<(), Box<dyn std::error::Error>> {
    let mut c = 0;
    let mut e = 0;
    for item in items {
        match item {
            Err(error) => {
                e += 1;
                eprintln!("{}", error)
            }
            Ok(ref record) => {
                c += 1;
                #[cfg(feature = "serde")]
                println!("{}", serde_json::to_string(&record)?);
                #[cfg(not(feature = "serde"))]
                println!("{:>10.4}  {}  {}", record.nav, record.date, record.name);
            }
        }
    }
    println!("Total: {} Error: {}", c, e);
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();

    if args.len() == 2 && args[1] == "--online" {
        let navs = amfi::nav_from_url("http://localhost:8000/NAVAll.txt")?;
        parse(navs)?;
    } else {
        let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        path.push("fixtures/NAVOpen.txt");
        let navs = amfi::nav_from_file(path)?;
        parse(navs)?;
    };

    Ok(())
}
