//! Simulation runner.
//! TODO: set up communication channels and start threads for components

mod simulation;
use accumulator::group::Rsa2048;
use multiqueue::{broadcast_queue, BroadcastReceiver, BroadcastSender};
use simulation::{Bridge, Miner, User};
use std::collections::HashMap;
use std::thread;
use uuid::Uuid;

const NUM_MINERS: usize = 5;
const NUM_BRIDGES: usize = 2;
const NUM_USERS_PER_BRIDGE: usize = 25;
const BLOCK_INTERVAL_SECONDS: u64 = 30;

fn new_queue<T: Clone>() -> (BroadcastSender<T>, BroadcastReceiver<T>) {
  broadcast_queue(256)
}

pub fn main() {
  println!("Simulation starting.");
  let mut simulation_threads = Vec::new();
  let (block_sender, block_receiver) = new_queue();
  let (tx_sender, tx_receiver) = new_queue();

  for i in 0..NUM_MINERS {
    // These clones cannot go inside the thread closure, since the variable being cloned would get
    // swallowed by the move (see below as well).
    let block_sender = block_sender.clone();
    let block_receiver = block_receiver.add_stream();
    let tx_receiver = tx_receiver.add_stream();
    simulation_threads.push(thread::spawn(move || {
      Miner::<Rsa2048>::launch(
        i == 0,
        BLOCK_INTERVAL_SECONDS,
        block_sender,
        block_receiver,
        tx_receiver,
      )
    }));
  }

  for _ in 0..NUM_BRIDGES {
    let (witness_request_sender, witness_request_receiver) = new_queue();
    let mut witness_response_senders = HashMap::new();

    for _ in 0..NUM_USERS_PER_BRIDGE {
      let (witness_response_sender, witness_response_receiver) = new_queue();
      let user_id = Uuid::new_v4();
      witness_response_senders.insert(user_id, witness_response_sender);

      let witness_request_sender = witness_request_sender.clone();
      let tx_sender = tx_sender.clone();
      simulation_threads.push(thread::spawn(move || {
        User::launch(
          user_id,
          witness_request_sender,
          witness_response_receiver,
          tx_sender,
        );
      }));
    }

    let block_receiver = block_receiver.add_stream();
    simulation_threads.push(thread::spawn(move || {
      Bridge::<Rsa2048>::launch(
        block_receiver,
        witness_request_receiver,
        witness_response_senders,
      );
    }));
  }

  println!("Simulation running.");
  for thread in simulation_threads {
    thread.join().unwrap();
  }
  println!("Simulation exiting.");
}
