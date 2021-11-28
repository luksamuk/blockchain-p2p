use sha2::{Digest, Sha256};
use log::info;

pub const DIFFICULTY_PREFIX: &str = "00";

pub fn hash_to_binary(hash: &[u8]) -> String {
    let mut res = String::default();
    for c in hash {
        res.push_str(&format!("{:b}", c));
    }
    res
}

pub fn mine_block(id: u64, timestamp: i64, previous_hash: &str, data: &str) -> (u64, String) {
    info!("Mining block...");
    let mut nonce = 0;

    loop {
        if nonce % 100000 == 0 {
            info!("Mining. Nonce: {}", nonce);
        }

        let hash = calculate_hash(id, timestamp, previous_hash, data, nonce);
        let binary_hash = hash_to_binary(&hash);
        if binary_hash.starts_with(DIFFICULTY_PREFIX) {
            info!(
                "Mined block. Nonce: {}, hash: {}, binary hash: {}",
                nonce,
                hex::encode(&hash),
                binary_hash,
            );
            return (nonce, hex::encode(hash));
        }
        nonce += 1;
    }
}

pub fn calculate_hash(
    id: u64,
    timestamp: i64,
    previous_hash: &str,
    data: &str,
    nonce: u64,
) -> Vec<u8> {
    let data = serde_json::json!({
        "id": id,
        "previous_hash": previous_hash,
        "data": data,
        "timestamp": timestamp,
        "nonce": nonce,
    });
    let mut hasher = Sha256::new();
    hasher.update(data.to_string().as_bytes());
    hasher.finalize().as_slice().to_owned()
}
