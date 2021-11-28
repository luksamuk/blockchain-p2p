use crate::block::Block;
use crate::hash_utils::*;
use chrono::Utc;
use log::warn;

pub struct App {
    pub blocks: Vec<Block>,
}

impl App {
    pub fn new() -> Self {
        Self { blocks: vec![] }
    }

    pub fn genesis(&mut self) {
        self.blocks.push(Block {
            id: 0,
            timestamp: Utc::now().timestamp(),
            previous_hash: String::from("genesis"),
            data: String::from("genesis!"),
            nonce: 2836,
            hash: String::from("0000f816a87f806bb0073dcf026a64fb40c946b5abee2573702828694d5b4c43"),
        });
    }

    pub fn add_block(&mut self, block: Block) -> Result<(), String> {
        let latest_block = self.blocks.last().ok_or("There is not initial block")?;
        if self.is_block_valid(&block, latest_block)? {
            self.blocks.push(block);
            Ok(())
        } else {
            Err(String::from("Could not add block: invalid"))
        }
    }

    fn is_block_valid(&self, block: &Block, previous: &Block) -> Result<bool, &str> {
        if block.previous_hash != previous.hash {
            warn!("Block with id {} has wrong previous hash", block.id);
            return Ok(false);
        } else if !hash_to_binary(&hex::decode(&block.hash).expect("Hash decoding error"))
            .starts_with(DIFFICULTY_PREFIX)
        {
            warn!(
                "Block with id {} is not the next block after block with id {}",
                block.id, previous.id
            );
            return Ok(false);
        } else if hex::encode(calculate_hash(
            block.id,
            block.timestamp,
            &block.previous_hash,
            &block.data,
            block.nonce,
        )) != block.hash
        {
            warn!("Block with id {} has invalid hash", block.id);
            return Ok(false);
        }
        Ok(true)
    }

    fn is_chain_valid(&self, chain: &[Block]) -> Result<bool, &str> {
        for i in 0..chain.len() {
            // Ignore genesis block
            if i == 0 {
                continue;
            }

            let first = chain
                .get(i - 1)
                .ok_or("Could not get first block of pair on analysed chain")?;
            let second = chain
                .get(i)
                .ok_or("Could not get second block of pair on analysed chain")?;
            if !self.is_block_valid(second, first)? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    pub fn choose_chain(
        &mut self,
        local: Vec<Block>,
        remote: Vec<Block>,
    ) -> Result<Vec<Block>, &str> {
        let local_valid = self.is_chain_valid(&local)?;
        let remote_valid = self.is_chain_valid(&remote)?;

        Ok(if local_valid && remote_valid {
            if local.len() >= remote.len() {
                local
            } else {
                remote
            }
        } else if remote_valid && !local_valid {
            remote
        } else if !remote_valid && local_valid {
            local
        } else {
            return Err("Both local and remote chains are invalid!");
        })
    }
}
