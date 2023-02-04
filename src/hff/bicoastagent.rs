use super::agents::{GearHedger,Agent, GAgent};
use super::account::OrderFill;
use super::quote::Tick;


/*
BiCoastAgent is a symmetric GearHedger with specifications such that:
- an epoch_target is set as the profit target before we recalibrate the mid price
- when the current tick leads to a cumulative profit + unrealized PL larger than the target,
we add the epoch target to the previous target, and the mid price becomes the current price.

*/
pub struct BiCoastAgent {
    epoch_target: f64,
    gear_hedger: GearHedger,

}

impl BiCoastAgent {

    // constructor
    fn new(price: f64, span: f64, scale: f64, exposure: f64) -> Self {
        Self {
            epoch_target: scale * exposure / span,
            gear_hedger: GAgent::Symmetric{pmid: price, span: span, scale: scale, exposure: exposure}.build().unwrap(),
        }
    }


    fn mid_price(&self) -> f64 {
        (self.gear_hedger.gear_f.p_0 + self.gear_hedger.gear_f.p_n)/2.0
    }

    fn shift_mid_to_price(&mut self, price: f64) {
        let span = self.gear_hedger.gear_f.p_n - self.gear_hedger.gear_f.p_0;
        self.gear_hedger.gear_f =  GAgent::Symmetric{
            pmid: price,
            span: span,
            scale: self.gear_hedger.scaleUp,
            exposure: self.gear_hedger.max_exposure}.build().unwrap().gear_f;
    }
}

impl Agent for BiCoastAgent {
    fn is_active(&self) -> bool {
        true
    }
    fn deactivate(&mut self) {

    }

    // computes the status of the Agent: should it be closed
    fn to_be_closed(&self) -> bool {
        false
    }

    fn target_action(&mut self) -> i64 {
        let price = self.gear_hedger.tentative_price;
        self.gear_hedger.target = self.gear_hedger.target + self.epoch_target;
        self.shift_mid_to_price(price);
        return 0;
    }
    // compute the agent exposure if trading this tick
    fn next_exposure(&mut self, tick: &Tick) -> i64 {
        0
    }

    // compute the new state after trading occured with a target exposure and Order fill at a price
    fn update_on_fill(&mut self, order_fill: &OrderFill) {}

    // current exposure of the agent
    fn exposure(&self) -> i64 {
        0
    }
}