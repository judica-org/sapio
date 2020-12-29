use super::*;
use bitcoin::hashes::Hash;
/// Any type which can generate a CTVHash. Allows some decoupling in the future if some types will
/// not be literal transactions.
pub trait CTVHash {
    fn get_ctv_hash(&self, input_index: u32) -> sha256::Hash;
    fn total_amount(&self) -> Amount;
}
impl CTVHash for bitcoin::Transaction {
    /// Uses BIP-119 Logic to compute a CTV Hash
    fn get_ctv_hash(&self, input_index: u32) -> sha256::Hash {
        let mut ctv_hash = sha256::Hash::engine();
        self.version.consensus_encode(&mut ctv_hash).unwrap();
        self.lock_time.consensus_encode(&mut ctv_hash).unwrap();
        (self.input.len() as u32)
            .consensus_encode(&mut ctv_hash)
            .unwrap();
        {
            let mut enc = sha256::Hash::engine();
            for seq in self.input.iter().map(|i| i.sequence) {
                seq.consensus_encode(&mut enc).unwrap();
            }
            sha256::Hash::from_engine(enc)
                .into_inner()
                .consensus_encode(&mut ctv_hash)
                .unwrap();
        }

        (self.output.len() as u32)
            .consensus_encode(&mut ctv_hash)
            .unwrap();

        {
            let mut enc = sha256::Hash::engine();
            for out in self.output.iter() {
                out.consensus_encode(&mut enc).unwrap();
            }
            sha256::Hash::from_engine(enc)
                .into_inner()
                .consensus_encode(&mut ctv_hash)
                .unwrap();
        }
        input_index.consensus_encode(&mut ctv_hash).unwrap();
        sha256::Hash::from_engine(ctv_hash)
    }

    fn total_amount(&self) -> Amount {
        Amount::from_sat(self.output.iter().fold(0, |a, b| a + b.value))
    }
}