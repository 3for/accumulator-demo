use super::state::{Block, Utxo};
use accumulator::group::UnknownOrderGroup;
use accumulator::hash::hash_to_prime;
use accumulator::Accumulator;
use multiqueue::{BroadcastReceiver, BroadcastSender};
use rug::Integer;
use std::clone::Clone;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::thread;
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct WitnessRequest {
  pub user_id: Uuid,
  pub request_id: Uuid,
  pub utxos: Vec<Utxo>,
}

#[derive(Clone, Debug)]
pub struct WitnessResponse<G: UnknownOrderGroup> {
  pub request_id: Uuid,
  pub witnesses: Vec<(Utxo, Accumulator<G>)>,
}

#[allow(dead_code)]
#[derive(Clone)]
pub struct Bridge<G: UnknownOrderGroup> {
  utxo_set_product: Integer,
  utxo_set_witness: Accumulator<G>,
  block_height: u64,
  user_ids: HashSet<Uuid>,
}

impl<G: UnknownOrderGroup> Bridge<G> {
  /// Assumes all bridges are online from genesis. We may want to implement syncing later.
  /// Also assumes that bridge/user relationships are fixed in `main`.
  #[allow(unused_variables)]
  #[allow(clippy::type_complexity)]
  pub fn start(
    utxo_set_witness: Accumulator<G>,
    utxo_set_product: Integer,
    block_receiver: BroadcastReceiver<Block<G>>,
    witness_request_receiver: BroadcastReceiver<WitnessRequest>,
    witness_response_senders: HashMap<Uuid, BroadcastSender<WitnessResponse<G>>>,
    utxo_update_senders: HashMap<Uuid, BroadcastSender<(Vec<Utxo>, Vec<Utxo>)>>,
  ) {
    let bridge_ref = Arc::new(Mutex::new(Bridge {
      utxo_set_product,
      utxo_set_witness,
      block_height: 0,
      user_ids: utxo_update_senders.keys().cloned().collect(),
    }));

    // Block updater thread.
    let bridge = bridge_ref.clone();
    let update_thread = thread::spawn(move || {
      for block in block_receiver {
        bridge.lock().unwrap().update(block, &utxo_update_senders);
      }
    });

    // Witness request handler.
    let bridge = bridge_ref.clone();
    let witness_thread = thread::spawn(move || {
      for request in witness_request_receiver {
        let witnesses = bridge
          .lock()
          .unwrap()
          .create_membership_witnesses(request.utxos);
        witness_response_senders[&request.user_id]
          .try_send(WitnessResponse {
            request_id: request.request_id,
            witnesses,
          })
          .unwrap();
      }
    });

    update_thread.join().unwrap();
    witness_thread.join().unwrap();
  }

  #[allow(clippy::type_complexity)]
  fn update(
    &mut self,
    block: Block<G>,
    utxo_update_senders: &HashMap<Uuid, BroadcastSender<(Vec<Utxo>, Vec<Utxo>)>>,
  ) {
    // Preserves idempotency if multiple miners are leaders.
    if block.height != self.block_height + 1 {
      return;
    }

    let mut user_updates = HashMap::new();
    for user_id in self.user_ids.iter() {
      user_updates.insert(user_id, (Vec::new(), Vec::new()));
    }

    for transaction in block.transactions {
      for deletion in transaction.utxos_deleted {
        if self.user_ids.contains(&deletion.0.user_id) {
          user_updates
            .get_mut(&deletion.0.user_id)
            .unwrap()
            .0
            .push(deletion.0.clone());
          self.utxo_set_product /= hash_to_prime(&deletion.0);
        }
      }
      for addition in transaction.utxos_added {
        if self.user_ids.contains(&addition.user_id) {
          user_updates
            .get_mut(&addition.user_id)
            .unwrap()
            .1
            .push(addition.clone());
          self.utxo_set_product *= hash_to_prime(&addition);
        }
      }
    }

    self.utxo_set_witness = block
      .new_acc
      .delete(&[(self.utxo_set_product.clone(), self.utxo_set_witness.clone())])
      .unwrap()
      .0;
    self.block_height = block.height;

    for (user_id, update) in user_updates {
      utxo_update_senders[user_id].try_send(update).unwrap();
    }
  }

  /// Generates individual membership witnesses for each given UTXO. See Accumulator::root_factor
  /// and BBF V3 section 4.1.
  fn create_membership_witnesses(&self, utxos: Vec<Utxo>) -> Vec<(Utxo, Accumulator<G>)> {
    let elems: Vec<Integer> = utxos.iter().map(|u| hash_to_prime(u)).collect();
    let agg_mem_wit = self
      .utxo_set_witness
      .clone()
      .exp_quotient(self.utxo_set_product.clone(), elems.iter().product())
      .unwrap();
    let witnesses = agg_mem_wit.root_factor(&elems);
    // ideally root factor would return the zipped version internally
    utxos
      .iter()
      .zip(witnesses.iter())
      .map(|(x, y)| (x.clone(), y.clone()))
      .collect()
  }
}
