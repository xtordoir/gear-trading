use serde::{Serialize,Deserialize};


use std::error::Error;
use std::collections::HashMap;
use super::quote::Tick;
use super::account::OrderFill;
use super::super::{Gear, GearRange};

#[derive(Debug,Deserialize,Serialize, Clone)]
pub enum GAgent {
    Buy{price0: f64, price1: f64, scale: f64, exposure: f64},
    Sell{price0: f64, price1: f64, scale: f64, exposure: f64},
    JumpLong{price0: f64, scale: f64, exposure: f64},
}

impl GAgent {
    pub fn build(&self) -> Option<GearHedger> {

        match self {
            GAgent::Buy{price0: price0, price1: price1, scale: scale, exposure: exposure} => Some(GearHedger::buyer(*price0, *price1, *scale, *scale, *exposure)),
            GAgent::Sell{price0: price0, price1: price1, scale: scale, exposure: exposure} => Some(GearHedger::seller(*price0, *price1, *scale, *scale, *exposure)),
            GAgent::JumpLong { price0: price0, scale: scale, exposure: exposure } => Some(GearHedger::jump(*price0, 1.0, 0.0, *scale, *scale, *exposure)),
            _ => None,
        }
    }
}
pub trait Agent {

    // computes the status of the Agent: should it be closed
    fn to_be_closed(&self) -> bool;

    // compute the agent exposure if trading this tick
    fn next_exposure(&mut self, tick: &Tick) -> i64;

    // compute the new state after trading occured with a target exposure and Order fill at a price
    fn update_on_fill(&mut self, order_fill: &OrderFill);

    // current exposure of the agent
    fn exposure(&self) -> i64;
}

/**
 A Hedger agent will buy and sell at price levels scale away from previous trade
 Following an exposure determined by a GearFunction and an exposure limit
 below preset limits.
***/
#[derive(Debug,Deserialize,Serialize, Clone)]
pub struct GearHedger {

    // static parameters of the Hedge
    pub max_exposure: f64,
    pub gear_f: Gear,
    // steps on the grid
    pub scaleUp:  f64,
    pub scaleDown:  f64,

    // next trades on the buy and sell sides
    pub lastTradePrice: f64,
    pub nextBuyPrice: f64,
    pub nextSellPrice: f64,

    // PL computer
    pub agentPL: AgentPL,

    //these fields are used when next exposure is computed before requesting an actual trade on the market
    pub tentative_price: f64,
    pub tentative_exposure: i64,

 }

impl GearHedger {

    pub fn buyer(price0: f64, price1: f64, scaleUp: f64, scaleDown: f64, max_exposure: f64) -> Self {
        Self {
            max_exposure: max_exposure,
            gear_f: Gear::positive(price0, price1),
            scaleUp: scaleUp,
            scaleDown: scaleDown,

            lastTradePrice: price1,
            nextBuyPrice: price1,
            nextSellPrice: price1,

            agentPL: AgentPL { exposure: 0, price_average: 0.0, cum_profit: 0.0, actual_cum_profit: 0.0 },
            tentative_price: price1,
            tentative_exposure: 0,
        }
    }

    pub fn seller(price0: f64, price1: f64, scaleUp: f64, scaleDown: f64, max_exposure: f64) -> Self {
        Self {
            max_exposure: max_exposure,
            gear_f: Gear::negative(price0, price1),
            scaleUp:  scaleUp,
            scaleDown:  scaleDown,

            lastTradePrice: price0,
            nextBuyPrice: price0,
            nextSellPrice: price0,

            agentPL: AgentPL { exposure: 0, price_average: 0.0, cum_profit: 0.0, actual_cum_profit: 0.0 },
            tentative_price: price0,
            tentative_exposure: 0,
        }
    }

    pub fn constant(exposure: f64) -> Self {
        Self {
            max_exposure: exposure.abs(),
            gear_f: Gear::constant(exposure as i64),
            scaleUp: 1.0,
            scaleDown: 1.0,

            lastTradePrice: 1.0,
            nextBuyPrice: 1.0,
            nextSellPrice: 1.0,

            agentPL: AgentPL { exposure: 0, price_average: 0.0, cum_profit: 0.0, actual_cum_profit: 0.0 },
            tentative_price: 1.0,
            tentative_exposure: 0,
        }
    }

    pub fn symmetric(price0: f64, price1: f64, scaleUp: f64, scaleDown: f64, max_exposure: f64) -> Self {
        let zero_price = (price0 + price1)/2.0;
        Self {
            max_exposure: max_exposure,
            gear_f: Gear::symmetric(price0, price1),
            scaleUp:  scaleUp,
            scaleDown:  scaleDown,

            lastTradePrice: zero_price,
            nextBuyPrice: zero_price,
            nextSellPrice: zero_price,

            agentPL: AgentPL { exposure: 0, price_average: 0.0, cum_profit: 0.0, actual_cum_profit: 0.0 },
            tentative_price: zero_price,
            tentative_exposure: 0,
        }
    }
    pub fn jump(price0: f64, g_0: f64, g_1: f64, scaleUp: f64, scaleDown: f64, max_exposure: f64) -> Self {
        Self {
            max_exposure: max_exposure,
            gear_f: Gear::jump(price0, g_0, g_1),
            scaleUp:  scaleUp,
            scaleDown:  scaleDown,

            lastTradePrice: price0,
            nextBuyPrice: price0,
            nextSellPrice: price0,

            agentPL: AgentPL { exposure: 0, price_average: 0.0, cum_profit: 0.0, actual_cum_profit: 0.0 },
            tentative_price: price0,
            tentative_exposure: 0,
        }
    }
}

impl Agent for GearHedger {

    // at the moment we never close, we need to add a way to add a delegate to decide closing of Agents
    fn to_be_closed(&self) -> bool {
        false
    }

    // trivialm as GearHedger have an AgentPL
    fn exposure(&self) -> i64 {
        self.agentPL.exposure
    }

    // BEWARE THIS IS BASED ON STRONG ASSUPTION
    // THAT WE ONLY SELL ON PRICE UP
    // AND BUY ON PRICE DOWN
    // WHICH IS ECONOMICALLY THE MOST SENSIBLE WAY, BUT...
    // TODO:
    // Check if the current tick entails a buy or a sale
    // set the tentative price and exposure accordingly
    // We should not have nextSell/nextBuyPrice but nextTradeBelow/nextTradeAbovePrice
    // We should make user there is NEVER two different trades on bid and ask of a single tick and gear function
    // thus we only trade if bid and ask entails the same direction of trade (buy or sell) and pick the
    // right of the two
    fn next_exposure(&mut self, tick: &Tick) -> i64 {
        if tick.bid >= self.nextSellPrice {
            self.tentative_price = tick.bid;
            self.tentative_exposure = (self.gear_f.g(tick.bid) * self.max_exposure) as i64;
            //(size * (self.price0 - tick.bid)/self.scale).round() as i64;
            self.tentative_exposure
        } else if tick.ask <= self.nextBuyPrice {
            self.tentative_price = tick.ask;
            self.tentative_exposure = (self.gear_f.g(tick.ask) * self.max_exposure) as i64;
            //(self.size as f64 * (self.price0 - tick.ask)/self.scale).round() as i64;
            self.tentative_exposure
        } else {
            self.agentPL.exposure
        }
    }

    fn update_on_fill(&mut self, order_fill: &OrderFill) {
        let traded = self.tentative_exposure - self.agentPL.exposure;
        if traded < 0 {
            self.agentPL.sell(order_fill.price, traded.abs());
            self.lastTradePrice = order_fill.price;
            self.nextSellPrice = order_fill.price + self.scaleUp;
            self.nextBuyPrice = order_fill.price - self.scaleDown;
        } else if traded > 0 {
            self.agentPL.buy(order_fill.price, traded.abs());
            self.lastTradePrice = order_fill.price;
            self.nextBuyPrice = order_fill.price - self.scaleDown;
            self.nextSellPrice = order_fill.price + self.scaleUp;
        }
    }

}




#[derive(Debug,Deserialize,Serialize, Clone)]
pub struct AgentPL {
    // exposure: signed position in integral units
    pub exposure: i64,
    // average price of position
    pub price_average: f64,
    // cumulated profit (Estimated)
    pub cum_profit: f64,
    // cumulated profit (Actual)
    pub actual_cum_profit: f64,
}


#[derive(Debug,Deserialize,Serialize)]
pub struct AgentInventory<T: Agent> {
    pub agents: HashMap<String, T>,
    pub pl: f64,
}
impl<T: Agent> AgentInventory<T> {
    pub fn new() -> Self {
        Self {
            agents: HashMap::new(),
            pl: 0.0,
        }
    }
}

impl<T: Agent> Agent for AgentInventory<T> {

    fn to_be_closed(&self) -> bool {
        false
    }

    fn exposure(&self) -> i64 {
        self.agents.iter().fold( 0, |a, b| a + b.1.exposure())
    }

    fn next_exposure(&mut self, tick: &Tick) -> i64 {
        let mut exposure = 0;
        for (_, val) in self.agents.iter_mut() {
            exposure = exposure + val.next_exposure(tick);
        }
        exposure
    }

    fn update_on_fill(&mut self, order_fill: &OrderFill) {
        for (_, val) in self.agents.iter_mut() {
            val.update_on_fill(order_fill);
        }
    }
}


impl AgentPL {
    // total_profit compute the Process total profit for a given exit price
    pub fn total_profit(&mut self, x: f64) -> f64 {
	    self.actual_cum_profit = self.cum_profit + (self.exposure as f64) * (x /self.price_average - 1.0);
        self.actual_cum_profit
    }

    pub fn pl_at_price(&self, x: f64) -> f64 {
        self.cum_profit + (self.exposure as f64) * (x /self.price_average - 1.0)
    }

    pub fn uPL(&self, x: f64) -> f64 {
        (self.exposure as f64) * (x /self.price_average - 1.0)
    }

    // IncreaseBy a number of units (positive on Long exposure, negative on Short exposure)
    pub fn increase_by(&mut self, x: f64, units: i64) {
        let de = units;
        let e = self.exposure + de;
        let a = (self.price_average * self.exposure.abs() as f64 + x * de.abs() as f64) / e.abs() as f64;
        self.exposure = e;
        self.price_average = a;
    }

    // DecreaseBy a number of Units (positive on Long exposure, negative on Short exposure)
    pub fn decrease_by(&mut self, x:f64, units: i64) {
        let de = units;
        let e = self.exposure - de;
        let pi = de as f64 * (x / self.price_average - 1.0);

        self.exposure = e;
        self.cum_profit += pi;
    }

    pub fn buy(&mut self, x: f64, units: i64) {
        if self.exposure >= 0 {
            // increase long position
            self.increase_by(x, units);
        } else if self.exposure < 0 && units > -self.exposure {
            // decrease short position
            // take the smallest between exposure and sale size
            let delta = units + self.exposure;
            self.decrease_by(x, self.exposure);
            self.increase_by(x, delta);
        } else if self.exposure < 0 {
            self.decrease_by(x, -units);
        }
    }
    pub fn sell(&mut self, x: f64, units: i64) {
        if self.exposure <= 0 {
            // increase long position
            self.increase_by(x, -units);
        } else if self.exposure > 0 && units > self.exposure {
            // decrease short position
            // take the smallest between exposure and sale size
            let delta = units - self.exposure;
            self.decrease_by(x, self.exposure);
            self.increase_by(x, -delta);
        } else if self.exposure > 0 {
            self.decrease_by(x, units);
        }
    }
}


#[cfg(test)]
mod tests {
    use super::{GearHedger, Agent};
    use super::super::quote::Tick;
    use super::super::account::OrderFill;

    #[test]
    fn exploration() {
        assert_eq!(2 + 2, 4);
    }

    #[test]
    fn symetric() {
        let mut gear = GearHedger::symmetric(0.80, 1.20, 0.0010, 100000.0);
        
        gear.next_exposure(&Tick{time:0, bid: 0.7000, ask: 0.7001,});
        gear.update_on_fill(&OrderFill{price: gear.tentative_price, units: gear.tentative_exposure});
        assert_eq!(gear.exposure(), gear.max_exposure as i64);

        gear.next_exposure(&Tick{time:0, bid: 1.0000, ask: 1.0000,});
        gear.update_on_fill(&OrderFill{price: gear.tentative_price, units: gear.tentative_exposure});
        assert_eq!(gear.exposure(), 0);

        gear.next_exposure(&Tick{time:0, bid: 1.2000, ask: 1.2000,});
        gear.update_on_fill(&OrderFill{price: gear.tentative_price, units: gear.tentative_exposure});
        assert_eq!(gear.exposure(), -gear.max_exposure as i64);

    }



}