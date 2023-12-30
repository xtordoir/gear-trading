extern crate gear_trading;

use clap::{arg, command, Parser};
use std::fs;
use gear_trading::hff::agents::*;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Name of the hedger file to merge 2 trades from
    #[arg(short = 'f', long)]
    hedger_file: Option<String>,

    // Name of the first agent to merge
    #[arg(short = 'n', long)]
    name1: String,

    #[arg(short = 'm', long)]
    name2: String,

    #[arg(short = 'o', long)]
    outname: String,
}

fn main() {
    let args = Args::parse();
    let name1: &str = args.name1.as_str();
    let name2: &str = args.name2.as_str();
    let outname = args.outname;

    // read the input inventory file
    let mut hedger = args
        .hedger_file
        .as_deref()
        .map(|f| {
            let hstr = fs::read_to_string(f).ok();
            hstr.map(|s| serde_json::from_str::<AgentInventory<GearHedger>>(s.as_str()).ok())
                .flatten()
        })
        .flatten().unwrap();

    // extract the agent1 and agent2
    let agent1 = hedger.agents.get(name1).unwrap();
    let agent2 = hedger.agents.get(name2).unwrap();

    let agent = agent1.merge_flat(agent2);
    hedger.agents.insert(outname, agent);
    hedger.agents.remove(name1);
    hedger.agents.remove(name2);

    let hedger_str = serde_json::to_string(&hedger).ok().unwrap();
    println!("{}", hedger_str);

}