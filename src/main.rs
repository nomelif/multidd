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
use std::io::ErrorKind;
use std::time::Instant;
use std::thread;


trait Sink{
  fn start(&mut self) -> ();
  fn wait(&mut self) -> ();
}

trait Source : Send{
  fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize>;
  fn size(&mut self) -> std::io::Result<u64>;
  fn blocksize(&self) -> u64;
}

// Source that can be split into many parts for parallell consumption

trait SplittableSource : Source{
  fn split(&mut self) -> Self;
}

// Struct to read data from a file

struct FileSource {file : File, path : String, blocksize : u64}

impl FileSource {
  fn new(string : &str, blocksize : u64) -> Result<FileSource, String>{
    match File::open(&string) {
      Ok(x) => Ok(FileSource{file : x, path : string.to_string(), blocksize : blocksize}),
      Err(_) => Err(format!("Failed to open {} for reading", string)),
    }
  }
}

impl SplittableSource for FileSource {
  fn split(&mut self) -> Self{
    FileSource::new(&self.path, self.blocksize).unwrap()
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
  fn blocksize(&self) -> u64 { self.blocksize }
}

// Struct to read data from stdin

struct StdInSource{stdin : std::io::Stdin, blocksize : u64}

impl StdInSource{
  fn new(blocksize : u64) -> StdInSource{
    StdInSource{stdin : std::io::stdin(), blocksize : blocksize}
  }
}

impl Source for StdInSource{
  fn read(&mut self, buf : &mut [u8]) -> std::io::Result<usize>{
      self.stdin.read(buf)
  }

  fn size(&mut self) -> std::io::Result<u64>{
    Err(std::io::Error::new(ErrorKind::Other, "Stream size undefined"))
  }
  fn blocksize(&self) -> u64 { self.blocksize }
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
  fn start(&mut self) {}
  fn wait(&mut self) {}
}

// Struct to pipe data out to stdio

struct StdOutSink{stdout : std::io::Stdout, source : Box<Source>, handle : Option<Box<thread::JoinHandle<Box<FnOnce()>>>>} 
impl StdOutSink{
  fn new(source : Box<Source>) -> StdOutSink{
    StdOutSink{stdout : std::io::stdout(), source : source, handle : None}
  }
}

impl Sink for StdOutSink{
  fn start(&mut self) {
    let handle = thread::spawn(|| {
    let mut buf = vec![0u8;self.source.blocksize() as usize];
    /*let mut len = self.source.read(&mut buf).expect("Failed initial read");
    let mut iteration = 0;
    while len > 0 {
      self.stdout.write_all(&buf[..len]).expect("Write failure");
      len = self.source.read(&mut buf).expect("Read failure");
      iteration += 1;
    }*/
    });
  }

  fn wait(&mut self) {}
}

trait ProgressMonitor {
    fn set_progress(&mut self, progress : u64) -> ();
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
  fn set_progress(&mut self, progress : u64) -> () {
    self.pb.set(progress);
  }
}


struct IndeterminateProgressMonitor { start_time : Instant }

impl IndeterminateProgressMonitor {
  fn new() -> IndeterminateProgressMonitor{
    IndeterminateProgressMonitor {start_time : Instant::now()}
  }
}

impl ProgressMonitor for IndeterminateProgressMonitor{
  fn set_progress(&mut self, progress : u64) -> () {
    let elapsed = self.start_time.elapsed();
    let elapsed_millis = elapsed.as_secs()*1000 + (elapsed.subsec_millis() as u64);
    let progress_mib : f32 = (progress as f32 / (1024*1024) as f32) as f32;
    if elapsed_millis != 0 {
      eprint!("{} MiB; {} MiB/s\r", progress_mib, progress_mib / (elapsed_millis as f32 / 1000.));
    }else{
      eprint!("{} MiB; -- MiB/s\r", progress_mib);
    }
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

  let mut input_source : Box<Source> = if input != "" {
      Box::new(FileSource::new(&input, blocksize as u64).unwrap())
  }else{
      Box::new(StdInSource::new(blocksize as u64))
  };

  let mut output_sink : Box<Sink> = if output != "" {
      Box::new(FileArraySink::new(&output.split(";").collect()).unwrap())
  } else {
      Box::new(StdOutSink::new(input_source))
  };

  /*let mut iteration = 0;
  let mut progress_monitor : Box<ProgressMonitor> = match input_source.size() {
      Err(_) => Box::new(IndeterminateProgressMonitor::new()),
      Ok(size) => Box::new(ProgressBarProgressMonitor::new(size)),
      };*/

  output_sink.start();

}
