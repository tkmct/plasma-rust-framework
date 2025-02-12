use crate::db::RangeAtBlockRecord;
use crate::property_executor::PropertyExecutor;
use crate::types::{PlasmaDataBlock, PropertyInput, QuantifierResult, QuantifierResultItem};
use abi_utils::Decodable;
use bytes::Bytes;
use ethereum_types::H256;
use merkle_interval_tree::{DoubleLayerTree, DoubleLayerTreeLeaf};
use plasma_db::traits::kvs::KeyValueStore;
use plasma_db::traits::rangestore::RangeStore;

pub struct BlockRangeQuantifier {}

impl Default for BlockRangeQuantifier {
    fn default() -> Self {
        Self {}
    }
}

impl BlockRangeQuantifier {
    pub fn verify_exclusion(plasma_data_block: &PlasmaDataBlock, inclusion_proof: &Bytes) -> bool {
        let leaf = DoubleLayerTreeLeaf {
            address: plasma_data_block.get_deposit_contract_address(),
            end: plasma_data_block.get_updated_range().get_end(),
            data: Bytes::from(H256::zero().as_bytes()),
        };
        DoubleLayerTree::verify(&leaf, inclusion_proof.clone(), plasma_data_block.get_root())
    }
    pub fn get_all_quantified<KVS>(
        decider: &PropertyExecutor<KVS>,
        inputs: &[PropertyInput],
    ) -> QuantifierResult
    where
        KVS: KeyValueStore,
    {
        let block_number = decider.get_variable(&inputs[0]).to_integer();
        let range = decider.get_variable(&inputs[1]).to_range();
        let result = decider
            .get_range_db()
            .bucket(&Bytes::from("range_at_block"))
            .bucket(&block_number.into())
            .get(range.get_start(), range.get_end())
            .unwrap();
        let sum = result
            .iter()
            .filter_map(|r| r.get_intersection(range.get_start(), range.get_end()))
            .fold(0, |acc, r| acc + r.get_end() - r.get_start());
        let mut full_range_included: bool = sum == (range.get_end() - range.get_start());
        let plasma_data_blocks: Vec<PlasmaDataBlock> = result
            .iter()
            .map(|r| RangeAtBlockRecord::from_abi(r.get_value()).unwrap())
            .filter_map(move |record| {
                if record.plasma_data_block.get_is_included() {
                    Some(record.plasma_data_block.clone())
                } else {
                    if !Self::verify_exclusion(&record.plasma_data_block, &record.inclusion_proof) {
                        full_range_included = false
                    }
                    None
                }
            })
            .collect();
        QuantifierResult::new(
            plasma_data_blocks
                .iter()
                .map(|p| QuantifierResultItem::StateUpdate(p.clone().into()))
                .collect(),
            full_range_included,
        )
    }
}
