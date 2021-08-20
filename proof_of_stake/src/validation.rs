//! Validation of updated PoS data

use std::borrow::Cow;
use std::collections::HashMap;
use std::convert::TryFrom;
use std::fmt::{Debug, Display};
use std::hash::Hash;
use std::ops::{Add, AddAssign, Neg, Sub, SubAssign};

use borsh::{BorshDeserialize, BorshSerialize};
use thiserror::Error;

use crate::btree_set::BTreeSetShims;
use crate::epoched::DynEpochOffset;
use crate::parameters::PosParams;
use crate::types::{
    BondId, Bonds, Epoch, TotalVotingPowers, ValidatorSets,
    ValidatorTotalDeltas, ValidatorVotingPowers, VotingPower, VotingPowerDelta,
    WeightedValidator,
};

#[allow(missing_docs)]
#[derive(Error, Debug)]
pub enum Error<Address, TokenChange>
where
    Address: Display
        + Debug
        + Clone
        + PartialEq
        + Eq
        + PartialOrd
        + Ord
        + Hash
        + BorshSerialize
        + BorshDeserialize,
    TokenChange: Debug + Display,
{
    #[error("Validator staking reward address is required for validator {0}")]
    StakingRewardAddressIsRequired(Address),
    #[error(
        "Staking reward address must be different from the validator's \
         address {0}"
    )]
    StakingRewardAddressEqValidator(Address),
    #[error("Unexpectedly missing total deltas value for validator {0}")]
    MissingValidatorTotalDeltas(Address),
    #[error("The sum of total deltas for validator {0} are negative")]
    NegativeValidatorTotalDeltasSum(Address),
    #[error("Unexpectedly missing balance value")]
    MissingBalance,
    #[error("Last update should be equal to the current epoch")]
    InvalidLastUpdate,
    #[error(
        "Invalid staking token balances. Balance delta {balance_delta}, \
         bonded {bond_delta}, unbonded {unbond_delta}, withdrawn \
         {withdraw_delta}."
    )]
    InvalidBalances {
        balance_delta: TokenChange,
        bond_delta: TokenChange,
        unbond_delta: TokenChange,
        withdraw_delta: TokenChange,
    },
    #[error(
        "Data must be set or updated in the correct epoch. Got epoch {got}, \
         expected one of {expected:?}"
    )]
    EpochedDataWrongEpoch { got: u64, expected: Vec<u64> },
    #[error("Empty bond {0} must be deleted")]
    EmptyBond(BondId<Address>),
    #[error(
        "Bond ID {id} must start at the correct epoch. Got epoch {got}, \
         expected {expected}"
    )]
    InvalidBondStartEpoch {
        id: BondId<Address>,
        got: u64,
        expected: u64,
    },
    #[error(
        "Bond ID {id} must be added at the correct epoch. Got epoch {got}, \
         expected {expected}"
    )]
    InvalidNewBondEpoch {
        id: BondId<Address>,
        got: u64,
        expected: u64,
    },
    #[error(
        "Invalid validator {address} sum of total deltas. Total delta \
         {total_delta}, bonded {bond_delta}, unbonded {unbond_delta}."
    )]
    InvalidValidatorTotalDeltasSum {
        address: Address,
        total_delta: TokenChange,
        bond_delta: TokenChange,
        unbond_delta: TokenChange,
    },
    #[error("Unexpectedly missing validator set value")]
    MissingValidatorSet,
    #[error("Validator {0} not found in the validator set in epoch {1}")]
    WeightedValidatorNotFound(WeightedValidator<Address>, u64),
    #[error("Duplicate validator {0} in the validator set in epoch {1}")]
    ValidatorSetDuplicate(WeightedValidator<Address>, u64),
    #[error("Validator {0} has an invalid total deltas value {1}")]
    InvalidValidatorTotalDeltas(Address, i128),
    #[error("There are too many active validators in the validator set")]
    TooManyActiveValidators,
    #[error(
        "An inactive validator {0} has voting power greater than an active \
         validator {1}"
    )]
    ValidatorSetOutOfOrder(
        WeightedValidator<Address>,
        WeightedValidator<Address>,
    ),
    #[error("Invalid active validator {0}")]
    InvalidActiveValidator(WeightedValidator<Address>),
    #[error("Invalid inactive validator {0}")]
    InvalidInactiveValidator(WeightedValidator<Address>),
    #[error("Unexpectedly missing voting power value for validator {0}")]
    MissingValidatorVotingPower(Address),
    #[error("Validator {0} has an invalid voting power value {1}")]
    InvalidValidatorVotingPower(Address, i64),
    #[error("Validator set should be updated when voting powers change")]
    ValidatorSetNotUpdated,
    #[error("Invalid voting power changes")]
    InvalidVotingPowerChanges,
    #[error(
        "Invalid validator {0} voting power changes. Expected {1}, but got \
         {2:?}"
    )]
    InvalidValidatorVotingPowerChange(
        Address,
        VotingPower,
        Option<VotingPower>,
    ),
    #[error("Unexpectedly missing total voting power")]
    MissingTotalVotingPower,
    #[error("Total voting power should be updated when voting powers change")]
    TotalVotingPowerNotUpdated,
    #[error(
        "Invalid total voting power change in epoch {0}. Expected {1}, but \
         got {2}"
    )]
    InvalidTotalVotingPowerChange(u64, VotingPowerDelta, VotingPowerDelta),
}

#[derive(Clone, Debug)]
pub enum DataUpdate<Address, TokenAmount, TokenChange>
where
    Address: Display
        + Debug
        + Clone
        + PartialEq
        + Eq
        + PartialOrd
        + Ord
        + Hash
        + BorshDeserialize
        + BorshSerialize,
    TokenAmount: Clone
        + Debug
        + Default
        + Eq
        + Sub
        + Add<Output = TokenAmount>
        + AddAssign
        + BorshDeserialize
        + BorshSerialize,
    TokenChange: Display
        + Debug
        + Default
        + Clone
        + Copy
        + Add<Output = TokenChange>
        + Sub<Output = TokenChange>
        + From<TokenAmount>
        + Into<i128>
        + PartialEq
        + Eq
        + BorshDeserialize
        + BorshSerialize,
{
    Balance(Data<TokenAmount>),
    Bond {
        id: BondId<Address>,
        data: Data<Bonds<TokenAmount>>,
    },
    Validator {
        address: Address,
        update: ValidatorUpdate<Address, TokenChange>,
    },
    ValidatorSet(Data<ValidatorSets<Address>>),
    TotalVotingPower(Data<TotalVotingPowers>),
}

#[derive(Clone, Debug)]
pub enum ValidatorUpdate<Address, TokenChange>
where
    Address: Clone + Debug,
    TokenChange: Display
        + Debug
        + Default
        + Clone
        + Copy
        + Add<Output = TokenChange>
        + Sub<Output = TokenChange>
        + PartialEq
        + Eq
        + BorshDeserialize
        + BorshSerialize,
{
    StakingRewardAddress(Data<Address>),
    TotalDeltas(Data<ValidatorTotalDeltas<TokenChange>>),
    VotingPowerUpdate(Data<ValidatorVotingPowers>),
}

#[derive(Clone, Debug)]
pub struct Data<T>
where
    T: Clone + Debug,
{
    /// State before the update
    pub pre: Option<T>,
    /// State after the update
    pub post: Option<T>,
}

pub fn validate<Address, TokenAmount, TokenChange>(
    params: &PosParams,
    changes: Vec<DataUpdate<Address, TokenAmount, TokenChange>>,
    current_epoch: impl Into<Epoch>,
) -> Vec<Error<Address, TokenChange>>
where
    Address: Display
        + Debug
        + Clone
        + PartialEq
        + Eq
        + PartialOrd
        + Ord
        + Hash
        + BorshDeserialize
        + BorshSerialize,
    TokenAmount: Display
        + Clone
        + Copy
        + Debug
        + Default
        + Eq
        + Sub
        + Add<Output = TokenAmount>
        + AddAssign
        + Into<u64>
        + From<u64>
        + BorshDeserialize
        + BorshSerialize,
    TokenChange: Display
        + Debug
        + Default
        + Clone
        + Copy
        + Add<Output = TokenChange>
        + Sub<Output = TokenChange>
        + Neg<Output = TokenChange>
        + SubAssign
        + AddAssign
        + From<TokenAmount>
        + Into<i128>
        + PartialEq
        + Eq
        + PartialOrd
        + Ord
        + BorshDeserialize
        + BorshSerialize,
{
    let current_epoch = current_epoch.into();
    use DataUpdate::*;
    use ValidatorUpdate::*;

    let pipeline_offset = DynEpochOffset::PipelineLen.value(params);
    let unbonding_offset = DynEpochOffset::UnbondingLen.value(params);
    let pipeline_epoch = current_epoch + pipeline_offset;
    let unbonding_epoch = current_epoch + unbonding_offset;

    let mut errors = vec![];

    let mut balance_delta = TokenChange::default();
    // Changes of validators' bonds
    let mut bond_delta: HashMap<Address, TokenChange> = HashMap::default();
    // Changes of validators' unbonds
    let mut unbond_delta: HashMap<Address, TokenChange> = HashMap::default();
    let mut withdraw_delta = TokenChange::default();

    // Changes of all validator total deltas (up to `unbonding_epoch`)
    let mut total_deltas: HashMap<Address, TokenChange> = HashMap::default();
    // Accumulative stake calculated from validator total deltas for each epoch
    // in which it has changed
    let mut total_stake_by_epoch: HashMap<
        Epoch,
        HashMap<Address, TokenAmount>,
    > = HashMap::default();
    // Accumulative validators' voting power calculated from their total deltas
    let mut expected_voting_power_by_epoch: HashMap<
        Epoch,
        HashMap<Address, VotingPower>,
    > = HashMap::default();
    // Total voting power delta calculated from validators' total deltas
    let mut expected_voting_power_delta_by_epoch: HashMap<
        Epoch,
        VotingPowerDelta,
    > = HashMap::default();
    // Changes of validators' voting power data
    let mut voting_power_by_epoch: HashMap<
        Epoch,
        HashMap<Address, VotingPower>,
    > = HashMap::default();

    let mut validator_set_pre: Option<ValidatorSets<Address>> = None;
    let mut validator_set_post: Option<ValidatorSets<Address>> = None;

    let mut total_voting_power_delta_by_epoch: HashMap<
        Epoch,
        VotingPowerDelta,
    > = HashMap::default();

    for change in changes {
        match change {
            Validator { address, update } => match update {
                StakingRewardAddress(data) => match (data.pre, data.post) {
                    (Some(_), Some(post)) => {
                        if post == address {
                            errors.push(
                                Error::StakingRewardAddressEqValidator(
                                    address.clone(),
                                ),
                            );
                        }
                    }
                    _ => errors.push(Error::StakingRewardAddressIsRequired(
                        address.clone(),
                    )),
                },
                TotalDeltas(data) => match (data.pre, data.post) {
                    (Some(pre), Some(post)) => {
                        if post.last_update() != current_epoch {
                            errors.push(Error::InvalidLastUpdate)
                        }
                        // Changes of all total deltas (up to `unbonding_epoch`)
                        let mut deltas = TokenChange::default();
                        // Sum of post total deltas
                        let mut post_deltas_sum = TokenChange::default();
                        // Iter from the first epoch to the last epoch of `post`
                        for epoch in Epoch::iter_range(
                            post.last_update(),
                            unbonding_offset + 1,
                        ) {
                            // Changes of all total deltas (up to
                            // `unbonding_epoch`)
                            let mut delta = TokenChange::default();
                            let mut pre_delta = TokenChange::default();
                            // Find the delta in `pre`
                            if let Some(change) = {
                                if epoch == post.last_update() {
                                    // On the first epoch, we have to get the
                                    // sum of all deltas at and before that
                                    // epoch as the `pre` could have been set in
                                    // an older epoch
                                    pre.get(epoch)
                                } else {
                                    pre.get_delta_at_epoch(epoch).copied()
                                }
                            } {
                                delta -= change;
                                pre_delta = change;
                            }
                            // Find the delta in `post`
                            if let Some(change) = post.get_delta_at_epoch(epoch)
                            {
                                let post_delta = *change;
                                delta += post_delta;
                                post_deltas_sum += post_delta;
                                let stake_post: i128 =
                                    Into::into(post_deltas_sum);
                                match u64::try_from(stake_post) {
                                    Ok(stake_post) => {
                                        let stake_post =
                                            TokenAmount::from(stake_post);
                                        total_stake_by_epoch
                                            .entry(epoch)
                                            .or_insert_with(HashMap::default)
                                            .insert(
                                                address.clone(),
                                                stake_post,
                                            );
                                        // Check if voting power should change
                                        let delta_pre =
                                            VotingPowerDelta::from_token_change(
                                                pre_delta, params,
                                            ).unwrap_or_default();
                                        let delta_post =
                                            VotingPowerDelta::from_token_change(
                                                post_delta, params,
                                            ).unwrap_or_default();
                                        if delta_pre != delta_post {
                                            // Accumulate expected voting power
                                            // change
                                            let voting_power_post =
                                                VotingPower::from_tokens(
                                                    stake_post, params,
                                                );
                                            expected_voting_power_by_epoch
                                                .entry(epoch)
                                                .or_insert_with(
                                                    HashMap::default,
                                                )
                                                .insert(
                                                    address.clone(),
                                                    voting_power_post,
                                                );
                                            let current_delta = expected_voting_power_delta_by_epoch.entry(epoch)
                                                .or_insert_with(Default::default);
                                            *current_delta +=
                                                delta_post - delta_pre;
                                        }
                                    }
                                    _ => errors.push(
                                        Error::InvalidValidatorTotalDeltas(
                                            address.clone(),
                                            stake_post,
                                        ),
                                    ),
                                }
                            }
                            deltas += delta;
                            // A total delta can only be increased at
                            // `pipeline_offset` from bonds and decreased at
                            // `unbonding_offset` from unbonding
                            // TODO slashing can decrease it on the current
                            if delta > TokenChange::default()
                                && epoch != pipeline_epoch
                            {
                                errors.push(Error::EpochedDataWrongEpoch {
                                    got: epoch.into(),
                                    expected: vec![pipeline_epoch.into()],
                                })
                            }
                            if delta < TokenChange::default()
                                && epoch != unbonding_epoch
                            {
                                errors.push(Error::EpochedDataWrongEpoch {
                                    got: epoch.into(),
                                    expected: vec![unbonding_epoch.into()],
                                })
                            }
                        }
                        if deltas < TokenChange::default() {
                            errors.push(Error::NegativeValidatorTotalDeltasSum(
                                address.clone(),
                            ))
                        }
                        if deltas != TokenChange::default() {
                            total_deltas.insert(address.clone(), deltas);
                        }
                    }
                    (None, Some(post)) => {
                        if post.last_update() != current_epoch {
                            errors.push(Error::InvalidLastUpdate)
                        }
                        // Changes of all total deltas (up to `unbonding_epoch`)
                        let mut deltas = TokenChange::default();
                        for epoch in Epoch::iter_range(
                            current_epoch,
                            unbonding_offset + 1,
                        ) {
                            if let Some(change) = post.get_delta_at_epoch(epoch)
                            {
                                // A new total delta can only be initialized
                                // at `pipeline_offset` (from bonds) and updated
                                // at `unbonding_offset` (from unbonding)
                                if epoch != pipeline_epoch
                                    && epoch != unbonding_epoch
                                {
                                    errors.push(Error::EpochedDataWrongEpoch {
                                        got: epoch.into(),
                                        expected: vec![pipeline_epoch.into()],
                                    })
                                }
                                deltas += *change;
                                let stake: i128 = Into::into(deltas);
                                match u64::try_from(stake) {
                                    Ok(stake) => {
                                        let stake = TokenAmount::from(stake);
                                        total_stake_by_epoch
                                            .entry(epoch)
                                            .or_insert_with(HashMap::default)
                                            .insert(address.clone(), stake);
                                        // Accumulate expected voting power
                                        // change
                                        let voting_power =
                                            VotingPower::from_tokens(
                                                stake, params,
                                            );
                                        expected_voting_power_by_epoch
                                            .entry(epoch)
                                            .or_insert_with(HashMap::default)
                                            .insert(
                                                address.clone(),
                                                voting_power,
                                            );
                                        let voting_power_delta = VotingPowerDelta::from_token_change(
                                            *change, params).unwrap_or_default();
                                        let current_delta = expected_voting_power_delta_by_epoch.entry(epoch)
                                                .or_insert_with(Default::default);
                                        *current_delta += voting_power_delta;
                                    }
                                    Err(_) => errors.push(
                                        Error::InvalidValidatorTotalDeltas(
                                            address.clone(),
                                            stake,
                                        ),
                                    ),
                                }
                            }
                        }
                        if deltas < TokenChange::default() {
                            errors.push(Error::NegativeValidatorTotalDeltasSum(
                                address.clone(),
                            ))
                        }
                        if deltas != TokenChange::default() {
                            total_deltas.insert(address.clone(), deltas);
                        }
                    }
                    (Some(_), None) => {
                        errors.push(Error::MissingValidatorTotalDeltas(address))
                    }
                    (None, None) => continue,
                },
                VotingPowerUpdate(data) => match (data.pre, data.post) {
                    (Some(_), Some(post)) | (None, Some(post)) => {
                        if post.last_update() != current_epoch {
                            errors.push(Error::InvalidLastUpdate)
                        }
                        let mut voting_power = VotingPowerDelta::default();
                        // Iter from the first epoch to the last epoch of
                        // `post`
                        for epoch in Epoch::iter_range(
                            current_epoch,
                            unbonding_offset + 1,
                        ) {
                            if let Some(delta) = post.get_delta_at_epoch(epoch)
                            {
                                voting_power += *delta;
                                let vp: i64 = Into::into(voting_power);
                                match u64::try_from(vp) {
                                    Ok(vp) => {
                                        let vp = VotingPower::from(vp);
                                        voting_power_by_epoch
                                            .entry(epoch)
                                            .or_insert_with(HashMap::default)
                                            .insert(address.clone(), vp);
                                    }
                                    Err(_) => errors.push(
                                        Error::InvalidValidatorVotingPower(
                                            address.clone(),
                                            vp,
                                        ),
                                    ),
                                }
                            }
                        }
                    }
                    (Some(_), None) => errors.push(
                        Error::MissingValidatorVotingPower(address.clone()),
                    ),
                    (None, None) => continue,
                },
            },
            Balance(data) => match (data.pre, data.post) {
                (None, Some(post)) => balance_delta += TokenChange::from(post),
                (Some(pre), Some(post)) => {
                    balance_delta -= TokenChange::from(pre);
                    balance_delta += TokenChange::from(post);
                }
                (Some(_), None) => errors.push(Error::MissingBalance),
                (None, None) => continue,
            },
            Bond { id, data } => match (data.pre, data.post) {
                // Bond may be updated from newly bonded tokens and unbonding
                (Some(pre), Some(post)) => {
                    if post.last_update() != current_epoch {
                        errors.push(Error::InvalidLastUpdate)
                    }
                    let mut total_pre_delta = TokenChange::default();
                    let mut total_post_delta = TokenChange::default();
                    // Pre-bonds keyed by their `start_epoch`
                    let mut pre_bonds: HashMap<Epoch, TokenChange> =
                        HashMap::default();
                    // Iter from the first epoch of `pre` to the last epoch of
                    // `post`
                    let pre_offset: u64 =
                        (current_epoch - pre.last_update()).into();
                    for epoch in Epoch::iter_range(
                        pre.last_update(),
                        pre_offset + pipeline_offset + 1,
                    ) {
                        if let Some(bond) = pre.get_delta_at_epoch(epoch) {
                            for (start_epoch, delta) in bond.delta.iter() {
                                let delta = TokenChange::from(*delta);
                                total_pre_delta += delta;
                                pre_bonds.insert(*start_epoch, delta);
                            }
                        }
                        if let Some(bond) = post.get_delta_at_epoch(epoch) {
                            for (start_epoch, delta) in bond.delta.iter() {
                                // On the current epoch, all bond's
                                // `start_epoch`s must be equal or lower than
                                // `current_epoch`. For all others, the
                                // `start_epoch` must be equal
                                // to the `epoch` at which it's set.
                                if (epoch == current_epoch
                                    && *start_epoch > current_epoch)
                                    || (epoch != current_epoch
                                        && *start_epoch != epoch)
                                {
                                    errors.push(Error::InvalidBondStartEpoch {
                                        id: id.clone(),
                                        got: (*start_epoch).into(),
                                        expected: epoch.into(),
                                    })
                                }
                                let delta = TokenChange::from(*delta);
                                total_post_delta += delta;

                                // Anywhere other than at `pipeline_offset`
                                // where new bonds are added, check against the
                                // data in `pre_bonds` to ensure that no new
                                // bond has been added and that the deltas are
                                // equal or lower to `pre_bonds` deltas.
                                // Note that any bonds from any epoch can be
                                // unbonded, even if they are not yet active.
                                if epoch != pipeline_epoch {
                                    match pre_bonds.get(start_epoch) {
                                        Some(pre_delta) => {
                                            if &delta > pre_delta {
                                                errors.push(
                                                Error::InvalidNewBondEpoch {
                                                    id: id.clone(),
                                                    got: epoch.into(),
                                                    expected: pipeline_epoch
                                                        .into(),
                                                });
                                            }
                                        }
                                        None => {
                                            errors.push(
                                                Error::InvalidNewBondEpoch {
                                                    id: id.clone(),
                                                    got: epoch.into(),
                                                    expected: (current_epoch
                                                        + pipeline_offset)
                                                        .into(),
                                                },
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    }
                    // An empty bond must be deleted
                    if total_post_delta == TokenChange::default() {
                        errors.push(Error::EmptyBond(id.clone()))
                    }
                    let total = total_post_delta - total_pre_delta;
                    if total != TokenChange::default() {
                        bond_delta.insert(id.validator, total);
                    }
                }
                // Bond may be created from newly bonded tokens only
                (None, Some(post)) => {
                    if post.last_update() != current_epoch {
                        errors.push(Error::InvalidLastUpdate)
                    }
                    let mut total_delta = TokenChange::default();
                    for epoch in
                        Epoch::iter_range(current_epoch, pipeline_offset + 1)
                    {
                        if let Some(bond) = post.get_delta_at_epoch(epoch) {
                            // A new bond must be initialized at
                            // `pipeline_offset`
                            if epoch != pipeline_epoch {
                                errors.push(Error::EpochedDataWrongEpoch {
                                    got: epoch.into(),
                                    expected: vec![pipeline_epoch.into()],
                                })
                            }
                            for (start_epoch, delta) in bond.delta.iter() {
                                if *start_epoch != epoch {
                                    errors.push(Error::InvalidBondStartEpoch {
                                        id: id.clone(),
                                        got: (*start_epoch).into(),
                                        expected: epoch.into(),
                                    })
                                }
                                total_delta += TokenChange::from(*delta);
                            }
                        }
                    }
                    // An empty bond must be deleted
                    if total_delta == TokenChange::default() {
                        errors.push(Error::EmptyBond(id.clone()))
                    }
                    bond_delta.insert(id.validator, total_delta);
                }
                // Bond may be deleted when all the tokens are unbonded
                (Some(pre), None) => {
                    for index in 0..pipeline_offset + 1 {
                        let index = index as usize;
                        let epoch = pre.last_update() + index;
                        if let Some(bond) = pre.get_delta_at_epoch(epoch) {
                            for delta in bond.delta.values() {
                                let delta: TokenChange =
                                    TokenChange::from(*delta);
                                bond_delta.insert(id.validator.clone(), -delta);
                            }
                        }
                    }
                }
                _ => continue,
            },
            ValidatorSet(data) => match (data.pre, data.post) {
                (Some(pre), Some(post)) => {
                    if post.last_update() != current_epoch {
                        errors.push(Error::InvalidLastUpdate)
                    }
                    validator_set_pre = Some(pre);
                    validator_set_post = Some(post);
                }
                _ => errors.push(Error::MissingValidatorSet),
            },
            TotalVotingPower(data) => match (data.pre, data.post) {
                (Some(pre), Some(post)) => {
                    if post.last_update() != current_epoch {
                        errors.push(Error::InvalidLastUpdate)
                    }
                    // Iter from the first epoch to the last epoch of `post`
                    for epoch in Epoch::iter_range(
                        post.last_update(),
                        unbonding_offset + 1,
                    ) {
                        // Find the delta in `pre`
                        let delta_pre = (if epoch == post.last_update() {
                            // On the first epoch, we have to get the
                            // sum of all deltas at and before that
                            // epoch as the `pre` could have been set in
                            // an older epoch
                            pre.get(epoch)
                        } else {
                            pre.get_delta_at_epoch(epoch).copied()
                        })
                        .unwrap_or_default();
                        // Find the delta in `post`
                        let delta_post = post
                            .get_delta_at_epoch(epoch)
                            .copied()
                            .unwrap_or_default();
                        if delta_pre != delta_post {
                            total_voting_power_delta_by_epoch
                                .insert(epoch, delta_post - delta_pre);
                        }
                    }
                }
                _ => errors.push(Error::MissingTotalVotingPower),
            },
        }
    }

    // Check total deltas against bonds and unbonds
    for (validator, total_delta) in total_deltas.iter() {
        let bond_delta =
            bond_delta.get(&validator).copied().unwrap_or_default();
        let unbond_delta =
            unbond_delta.get(&validator).copied().unwrap_or_default();
        let total_delta = *total_delta;
        if total_delta != bond_delta - unbond_delta {
            errors.push(Error::InvalidValidatorTotalDeltasSum {
                address: validator.clone(),
                total_delta,
                bond_delta,
                unbond_delta,
            })
        }
    }
    // Check that all bonds also have a total deltas update
    for validator in bond_delta.keys() {
        if !total_deltas.contains_key(validator) {
            errors.push(Error::MissingValidatorTotalDeltas(validator.clone()))
        }
    }
    // Check that all unbonds also have a total deltas update
    for validator in unbond_delta.keys() {
        if !total_deltas.contains_key(validator) {
            errors.push(Error::MissingValidatorTotalDeltas(validator.clone()))
        }
    }

    // Check validator sets against validator total stakes.
    // Iter from the first epoch to the last epoch of `validator_set_post`
    if let Some(post) = validator_set_post {
        for epoch in Epoch::iter_range(current_epoch, unbonding_offset + 1) {
            if let Some(post) = post.get_at_epoch(epoch) {
                // Check that active validators length is not over the limit
                if post.active.len() > params.max_validator_slots as usize {
                    errors.push(Error::TooManyActiveValidators)
                }
                // Check that all active have voting power >= any inactive
                if let (
                    Some(max_inactive_validator),
                    Some(min_active_validator),
                ) = (post.inactive.last_shim(), post.active.last_shim())
                {
                    if max_inactive_validator.voting_power
                        > min_active_validator.voting_power
                    {
                        errors.push(Error::ValidatorSetOutOfOrder(
                            max_inactive_validator.clone(),
                            min_active_validator.clone(),
                        ));
                    }
                }

                match validator_set_pre.as_ref().and_then(|pre| pre.get(epoch))
                {
                    Some(pre) => {
                        let total_stakes = total_stake_by_epoch
                            .get(&epoch)
                            .map(Cow::Borrowed)
                            .unwrap_or_else(|| Cow::Owned(HashMap::default()));
                        // Check active validators
                        for validator in &post.active {
                            match total_stakes.get(&validator.address) {
                                Some(stake) => {
                                    let voting_power = VotingPower::from_tokens(
                                        *stake, params,
                                    );
                                    // Any validator who's total deltas changed,
                                    // should
                                    // be up-to-date
                                    if validator.voting_power != voting_power {
                                        errors.push(
                                            Error::InvalidActiveValidator(
                                                validator.clone(),
                                            ),
                                        )
                                    }
                                }
                                None => {
                                    // Others must be the same as in pre
                                    if !pre.active.contains(validator) {
                                        errors.push(
                                            Error::InvalidActiveValidator(
                                                validator.clone(),
                                            ),
                                        )
                                    }
                                }
                            }
                        }
                        // Check inactive validators
                        for validator in &post.inactive {
                            // Any validator who's total deltas changed, should
                            // be up-to-date
                            match total_stakes.get(&validator.address) {
                                Some(stake) => {
                                    let voting_power = VotingPower::from_tokens(
                                        *stake, params,
                                    );
                                    if validator.voting_power != voting_power {
                                        errors.push(
                                            Error::InvalidInactiveValidator(
                                                validator.clone(),
                                            ),
                                        )
                                    }
                                }
                                None => {
                                    // Others must be the same as in pre
                                    if !pre.active.contains(validator) {
                                        errors.push(
                                            Error::InvalidInactiveValidator(
                                                validator.clone(),
                                            ),
                                        )
                                    }
                                }
                            }
                        }
                    }
                    None => errors.push(Error::MissingValidatorSet),
                }
            } else if let Some(total_stake) = total_stake_by_epoch.get(&epoch) {
                // When there's some total delta change for this epoch,
                // check that it wouldn't have affected the validator set
                // (i.e. the validator's voting power is unchanged).
                match post.get(epoch) {
                    Some(post) => {
                        for (validator, tokens_at_epoch) in total_stake {
                            let voting_power = VotingPower::from_tokens(
                                *tokens_at_epoch,
                                params,
                            );
                            let weighted_validator = WeightedValidator {
                                voting_power,
                                address: validator.clone(),
                            };
                            if !post.active.contains(&weighted_validator) {
                                if !post.inactive.contains(&weighted_validator)
                                {
                                    errors.push(
                                        Error::WeightedValidatorNotFound(
                                            weighted_validator,
                                            epoch.into(),
                                        ),
                                    );
                                }
                            } else if post
                                .inactive
                                .contains(&weighted_validator)
                            {
                                // Validator cannot be both active and inactive
                                errors.push(Error::ValidatorSetDuplicate(
                                    weighted_validator,
                                    epoch.into(),
                                ))
                            }
                        }
                    }
                    None => errors.push(Error::MissingValidatorSet),
                }
            }
        }
    } else if !voting_power_by_epoch.is_empty() {
        errors.push(Error::ValidatorSetNotUpdated)
    }

    // Check voting power changes against validator total stakes
    for (epoch, voting_powers) in &voting_power_by_epoch {
        if let Some(total_stakes) = total_stake_by_epoch.get(epoch) {
            for (validator, voting_power) in voting_powers {
                if let Some(stake) = total_stakes.get(&validator) {
                    let voting_power_from_stake =
                        VotingPower::from_tokens(*stake, params);
                    if *voting_power != voting_power_from_stake {
                        errors.push(Error::InvalidVotingPowerChanges)
                    }
                } else {
                    errors.push(Error::InvalidVotingPowerChanges)
                }
            }
        } else {
            errors.push(Error::InvalidVotingPowerChanges)
        }
    }

    // Check expected voting power changes
    for (epoch, expected_voting_powers) in expected_voting_power_by_epoch {
        for (validator, expected_voting_power) in expected_voting_powers {
            match voting_power_by_epoch.get(&epoch) {
                Some(actual_voting_powers) => {
                    match actual_voting_powers.get(&validator) {
                        Some(actual_voting_power) => {
                            if *actual_voting_power != expected_voting_power {
                                errors.push(
                                    Error::InvalidValidatorVotingPowerChange(
                                        validator,
                                        expected_voting_power,
                                        Some(*actual_voting_power),
                                    ),
                                );
                            }
                        }
                        None => errors.push(
                            Error::InvalidValidatorVotingPowerChange(
                                validator,
                                expected_voting_power,
                                None,
                            ),
                        ),
                    }
                }
                None => errors.push(Error::InvalidValidatorVotingPowerChange(
                    validator,
                    expected_voting_power,
                    None,
                )),
            }
        }
    }

    // Check expected total voting power change
    for (epoch, expected_delta) in expected_voting_power_delta_by_epoch {
        match total_voting_power_delta_by_epoch.get(&epoch) {
            Some(actual_delta) => {
                if *actual_delta != expected_delta {
                    errors.push(Error::InvalidTotalVotingPowerChange(
                        epoch.into(),
                        expected_delta,
                        *actual_delta,
                    ));
                }
            }
            None => {
                if expected_delta != VotingPowerDelta::default() {
                    errors.push(Error::TotalVotingPowerNotUpdated)
                }
            }
        }
    }

    // Sum the bond totals
    let bond_delta = bond_delta
        .values()
        .into_iter()
        .fold(TokenChange::default(), |acc, delta| acc + (*delta));
    // Sum the unbond totals
    let unbond_delta = unbond_delta
        .values()
        .into_iter()
        .fold(TokenChange::default(), |acc, delta| acc + (*delta));

    if balance_delta != bond_delta - unbond_delta - withdraw_delta {
        errors.push(Error::InvalidBalances {
            balance_delta,
            bond_delta,
            unbond_delta,
            withdraw_delta,
        })
    }

    errors
}