use serde::{Deserialize, Serialize};

use super::super::{Gear, GearRange};
use super::account::OrderFill;
use super::quote::Tick;
use std::collections::HashMap;
use std::error::Error;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub enum GAgent {
    OHLC {
        open: f64,
        high: f64,
        low: f64,
        close: f64,
        scale: f64,
        exposure: f64,
        target: Option<f64>,
    },
    // Coastline trader agent with parameters as defined in golang
    CL {
        direction: i64,
        price: f64,
        scale: f64,
        size: f64,
        i0: Option<f64>,
        imax: f64,
        target: Option<f64>,
    },
    Symmetric {
        pmid: f64,
        span: f64,
        scale: f64,
        exposure: f64,
        target: f64,
    },
    Buy {
        price0: f64,
        price1: f64,
        scale: f64,
        exposure: f64,
    },
    Sell {
        price0: f64,
        price1: f64,
        scale: f64,
        exposure: f64,
    },
    JumpLong {
        price0: f64,
        scale: f64,
        exposure: f64,
    },
    // Coastline trader agent with parameters as defined in golang
    Coastline {
        direction: i64,
        price0: f64,
        scale: f64,
        size: f64,
        imax: f64,
    },
    Segment {
        price0: f64,
        exposure0: f64,
        pricen: f64,
        exposuren: f64,
        scale: f64,
        target: f64,
    },
}

impl GAgent {
    pub fn build(&self) -> Option<GearHedger> {
        match self {
            GAgent::OHLC {
                open: open,
                high: high,
                low: low,
                close: close,
                scale: scale,
                exposure: exposure,
                target: target,
            } => {
                // price to zero exposure
                let zerop = if open < close {close} else {open};
                // check where to set exposure at extremes
                let exposure0 = exposure.min(exposure * (zerop - low) / (high - zerop));
                let exposuren = - exposure.min(exposure * (high - zerop) / (zerop - low));
                let actualTarget = target.unwrap_or(f64::MAX);
                Some(GearHedger::segment(
                        *low, exposure0, *high, exposuren, *scale, actualTarget,
            ))
            },
            GAgent::CL {
                direction: direction,
                price: price,
                scale: scale,
                size: size,
                i0: i0,
                imax: imax,
                target: target,
            } => {
                let shift = i0.unwrap_or(1.0) * *scale;
                let zerop = if *direction > 0 { *price + shift} else { *price - shift };

                let high = zerop + imax * scale;
                let low = zerop - imax * scale;

                let actualTarget = target.unwrap_or(size * scale);
                let exposure = *size * *imax;

                Some(GearHedger::segment(
                        low, exposure, high, -exposure, *scale, actualTarget,
            ))
            },
            GAgent::Symmetric {
                pmid: pmid,
                span: span,
                scale: scale,
                exposure: exposure,
                target: target,
            } => Some(GearHedger::symmetric(
                    *pmid - *span,
                *pmid + *span,
                *scale,
                *scale,
                *exposure,
                *target,
            )),
            GAgent::Buy {
                price0: price0,
                price1: price1,
                scale: scale,
                exposure: exposure,
            } => Some(GearHedger::buyer(
                    *price0, *price1, *scale, *scale, *exposure,
            )),
            GAgent::Sell {
                price0: price0,
                price1: price1,
                scale: scale,
                exposure: exposure,
            } => Some(GearHedger::seller(
                    *price0, *price1, *scale, *scale, *exposure,
            )),
            GAgent::JumpLong {
                price0: price0,
                scale: scale,
                exposure: exposure,
            } => Some(GearHedger::jump(
                    *price0, 1.0, 0.0, *scale, *scale, *exposure,
            )),
            GAgent::Coastline {
                direction: direction,
                price0: price0,
                scale: scale,
                size: size,
                imax: imax,
            } => Some(GearHedger::coastline(
                    *direction, *price0, *scale, *size, *imax,
            )),
            GAgent::Segment {
                price0: price0,
                exposure0: exposure0,
                pricen: pricen,
                exposuren: exposuren,
                scale: scale,
                target: target,
            } => Some(GearHedger::segment(
                    *price0, *exposure0, *pricen, *exposuren, *scale, *target,
            )),
            _ => None,
        }
    }
}
pub trait Agent {

    fn close(&mut self, tick :&Tick) -> i64;
    // active status
    fn is_active(&self) -> bool;
    fn deactivate(&mut self);

    // computes the status of the Agent: should it be closed
    fn to_be_closed(&self) -> bool;

    // actions to be done if PL is reaching target
    fn target_action(&mut self) -> i64;

    // target_exposure
    fn target_exposure(&mut self, tick: &Tick) -> i64;

    // compute the agent exposure if trading this tick
    fn next_exposure(&mut self, tick: &Tick) -> i64;

    //
    /*
    fn next_state<F>(&mut self, tick: &Tick, f: F) -> i64
    where F: FnMut(&mut Self) -> i64;
    */
    // compute the new state after trading occured with a target exposure and Order fill at a price
    fn update_on_fill(&mut self, order_fill: &OrderFill);

    // set the next exposure and execute the order fill at the same time and price
    fn next_exposure_and_fill(&mut self, order_fill: &OrderFill);

    // current exposure of the agent
    fn exposure(&self) -> i64;
}

/**
 A Hedger agent will buy and sell at price levels scale away from previous trade
 Following an exposure determined by a GearFunction and an exposure limit
 below preset limits.
***/
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct GearHedger {
    // static parameters of the Hedge
    pub max_exposure: f64,
    pub gear_f: Gear,
    // steps on the grid
    pub scaleUp: f64,
    pub scaleDown: f64,

    // activation status and PL target
    pub active: bool,
    pub target: f64,

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

    /** method used to merge 2 GearHedger
    */
    pub fn merge_flat(& self, other: &GearHedger) -> Self {
        // compute the price range
        let p_0 = self.gear_f.p_0.min(other.gear_f.p_0);
        let p_n = self.gear_f.p_n.max(other.gear_f.p_n);

        // compute the exposure range, then resulting gear and max_exposure
        let low_gear = self.gear_f.g_0 * self.max_exposure + other.gear_f.g_0 * other.max_exposure;
        let high_gear = self.gear_f.g_n * self.max_exposure + other.gear_f.g_n * other.max_exposure;

        // highest of the absolute gears
        let max_exposure = low_gear.abs().max(high_gear.abs());
        let g_0 = low_gear / max_exposure;
        let g_n = high_gear / max_exposure;

        let gear = Gear::segment(p_0, g_0, p_n, g_n);
        // lets trade at the average scales
        let scaleUp = (self.scaleUp + other.scaleUp) / 2.0;
        let scaleDown = (self.scaleDown + other.scaleDown) / 2.0;
        let scale = (scaleUp + scaleDown) / 2.0;

        // what the hell will be the target?
        // At least the sum of the two...plus the accumulated losses
        let target = self.target + other.target - self.agentPL.cum_profit - other.agentPL.cum_profit;
        // how much has been realized: buy-sell net * price difference...
        // if the exposures are different signs, then we are realizing some pl
        let mut agent: GearHedger = GAgent::Segment { price0: p_0, exposure0: low_gear, pricen: p_n, exposuren: high_gear, scale: scale, target: target }.build().unwrap();
        agent.next_exposure_and_fill(&OrderFill { price: self.agentPL.price_average, units: self.agentPL.exposure });
        agent.next_exposure_and_fill(&OrderFill { price: other.agentPL.price_average, units: other.agentPL.exposure });

        agent.active = true;

        return agent;
    }

    pub fn buyer(
        price0: f64,
        price1: f64,
        scaleUp: f64,
        scaleDown: f64,
        max_exposure: f64,
    ) -> Self {
        Self {
            max_exposure: max_exposure,
            gear_f: Gear::positive(price0, price1),
            scaleUp: scaleUp,
            scaleDown: scaleDown,

            active: true,
            target: f64::MAX,

            lastTradePrice: price1,
            nextBuyPrice: price1,
            nextSellPrice: price1,

            agentPL: AgentPL {
                exposure: 0,
                price_average: 0.0,
                cum_profit: 0.0,
                unrealized_pl: 0.0,
            },
            tentative_price: price1,
            tentative_exposure: 0,
        }
    }

    pub fn seller(
        price0: f64,
        price1: f64,
        scaleUp: f64,
        scaleDown: f64,
        max_exposure: f64,
    ) -> Self {
        Self {
            max_exposure: max_exposure,
            gear_f: Gear::negative(price0, price1),
            scaleUp: scaleUp,
            scaleDown: scaleDown,

            active: true,
            target: f64::MAX,

            lastTradePrice: price0,
            nextBuyPrice: price0,
            nextSellPrice: price0,

            agentPL: AgentPL {
                exposure: 0,
                price_average: 0.0,
                cum_profit: 0.0,
                unrealized_pl: 0.0,
            },
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

            active: true,
            target: f64::MAX,

            lastTradePrice: 1.0,
            nextBuyPrice: 1.0,
            nextSellPrice: 1.0,

            agentPL: AgentPL {
                exposure: 0,
                price_average: 0.0,
                cum_profit: 0.0,
                unrealized_pl: 0.0,
            },
            tentative_price: 1.0,
            tentative_exposure: 0,
        }
    }

    pub fn symmetric(
        price0: f64,
        price1: f64,
        scaleUp: f64,
        scaleDown: f64,
        max_exposure: f64,
        target: f64,
    ) -> Self {
        let zero_price = (price0 + price1) / 2.0;
        Self {
            max_exposure: max_exposure,
            gear_f: Gear::symmetric(price0, price1),
            scaleUp: scaleUp,
            scaleDown: scaleDown,

            active: true,
            target: target,

            lastTradePrice: zero_price,
            nextBuyPrice: zero_price,
            nextSellPrice: zero_price,

            agentPL: AgentPL {
                exposure: 0,
                price_average: 0.0,
                cum_profit: 0.0,
                unrealized_pl: 0.0,
            },
            tentative_price: zero_price,
            tentative_exposure: 0,
        }
    }
    pub fn jump(
        price0: f64,
        g_0: f64,
        g_1: f64,
        scaleUp: f64,
        scaleDown: f64,
        max_exposure: f64,
    ) -> Self {
        Self {
            max_exposure: max_exposure,
            gear_f: Gear::jump(price0, g_0, g_1),
            scaleUp: scaleUp,
            scaleDown: scaleDown,

            active: true,
            target: f64::MAX,

            lastTradePrice: price0,
            nextBuyPrice: price0,
            nextSellPrice: price0,

            agentPL: AgentPL {
                exposure: 0,
                price_average: 0.0,
                cum_profit: 0.0,
                unrealized_pl: 0.0,
            },
            tentative_price: price0,
            tentative_exposure: 0,
        }
    }

    pub fn coastline(direction: i64, price0: f64, scale: f64, size: f64, imax: f64) -> Self {
        Self {
            max_exposure: size * imax,
            gear_f: Gear::coastline(direction, price0, scale, imax),
            scaleUp: scale,
            scaleDown: scale,

            active: true,
            target: scale * size,

            lastTradePrice: price0,
            nextBuyPrice: price0,
            nextSellPrice: price0,

            agentPL: AgentPL {
                exposure: 0,
                price_average: 0.0,
                cum_profit: 0.0,
                unrealized_pl: 0.0,
            },
            tentative_price: price0,
            tentative_exposure: 0,
        }
    }
    pub fn segment(
        price0: f64,
        exposure0: f64,
        pricen: f64,
        exposuren: f64,
        scale: f64,
        target: f64,
    ) -> Self {
        let (g_0, g_1) = if exposure0.abs() > exposuren.abs() {
            (1.0 * exposure0.signum(), exposuren / exposure0.abs())
        } else {
            (exposure0 / exposuren.abs(), 1.0 * exposuren.signum())
        };
        let max_exposure = exposure0.abs().max(exposuren.abs());

        Self {
            max_exposure: max_exposure,
            gear_f: Gear::segment(price0, g_0, pricen, g_1),
            scaleUp: scale,
            scaleDown: scale,

            active: true,
            target: target,

            lastTradePrice: price0,
            nextBuyPrice: price0,
            nextSellPrice: price0,

            agentPL: AgentPL {
                exposure: 0,
                price_average: 0.0,
                cum_profit: 0.0,
                unrealized_pl: 0.0,
            },
            tentative_price: price0,
            tentative_exposure: 0,
        }
    }
}

impl Agent for GearHedger {

    fn close(&mut self, tick :&Tick) -> i64 {
        // otherwize,we check if we need to adjust exposure
        if self.agentPL.exposure > 0 {
            self.tentative_price = tick.bid;
        } else {
            self.tentative_price = tick.ask;
        }
        self.tentative_exposure = 0;
        0
    }

    // is active status
    fn is_active(&self) -> bool {
        self.active
    }
    fn deactivate(&mut self) {
        self.active = false;
    }

    // at the moment we never close, we need to add a way to add a delegate to decide closing of Agents
    fn to_be_closed(&self) -> bool {
        self.agentPL.cum_profit > self.target
        //false
    }

    // trivialm as GearHedger have an AgentPL
    fn exposure(&self) -> i64 {
        self.agentPL.exposure
    }

    fn target_action(&mut self) -> i64 {
        self.tentative_exposure = 0;
        self.deactivate();
        return 0;
    }

    fn target_exposure(&mut self, tick: &Tick) -> i64 {
        // otherwize,we check if we need to adjust exposure
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
        // deal with a profit above target
        // we will trade to set exposure to zero and deactivate the agent.
        // TODO : call a closure defining the behaviour of the agent
        // default would be to deactivate the agent
        let close_price = if self.exposure() > 0 {
            tick.bid
        } else {
            tick.ask
        };
        if self.agentPL.pl_at_price(close_price) > self.target {
            self.tentative_price = close_price;
            self.tentative_exposure = 0;
            let e = self.target_action();
            return e;
        }
        self.target_exposure(tick)
    }

    fn next_exposure_and_fill(&mut self, order_fill: &OrderFill) {
        self.tentative_price = order_fill.price;
        self.tentative_exposure = self.tentative_exposure + order_fill.units;
        self.update_on_fill(order_fill);
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
        if self.to_be_closed() {
            self.deactivate()
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct AgentPL {
    // exposure: signed position in integral units
    pub exposure: i64,
    // average price of position
    pub price_average: f64,
    // cumulated profit (Estimated)
    pub cum_profit: f64,
    // cumulated profit (Actual)
    pub unrealized_pl: f64,
}

#[derive(Debug, Deserialize, Serialize)]
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
    //
    //    pub fn deactivate(&mut self, key: &String) {
    //        self.agents.iter_mut().filter(|a| a.0 == key).map(|a| a.1.deactivate());
    //        ()
    //    }
}

impl<T: Agent> Agent for AgentInventory<T> {

    fn close(&mut self, tick :&Tick) -> i64 {
        for (_, val) in self.agents.iter_mut() {
            val.close(tick);
        }
        0
    }

    fn is_active(&self) -> bool {
        true
    }
    fn deactivate(&mut self) {
        for (_, val) in self.agents.iter_mut() {
            val.deactivate();
        }
    }

    fn to_be_closed(&self) -> bool {
        false
    }

    fn exposure(&self) -> i64 {
        self.agents
            .iter()
            .filter(|a| a.1.is_active())
            .fold(0, |a, b| a + b.1.exposure())
    }

    // we do nothing, it only happens on each individual Agent of the inventory
    fn target_action(&mut self) -> i64 {
        0
    }

    // we do nothing, it only happens on each individual Agent of the inventory
    fn target_exposure(&mut self, tick: &Tick) -> i64 {
        0
    }


    fn next_exposure(&mut self, tick: &Tick) -> i64 {
        let mut exposure = 0;
        for (_, val) in self.agents.iter_mut().filter(|a| a.1.is_active()) {
            exposure = exposure + val.next_exposure(tick);
        }
        exposure
    }

    fn update_on_fill(&mut self, order_fill: &OrderFill) {
        for (_, val) in self.agents.iter_mut().filter(|a| a.1.is_active()) {
            val.update_on_fill(order_fill);
        }
    }
    fn next_exposure_and_fill(&mut self, order_fill: &OrderFill) {
        self.next_exposure(&Tick{bid: order_fill.price, ask: order_fill.price, time: 0});
        self.update_on_fill(order_fill);
    }
}

impl AgentPL {
    // total_profit compute the Process total profit for a given exit price
    pub fn total_profit(&mut self, x: f64) -> f64 {
        self.unrealized_pl = (self.exposure as f64) * (x / self.price_average - 1.0);
        self.unrealized_pl + self.cum_profit
    }

    pub fn pl_at_price(&self, x: f64) -> f64 {
        self.cum_profit + (self.exposure as f64) * (x / self.price_average - 1.0)
    }

    pub fn uPL(&self, x: f64) -> f64 {
        (self.exposure as f64) * (x / self.price_average - 1.0)
    }

    // IncreaseBy a number of units (positive on Long exposure, negative on Short exposure)
    pub fn increase_by(&mut self, x: f64, units: i64) {
        let de = units;
        let e = self.exposure + de;
        let a = (self.price_average * self.exposure.abs() as f64 + x * de.abs() as f64)
            / e.abs() as f64;
        self.exposure = e;
        self.price_average = a;
        self.unrealized_pl = self.exposure as f64 * (x / self.price_average - 1.0);
    }

    // DecreaseBy a number of Units (positive on Long exposure, negative on Short exposure)
    pub fn decrease_by(&mut self, x: f64, units: i64) {
        let de = units;
        let e = self.exposure - de;
        let pi = de as f64 * (x / self.price_average - 1.0);

        self.exposure = e;
        self.cum_profit += pi;
        self.unrealized_pl = self.exposure as f64 * (x / self.price_average - 1.0);
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
    use super::super::account::OrderFill;
    use super::super::quote::Tick;
    use super::GAgent;
    use super::{Agent, GearHedger};

    #[test]
    fn exploration() {
        assert_eq!(2 + 2, 4);
    }

    #[test]
    fn symetric() {
        let mut gear = GearHedger::symmetric(0.80, 1.20, 0.0010, 0.0010, 100000.0, 100000.0);

        gear.next_exposure(&Tick {
            time: 0,
            bid: 0.7000,
            ask: 0.7001,
        });
        gear.update_on_fill(&OrderFill {
            price: gear.tentative_price,
            units: gear.tentative_exposure,
        });
        assert_eq!(gear.exposure(), gear.max_exposure as i64);

        gear.next_exposure(&Tick {
            time: 0,
            bid: 1.0000,
            ask: 1.0000,
        });
        gear.update_on_fill(&OrderFill {
            price: gear.tentative_price,
            units: gear.tentative_exposure,
        });
        assert_eq!(gear.exposure(), 0);

        gear.next_exposure(&Tick {
            time: 0,
            bid: 1.2000,
            ask: 1.2000,
        });
        gear.update_on_fill(&OrderFill {
            price: gear.tentative_price,
            units: gear.tentative_exposure,
        });
        assert_eq!(gear.exposure(), -gear.max_exposure as i64);
    }

    #[test]
    fn target() {
        let mut agent = GAgent::Segment {
            price0: 1.0010,
            exposure0: 100000.0,
            pricen: 1.0210,
            exposuren: -100000.0,
            scale: 0.0010,
            target: 10.0,
        }
        .build()
        .unwrap();
        let tick_0 = Tick {
            time: 0,
            bid: 1.0100,
            ask: 1.0100,
        };
        agent.next_exposure(&tick_0);
        let orderFill_0 = OrderFill {
            price: agent.tentative_price,
            units: agent.tentative_exposure,
        };
        agent.update_on_fill(&orderFill_0);

        let tick_1 = Tick {
            time: 0,
            bid: 1.0120,
            ask: 1.0120,
        };
        agent.next_exposure(&tick_1);
        let orderFill_1 = OrderFill {
            price: agent.tentative_price,
            units: agent.tentative_exposure,
        };
        agent.update_on_fill(&orderFill_1);

        println!("{:?}", agent);
       // assert_eq!(agent.agentPL.cum_profit, 0.0);
       // assert_eq!(agent.exposure(), 10000);
    }
}
