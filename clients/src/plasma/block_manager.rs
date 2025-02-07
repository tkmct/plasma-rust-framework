use super::block_db::BlockDb;
use super::command::NewTransactionEvent;
use super::error::Error;
use super::plasma_block::PlasmaBlock;
use contract_wrapper::commitment_contract_adaptor::CommitmentContractAdaptor;
use ethabi::Contract as ContractABI;
use ethereum_types::Address;
use ovm::types::{Integer, StateUpdate};
use plasma_db::traits::db::DatabaseTrait;
use plasma_db::traits::kvs::KeyValueStore;
use plasma_db::RangeDbImpl;
use std::fs::File;
use std::io::BufReader;

pub struct BlockManager<KVS: KeyValueStore> {
    db: RangeDbImpl<KVS>,
    commitment_contract_address: Address,
    aggregator_address: Address,
    current_block_number: u64,
}

impl<KVS: KeyValueStore + DatabaseTrait> BlockManager<KVS> {
    pub fn new(aggregator_address: Address, commitment_contract_address: Address) -> Self {
        let db = KVS::open("plasma_aggregator_db");
        let db = RangeDbImpl::from(db);

        BlockManager {
            aggregator_address,
            commitment_contract_address,
            db,
            current_block_number: 1,
        }
    }

    pub fn get_queued_state_updates(&self) -> Vec<StateUpdate> {
        let block_db = BlockDb::from(&self.db);
        block_db.get_pending_state_updates().unwrap()
    }

    pub fn enqueue_state_update(&self, state_update: &StateUpdate) -> Result<(), Error> {
        let block_db = BlockDb::from(&self.db);
        block_db
            .enqueue_state_update(state_update)
            .map_err::<Error, _>(Into::into)
    }

    pub fn enqueue_tx(&self, tx: NewTransactionEvent) -> Result<(), Error> {
        let block_db = BlockDb::from(&self.db);
        block_db.enqueue_tx(tx).map_err::<Error, _>(Into::into)
    }

    /// generate block from queued state updates
    /// save block in block_db, submit to CommitmentContract
    /// return generated block
    pub fn submit_next_block(&mut self) -> Result<(), Error> {
        let block_db = BlockDb::from(&self.db);
        let state_updates = block_db
            .get_pending_state_updates()
            .map_err::<Error, _>(Into::into)?;
        let transactions = block_db.get_pending_txs().map_err::<Error, _>(Into::into)?;
        let mut block = PlasmaBlock::new(self.current_block_number, state_updates, transactions);

        let root = block.merkelize()?;

        // send root hash to commitment contract
        let f = File::open("../contract-wrapper/CommitmentChain.json").unwrap();
        let reader = BufReader::new(f);
        let contract_abi = ContractABI::load(reader).unwrap();
        let contract = CommitmentContractAdaptor::new(
            "http://127.0.0.1:8545",
            self.commitment_contract_address,
            contract_abi,
        )
        .unwrap();
        let _ = contract.submit_block(self.aggregator_address, block.get_block_number(), root)?;

        let _ = block_db.save_block(&block);
        let _ = block_db.delete_all_queued_state_updates();
        let _ = block_db.delete_all_queued_txs();
        self.save_next_block_number(self.get_next_block_number());
        Ok(())
    }

    pub fn get_block_range(&self, block_number: Integer) -> Result<PlasmaBlock, Error> {
        let block_db = BlockDb::from(&self.db);
        block_db.get_block(block_number)
    }

    pub fn get_current_block_number(&self) -> u64 {
        self.current_block_number
    }

    pub fn get_next_block_number(&self) -> u64 {
        self.current_block_number + 1
    }

    pub fn save_next_block_number(&mut self, block_number: u64) {
        self.current_block_number = block_number;
    }
}
