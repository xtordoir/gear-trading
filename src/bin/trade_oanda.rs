extern crate gear_trading;

use clap::{arg, command, Parser};
use std::fs;
use std::{thread, time};

use chrono::DateTime;
use chrono::Utc;
use error_chain::error_chain;
use serde::Deserialize;
use serde_json::json;
use std::env;
//use reqwest::Client;
use gear_trading::hff::account::*;
use gear_trading::hff::agents::*;
use gear_trading::hff::quote::Tick;
use gear_trading::oanda::client::Client;
use gear_trading::oanda::*;
use std::error::Error;
use tokio::main;

/*
error_chain! {
    foreign_links {
        EnvVar(env::VarError);
        HttpRequest(reqwest::Error);
    }
}
*/
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Name of the hedger file
    #[arg(short = 'f', long)]
    hedger_file: Option<String>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();

    //let cp = args.hedgerfile.as_deref();
    let hedger_opt = args
        .hedger_file
        .as_deref()
        .map(|f| {
            let hstr = fs::read_to_string(f).ok();
            hstr.map(|s| serde_json::from_str::<AgentInventory<GearHedger>>(s.as_str()).ok())
                .flatten()
        })
        .flatten();

    if hedger_opt.is_none() {}

    let delay = time::Duration::from_secs(15);
    let mut iter = 0;

    let oanda_url = env::var("OANDA_URL")?;
    let oanda_account = env::var("OANDA_ACCOUNT")?;
    let oanda_api_key = env::var("OANDA_API_KEY")?;

    let client = Client::new(
        oanda_url.clone(),
        oanda_account.clone(),
        oanda_api_key.clone(),
    );
    let mut hedger =
        hedger_opt.unwrap_or_else(|| {
            let mut inventory: AgentInventory<GearHedger> = AgentInventory::new();
            inventory.agents.insert(String::from("shortloser"), GearHedger::symmetric(1.0150, 1.0650, 0.0010, 422500.0));
            inventory
        });
        //GearHedger::symmetric(1.0150, 1.0650, 0.0010, 422500.0));

    let hedger_str = serde_json::to_string(&hedger).ok().unwrap();
    println!("{}", hedger_str);

    //let mut inventory: AgentInventory<GearHedger> = AgentInventory::new();
    //inventory.agents.insert(String::from("shortloser"), hedger.clone());
    // inventory.agents.insert(String::from("bias"), DriftingHedge::new(4.0000, 422500, 2.0000));

    //let inv_str = serde_json::to_string(&inventory).ok().unwrap();
    //println!("{}", inv_str);

    loop {
        // control loop counts and timing
        if iter != 0 {
            thread::sleep(delay);
        }
        iter = iter + 1;
        if iter > 10000 {
            break;
        }

        // get the market tick
        let tick = client
            .get_pricing(String::from("EUR_USD"))
            .await
            .unwrap()
            .get_tick();

        // time now
        let now = Utc::now().timestamp();

        // check account positions
        let positions = client.get_open_positions().await.unwrap().to_position_vec();
        //println!("{:?}", positions);

        // compare target exposure with actual
        let target_exposure = hedger.next_exposure(&tick);
        let account_exposure = positions.first().map_or_else(|| 0, |p| p.units);
        //println!("Target Exposure: {}", target_exposure);
        //println!("Actual Exposure: {}", account_exposure);

        // no trade
        if target_exposure == account_exposure {

            continue;
        }

        // create order
        let order = OrderRequest::new(target_exposure - account_exposure, "EUR_USD".to_string());

        eprintln!("Trading : {} to reach {} at price", target_exposure - account_exposure, target_exposure);

        client.post_order_request(&order).await.map_or(
            eprintln!("Cannot get the Post Order to Oanda, will try again next cycle"),
            |order_fill| {
                order_fill.get_order_fill().map_or(
                    eprintln!("Cannot get the OrderFill from response, will try again next cycle"),
                    |of| {
                        hedger.update_on_fill(&of);
                        let hedger_str = serde_json::to_string(&hedger).ok().unwrap();
                        println!("{}", hedger_str);
                    },
                );
            },
        );


        /*
                order_fill.map(|fill| {
                    hedger.update_on_fill(&fill);
                    let hedger_str = serde_json::to_string(&hedger).ok().unwrap();
                    println!("{}", hedger_str);
                });
        */
        //println!("Agent: {} @ {:.5}", hedger.agentPL.exposure, hedger.agentPL.price_average);
    }

    Ok(())
}
