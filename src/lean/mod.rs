use super::hff::quote::Bar;

use std::error::Error;
use csv;
use std::fs;
use std::io;
use std::fs::DirEntry;
use std::path::{Path, PathBuf};
use std::ffi::OsStr;
use zip::ZipArchive;
use std::io::Read;
use chrono::Utc;
use chrono::DateTime;
use chrono::LocalResult;
use chrono::TimeZone;
use core::slice::Iter;

pub struct DayBars {
    daypaths: Vec<PathBuf>,
    iter: usize,
}


impl DayBars {

    pub fn empty() -> Self {
        let dta = vec!();
        Self::new(dta)
    }

    pub fn new(dayPaths: Vec<PathBuf>) -> Self {
        Self {
            daypaths: dayPaths.to_owned(),
            iter: 0,
        }
    }
    pub fn next_day(&mut self) -> Option<(LocalResult<DateTime<Utc>>, Vec<Bar>)> {
        let entry = self.daypaths.get(self.iter);
        self.iter = self.iter + 1;
        if entry.is_some() {
            let path_buf = entry.unwrap();
            if let Some("zip") = path_buf.as_path().extension().and_then(OsStr::to_str) {
                //entry.map(|e| e.as_path().extension().and_then(OsStr::to_str)) {
                let year = path_buf.file_stem().unwrap().to_str().unwrap()[0..4].parse::<i32>().unwrap();
                let month = path_buf.file_stem().unwrap().to_str().unwrap()[4..6].parse::<u32>().unwrap();
                let day = path_buf.file_stem().unwrap().to_str().unwrap()[6..8].parse::<u32>().unwrap();
                let date = Utc.with_ymd_and_hms(year, month, day, 0, 0, 0);
                if let Ok(data) = Lean::readZipStuff(&entry.unwrap()) {
                    return Some((date,data));
                }
            } else {
                return self.next_day();
            }
        }
        None
    }
}


pub struct Lean {
    pub dir: String,
}

impl Lean {

    fn listDir(&self, target: &String) -> Result<Vec<PathBuf>, io::Error> {

        let mut entries = fs::read_dir(target.clone())?
        .map(|res| res.map(|e| e.path()))
        .collect::<Result<Vec<_>, io::Error>>()?;
        entries.sort();
        Ok(entries)
    }

    pub fn list_entries(&self, target: &String) -> DayBars {
        let dir = format!("{}/{}", self.dir, target);
        //eprintln!("reading dir {}", dir);
        let entries = self.listDir(&dir.to_string());
        //eprintln!("entries: {:?}", entries);

        DayBars::new(entries.unwrap())

        /*
        entries.map(|e| {
            e.iter().map(|pb| pb.as_path().)
            DayBars::new(&e)
        }).unwrap_or(DayBars::empty())
        */
    }

    pub fn readZipStuff(path: &Path)  -> Result<Vec<Bar>,Box<dyn Error>>{
        let zipfile = std::fs::File::open(path).unwrap();
        let mut archive = zip::ZipArchive::new(zipfile).unwrap();
        let mut file = archive.by_index(0)?;
        //eprintln!("Filename: {}", file.name());

        let mut contents = String::new();
        file.read_to_string(&mut contents).unwrap();
        let mut reader = csv::ReaderBuilder::new().has_headers(false).from_reader(contents.as_bytes());
        let mut vec: Vec<Bar> = Vec::new();

        for result in reader.deserialize() {
            let record: Bar = result?;
            //println!("{}\t{}", record.t, record.c);
            vec.push(record);
        }
        Ok(vec)
    }
}