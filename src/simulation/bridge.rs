// TODO: Remove Clippy suppressions.
use super::state::{Block, Utxo};
use super::util;
use accumulator::group::UnknownOrderGroup;
use accumulator::hash::hash_to_prime;
use accumulator::util::int;
use accumulator::Accumulator;
use multiqueue::{BroadcastReceiver, BroadcastSender};
use rug::Integer;
use std::clone::Clone;
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

#[allow(dead_code)]
#[derive(Clone)]
pub struct Bridge<G: UnknownOrderGroup> {
  utxo_set_product: Integer,
  utxo_set_witness: Accumulator<G>,
  block_height: u64,
}

#[allow(dead_code)]
impl<G: UnknownOrderGroup> Bridge<G> {
  /// Assumes all bridges are online from genesis. We may want to implement syncing later.
  /// Also assumes that bridge/user relationships are fixed
  #[allow(unused_variables)]
  pub fn launch(
    block_receiver: BroadcastReceiver<Block<G>>,
    witness_request_receiver: BroadcastReceiver<(Vec<Utxo>)>, // TODO (type)
    witness_response_senders: HashMap<Uuid, BroadcastSender<(Vec<Accumulator<G>>)>>, // TODO (type)
  ) {
    let bridge = Bridge {
      utxo_set_product: int(1),
      utxo_set_witness: Accumulator::<G>::new(),
      block_height: 0,
    };
    // TODO
    unimplemented!();
  }

  fn update(&mut self, block: Block<G>) {
    // Preserves idempotency if multiple miners are leaders.
    if block.height != self.block_height + 1 {
      return;
    }

    let (elems_added, elems_deleted) = util::elems_from_transactions(&block.transactions);
    let elems_added_product: Integer = elems_added.iter().product();
    let elems_deleted_product: Integer = elems_deleted.iter().map(|(u, _wit)| u).product();

    self.utxo_set_product *= elems_added_product;
    self.utxo_set_product /= elems_deleted_product;

    // TODO: Avoid clone.
    self.utxo_set_witness = self
      .utxo_set_witness
      .clone()
      .delete(&elems_deleted)
      .unwrap()
      .0;
    self.utxo_set_witness = self.utxo_set_witness.clone().add(&elems_added).0;
    self.block_height = block.height;
  }

  /// TODO: Remove?
  fn create_aggregate_membership_witness(&self, utxos: HashSet<Utxo>) -> Accumulator<G> {
    let subproduct: Integer = utxos.iter().map(|u| hash_to_prime(u)).product();
    self
      .utxo_set_witness
      .clone()
      .exp_quotient(self.utxo_set_product.clone(), subproduct)
      .unwrap()
  }

  /// Generates individual membership witnesses for each given UTXO. See Accumulator::root_factor
  /// and BBF V3 section 4.1.
  fn create_membership_witnesses(&self, utxos: HashSet<Utxo>) -> Vec<Accumulator<G>> {
    let elems: Vec<Integer> = utxos.iter().map(|u| hash_to_prime(u)).collect();
    let agg_mem_wit = self
      .utxo_set_witness
      .clone()
      .exp_quotient(self.utxo_set_product.clone(), elems.iter().product())
      .unwrap();
    agg_mem_wit.root_factor(&elems)
  }
}
