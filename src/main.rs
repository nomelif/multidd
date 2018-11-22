extern crate argparse;
extern crate pbr;

use argparse::{ArgumentParser, StoreTrue, Store};
use std::fs::File;
use std::io::Read;
use std::io::Write;
use std::fs;
use pbr::ProgressBar;
use pbr::Units;

fn main() {

  let mut quiet = false;
  let mut input = String::new();
  let mut output = String::new();
  let mut blocksize : usize = 8192;

  {
    let mut ap = ArgumentParser::new();
    ap.set_description("Copy from one location to multiple while monitoring progress.");
    ap.refer(&mut quiet)
        .add_option(&["-q", "--quiet"], StoreTrue, "Do not output prgress");
    ap.refer(&mut input)
        .add_option(&["-i", "--if"], Store, "Input file (omit for stdin)");
    ap.refer(&mut output)
        .add_option(&["-o", "--of"], Store, "Input file (omit for stdout)");
    ap.refer(&mut blocksize)
        .add_option(&["-b", "--block"], Store, "Block size in bytes");
    ap.parse_args_or_exit();
  }


  // We are going to need a mutable buffer in all cases

  let mut buf = vec![0u8;blocksize];

  // Writing to a file or a set of files

  if output != "" {

    // Open file handles 

    let mut outfiles : Vec<File> = output.split(";")
               .map(|file_name|
                 File::create(file_name)
                   .expect(&format!("Couldn't open output file {}", file_name))).collect();

    // Read from a file

    if input != "" {
      let mut file = File::open(&input).expect(&format!("Couldn't open input file {}", &input));
      let fsize = fs::metadata(&input).expect(&format!("Failed to look up size for {}", &input)).len();
      let mut len = file.read(&mut buf).expect(&format!("Failed initial read on {}", &input));
      let mut count = len as u64;
      let mut pb = ProgressBar::new(fsize);

      if !quiet {
        pb.set_units(Units::Bytes);
        pb.format("[=> ]");
      }

      let mut iteration = 0;

      while len > 0 {

        // Update the ui

        if iteration % 100 == 0 && !quiet{
          pb.set(count);
        }

        for mut outfile in &mut outfiles{
          outfile.write_all(&buf[..len]).expect("Write failure");
        }

        len = file.read(&mut buf).expect(&format!("Read failure on {}", &input));
        count += len as u64;
        iteration += 1;
      }

    }else{ // Case 2: reading from stdin

      let mut stdin = std::io::stdin();
      let mut len = stdin.read(&mut buf).expect("Failed initial read from stdin");

      while len > 0 {
        for mut outfile in &mut outfiles {
          outfile.write_all(&buf[..len]).expect("Write failure");
        }
        len = stdin.read(&mut buf).expect("Read failure from stdin");
      }
    }
  } else {
  let mut stdout = std::io::stdout();
    if input != "" {
      let mut file = File::open(&input).expect(&format!("Couldn't open input file {}", &input));
      let mut len = file.read(&mut buf).expect(&format!("Failed initial read on {}", &input));
      while len > 0 {
        stdout.write_all(&buf[..len]).expect("Failed writing to stdout");
        len = file.read(&mut buf).expect(&format!("Read failure on {}", &input));
      }
  }else{
    let mut stdin = std::io::stdin();
    let mut len = stdin.read(&mut buf).expect("Failed initial read on stdin");
    while len > 0 {
      stdout.write_all(&buf[..len]).expect("Failed writing to stdout");
      len = stdin.read(&mut buf).expect("Failed reading from stdin");
    }
   }
  }
}
