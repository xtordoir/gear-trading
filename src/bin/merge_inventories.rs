extern crate gear_trading;

use clap::{arg, command, Parser};
use std::fs;
use gear_trading::hff::agents::*;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Name of the hedger file to merge 2 trades from
    #[arg(short = 'f', long)]
    hedger_file1: Option<String>,

    #[arg(short = 'g', long)]
    hedger_file2: Option<String>,
}

fn main() {
    let args = Args::parse();

    // read the input inventory file
    let mut hedger1: AgentInventory<GearHedger> = args
        .hedger_file1
        .as_deref()
        .map(|f| {
            let hstr = fs::read_to_string(f).ok();
            hstr.map(|s| serde_json::from_str::<AgentInventory<GearHedger>>(s.as_str()).ok())
                .flatten()
        })
        .flatten().unwrap();

    // read the second input inventory file
    let mut hedger2 = args
        .hedger_file2
        .as_deref()
        .map(|f| {
            let hstr = fs::read_to_string(f).ok();
            hstr.map(|s| serde_json::from_str::<AgentInventory<GearHedger>>(s.as_str()).ok())
                .flatten()
        })
        .flatten().unwrap();
    
    let _ = hedger2.agents.iter().for_each(|a| {
        let xx = a.1.clone();
        hedger1.agents.insert(String::from(a.0), xx);
    });
    
    let hedger_str = serde_json::to_string(&hedger1).ok().unwrap();
    println!("{}", hedger_str);

}