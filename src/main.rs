extern crate argparse;
extern crate pbr;

use argparse::{ArgumentParser, StoreTrue, Store};
use std::fs::File;
use std::io::Read;
use std::io::Write;
use std::io::prelude::*;
use std::io::SeekFrom;
use pbr::ProgressBar;
use pbr::Units;
use std::io::{Error, ErrorKind};


trait Sink{
  fn write(&mut self, buf : &[u8]) -> std::io::Result<()>;
  fn force_sync(&mut self) -> std::io::Result<()>;
}

trait Source{
  fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize>;
  fn size(&mut self) -> std::io::Result<u64>;
}

// Struct to read data from a file

struct FileSource {file : File}

impl FileSource {
  fn new(string : &str) -> Result<FileSource, String>{
    match File::open(&string) {
      Ok(x) => Ok(FileSource{file : x}),
      Err(_) => Err(format!("Failed to open {} for reading", string)),
    }
  }
}

impl Source for FileSource {
  fn read(&mut self, buf : &mut [u8]) -> std::io::Result<usize>{
    self.file.read(buf)
  }

  fn size(&mut self) -> std::io::Result<u64>{
    let fsize = self.file.seek(SeekFrom::End(0));
    self.file.seek(SeekFrom::Start(0)).expect("Couldn't seek back to start of input file");
    fsize.and_then(|x| match x {
        0 => Err(std::io::Error::new(ErrorKind::Other, "Zero-size file; can't display progress")),
        x => Ok(x)
    })
  }
}

// Struct to read data from stdin

struct StdInSource{stdin : std::io::Stdin}

impl StdInSource{
  fn new() -> StdInSource{
    StdInSource{stdin : std::io::stdin(),}
  }
}

impl Source for StdInSource{
  fn read(&mut self, buf : &mut [u8]) -> std::io::Result<usize>{
      self.stdin.read(buf)
  }

  fn size(&mut self) -> std::io::Result<u64>{
    Err(std::io::Error::new(ErrorKind::Other, "Stream size undefined"))
  }
}



// Struct to pipe data to a set of files

struct FileArraySink { files: Vec<File> }

impl FileArraySink {
  fn new(strings : &Vec<&str>) -> Result<FileArraySink, String>{
      let mut file_vector = vec![];
      for filename in strings{
          let file = File::create(filename);
          if file.is_err() {
            return Err(format!("Failed to open file {} for writing", filename));
          }
          file_vector.push(file.unwrap());
      }
      Ok(FileArraySink{files : file_vector})
  }
}

impl Sink for FileArraySink {
  fn write(&mut self, buf : &[u8]) -> std::io::Result<()>{
    let mut result = Ok(());
    for mut outfile in &mut self.files.iter(){
      result = result.and_then(|_| outfile.write_all(buf));
    }
    result
  }
  fn force_sync(&mut self) -> std::io::Result<()>{
    let mut result = Ok(());
    for mut outfile in &mut self.files.iter(){
      result = result.and_then(|_| outfile.sync_all());
    }
    result
  }
}

// Struct to pipe data out to stdio

struct StdOutSink{stdout : std::io::Stdout}

impl StdOutSink{
  fn new() -> StdOutSink{
    StdOutSink{stdout : std::io::stdout(),}
  }
}

impl Sink for StdOutSink{
  fn write(&mut self, buf : &[u8]) -> std::io::Result<()>{
      self.stdout.write_all(buf)
  }
  fn force_sync(&mut self) -> std::io::Result<()> { Ok(()) }
}

trait ProgressMonitor {
    fn setProgress(&mut self, progress : u64) -> ();
}

struct DummyProgressMonitor { }

impl DummyProgressMonitor {
  fn new() -> DummyProgressMonitor{
    DummyProgressMonitor{}
  }
}

impl ProgressMonitor for DummyProgressMonitor {
  fn setProgress(&mut self, progress : u64) -> () { }
}

struct ProgressBarProgressMonitor { pb : ProgressBar<std::io::Stderr> }

impl ProgressBarProgressMonitor {
  fn new(fsize : u64) -> ProgressBarProgressMonitor{
    let mut pb = ProgressBar::on(std::io::stderr(), fsize);
    pb.set_units(Units::Bytes);
    pb.format("[=> ]");
    ProgressBarProgressMonitor {pb : pb}
  }
}

impl ProgressMonitor for ProgressBarProgressMonitor{
  fn setProgress(&mut self, progress : u64) -> () {
    self.pb.set(progress);
  }
}


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

  let mut output_sink : Box<Sink> = if output != "" {
      Box::new(FileArraySink::new(&output.split(";").collect()).unwrap())
  } else {
      Box::new(StdOutSink::new())
  };

  let mut input_source : Box<Source> = if input != "" {
      Box::new(FileSource::new(&input).unwrap())
  }else{
      Box::new(StdInSource::new())
  };

  let mut iteration = 0;
  let mut progressMonitor : Box<ProgressMonitor> = match input_source.size() {
      Err(_) => Box::new(DummyProgressMonitor::new()),
      Ok(size) => Box::new(ProgressBarProgressMonitor::new(size)),
      };

  let mut len = input_source.read(&mut buf).expect("Failed initial read");
  while len > 0 {

    // Do fs sync
    
    if iteration % 500 == 0{
      output_sink.force_sync().expect("Failed to do OS sync of output stream");
      if !quiet {
        progressMonitor.setProgress((iteration * blocksize + len) as u64);
      }
    }
    output_sink.write(&buf[..len]).expect("Write failure");
    len = input_source.read(&mut buf).expect("Read failure");
    iteration += 1;
  }
}
