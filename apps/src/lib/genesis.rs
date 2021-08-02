//! The parameters used for the chain's genesis

use anoma::ledger::parameters::{EpochDuration, Parameters};
use anoma::ledger::pos::{GenesisValidator, PosParams};
#[cfg(feature = "dev")]
use anoma::types::key::ed25519::Keypair;

#[derive(Debug)]
pub struct Genesis {
    #[cfg(not(feature = "dev"))]
    pub validators: Vec<GenesisValidator>,
    #[cfg(feature = "dev")]
    pub validator: GenesisValidator,
    #[cfg(feature = "dev")]
    pub validator_key: Keypair,
    pub parameters: Parameters,
    pub pos_params: PosParams,
}

#[cfg(feature = "dev")]
pub fn genesis() -> Genesis {
    use anoma::types::address::Address;
    use anoma::types::token;

    // NOTE When the validator's key changes, tendermint must be reset with
    // `anoma reset` command. To generate a new validator, use the
    // `tests::gen_genesis_validator` below.
    let keypair = Keypair::from_bytes(&[
        // SecretKey bytes
        80, 110, 166, 33, 135, 254, 34, 138, 253, 44, 214, 71, 50, 230, 39, 246,
        124, 201, 68, 138, 194, 251, 192, 36, 55, 160, 211, 68, 65, 189, 121,
        217, // PublicKey bytes
        94, 112, 76, 78, 70, 38, 94, 28, 204, 135, 80, 81, 73, 247, 155, 157,
        46, 65, 77, 1, 164, 227, 128, 109, 252, 101, 240, 167, 57, 1, 193, 208,
    ])
    .unwrap();
    let staking_reward_keypair = Keypair::from_bytes(&[
        61, 198, 87, 204, 44, 94, 234, 228, 217, 72, 245, 27, 40, 2, 151, 174,
        24, 247, 69, 6, 9, 30, 44, 16, 88, 238, 77, 162, 243, 125, 240, 206,
        111, 92, 66, 23, 105, 211, 33, 236, 5, 208, 17, 88, 177, 112, 100, 154,
        1, 132, 143, 67, 162, 121, 136, 247, 20, 67, 4, 27, 226, 63, 47, 57,
    ])
    .unwrap();
    let address = Address::decode("a1qq5qqqqqgfqnsd6pxse5zdj9g5crzsf5x4zyzv6yxerr2d2rxpryzwp5g5m5zvfjxv6ygsekjmraj0").unwrap();
    let staking_reward_address = Address::decode("a1qq5qqqqqxaz5vven8yu5gdpng9zrys6ygvurwv3sgsmrvd6xgdzrys6yg4pnwd6z89rrqv2xvjcy9t").unwrap();
    let validator = GenesisValidator {
        address,
        staking_reward_address,
        tokens: token::Amount::whole(100_000),
        consensus_key: keypair.public.clone().into(),
        staking_reward_key: staking_reward_keypair.public.clone().into(),
    };
    let parameters = Parameters {
        epoch_duration: EpochDuration {
            min_num_of_blocks: 10,
            min_duration: anoma::types::time::Duration::minutes(1).into(),
        },
    };
    Genesis {
        validator,
        validator_key: keypair,
        parameters,
        pos_params: PosParams::default(),
    }
}

#[cfg(test)]
pub mod tests {
    use anoma::types::address::testing::gen_established_address;
    use anoma::types::key::ed25519::Keypair;
    use rand::prelude::ThreadRng;
    use rand::thread_rng;

    /// Run `cargo test gen_genesis_validator -- --nocapture` to generate a
    /// new genesis validator address, staking reward address and keypair.
    #[test]
    fn gen_genesis_validator() {
        let address = gen_established_address();
        let staking_reward_address = gen_established_address();
        let mut rng: ThreadRng = thread_rng();
        let keypair = Keypair::generate(&mut rng);
        let staking_reward_keypair = Keypair::generate(&mut rng);
        println!("address: {}", address);
        println!("staking_reward_address: {}", staking_reward_address);
        println!("keypair: {:?}", keypair.to_bytes());
        println!(
            "staking_reward_keypair: {:?}",
            staking_reward_keypair.to_bytes()
        );
    }
}
