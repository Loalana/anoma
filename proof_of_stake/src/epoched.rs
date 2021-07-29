//! [`Epoched`] and [`EpochedDelta`] are structures for data that is set for
//! future epochs at a given [`EpochOffset`].

use core::marker::PhantomData;
use core::{cmp, fmt, ops};

use crate::types::Epoch;
use crate::PosParams;

#[derive(Debug, Clone)]
pub struct Epoched<Data, Offset>
where
    Data: Clone,
    Offset: EpochOffset,
{
    /// The epoch in which this data was last updated
    last_update: Epoch,
    data: Vec<Option<Data>>,
    offset: PhantomData<Offset>,
}

#[derive(Debug, Clone)]
pub struct EpochedDelta<Data, Offset>
where
    Data: Clone + ops::Add<Output = Data>,
    Offset: EpochOffset,
{
    /// The epoch in which this data was last updated
    last_update: Epoch,
    data: Vec<Option<Data>>,
    offset: PhantomData<Offset>,
}

/// Which offset should be used to set data. The value is read from
/// [`PosParams`].
pub trait EpochOffset: fmt::Debug + Clone {
    /// Find the value of a given offset from PoS parameters.
    fn value(params: &PosParams) -> u64;
    /// Convert to [`DynEpochOffset`]
    fn dyn_offset() -> DynEpochOffset;
}
#[derive(Debug, Clone)]
pub struct OffsetPipelineLen;
impl EpochOffset for OffsetPipelineLen {
    fn value(params: &PosParams) -> u64 {
        params.pipeline_len
    }

    fn dyn_offset() -> DynEpochOffset {
        DynEpochOffset::PipelineLen
    }
}
#[derive(Debug, Clone)]
pub struct OffsetUnboundingLen;
impl EpochOffset for OffsetUnboundingLen {
    fn value(params: &PosParams) -> u64 {
        params.unbonding_len
    }

    fn dyn_offset() -> DynEpochOffset {
        DynEpochOffset::UnbondingLen
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DynEpochOffset {
    PipelineLen,
    UnbondingLen,
}
impl DynEpochOffset {
    fn value(&self, params: &PosParams) -> u64 {
        match self {
            DynEpochOffset::PipelineLen => params.pipeline_len,
            DynEpochOffset::UnbondingLen => params.unbonding_len,
        }
    }
}

impl<Data, Offset> Epoched<Data, Offset>
where
    Data: fmt::Debug + Clone,
    Offset: EpochOffset,
{
    /// Initialize new epoched data. Sets the head to the given value.
    /// This should only be used at genesis.
    pub fn init_at_genesis(
        value: Data,
        current_epoch: impl Into<Epoch>,
    ) -> Self {
        Self {
            last_update: current_epoch.into(),
            data: vec![Some(value)],
            offset: PhantomData,
        }
    }

    /// Initialize new data at the data's epoch offset.
    pub fn init(
        value: Data,
        current_epoch: impl Into<Epoch>,
        params: &PosParams,
    ) -> Self {
        let offset = Offset::value(params);
        let mut data = vec![];
        for _ in 0..offset {
            data.push(None);
        }
        data.push(Some(value));
        Self {
            last_update: current_epoch.into(),
            data,
            offset: PhantomData,
        }
    }

    /// Find the value for the given epoch.
    pub fn get(&self, epoch: impl Into<Epoch>) -> Option<&Data> {
        let epoch = epoch.into();
        let index: usize = (epoch.sub_or_default(self.last_update)).into();
        self.get_at_index(index)
    }

    /// Find the value at the offset from the given epoch.
    pub fn get_at_offset(
        &self,
        epoch: impl Into<Epoch>,
        offset: DynEpochOffset,
        params: &PosParams,
    ) -> Option<&Data> {
        let offset = offset.value(params);
        let epoch_at_offset = epoch.into() + offset;
        let index: usize =
            (epoch_at_offset.sub_or_default(self.last_update)).into();
        self.get_at_index(index)
    }

    /// Find the value at or before the given index.
    fn get_at_index(&self, offset: usize) -> Option<&Data> {
        let mut index = cmp::min(offset, self.data.len());
        loop {
            if let Some(result @ Some(_)) = self.data.get(index) {
                return result.as_ref();
            }
            if index == 0 {
                return None;
            } else {
                index -= 1;
            }
        }
    }

    /// Update the data associated with epochs, if needed. The head element is
    /// set to the latest value and any value before the current epoch is
    /// dropped.
    fn update_data(
        &mut self,
        current_epoch: impl Into<Epoch>,
        params: &PosParams,
    ) {
        let epoch = current_epoch.into();
        debug_assert!(
            epoch >= self.last_update,
            "The current epoch must be greater than or equal to the last \
             update"
        );
        let offset = Offset::value(params) as usize;
        let last_update = self.last_update;
        let shift: usize =
            cmp::min((epoch.sub_or_default(last_update)).into(), offset);

        // Resize the data if needed
        if self.data.len() < offset + 1 {
            self.data.resize_with(offset + 1, Default::default);
        }

        if shift != 0 {
            let mid_point = cmp::min(shift, self.data.len());
            let mut latest_value: Option<Data> = None;
            // Find the latest value in elements before the mid-point and clear
            // them
            for i in 0..mid_point {
                if let Some(Some(data)) = self.data.get(i) {
                    latest_value = Some(data.clone());
                }
                self.data[i] = None;
            }
            // Rotate left on the mid-point
            self.data.rotate_left(mid_point);
            // Update the head with the latest value, if it's not already set
            let current = self.data.get_mut(0).unwrap();
            if current.is_none() {
                *current = latest_value;
            }
        }

        self.last_update = epoch;
    }

    /// Set the value at the data's epoch offset.
    pub fn set(
        &mut self,
        value: Data,
        current_epoch: impl Into<Epoch>,
        params: &PosParams,
    ) {
        self.update_data(current_epoch, params);

        let offset = Offset::value(params) as usize;
        self.data[offset] = Some(value);
    }

    /// Update the values starting from the given epoch offset (which must not
    /// be greater than the `Offset` type parameter of self) with the given
    /// function.
    pub fn update_from_offset(
        &mut self,
        update_value: impl Fn(&mut Data, Epoch),
        current_epoch: impl Into<Epoch>,
        offset: DynEpochOffset,
        params: &PosParams,
    ) {
        let offset = offset.value(params) as usize;
        debug_assert!(offset <= Offset::value(params) as usize);
        let epoch = current_epoch.into();
        self.update_data(epoch, params);

        if let Some(data) = self.data.get_mut(offset).unwrap() {
            // If there's a value at `offset`, update it
            update_value(data, self.last_update + offset)
        } else {
            // Try to find if there's any value before `offset`
            let mut latest_value: Option<Data> = None;
            for i in (0..offset).rev() {
                if let Some(Some(data)) = self.data.get(i) {
                    latest_value = Some(data.clone());
                    break;
                }
            }
            // If there's a value before `offset`, update it and use it as the
            // current value
            if let Some(mut latest_value) = latest_value {
                let val_at_offset = self.data.get_mut(offset).unwrap();
                update_value(&mut latest_value, self.last_update + offset);
                *val_at_offset = Some(latest_value);
            }
        }
        // Update any data after `offset`
        for i in offset + 1..self.data.len() {
            if let Some(Some(data)) = self.data.get_mut(i) {
                update_value(data, self.last_update + i)
            }
        }
    }
}

impl<Data, Offset> EpochedDelta<Data, Offset>
where
    Data: fmt::Debug + Clone + ops::Add<Output = Data>,
    Offset: EpochOffset,
{
    /// Initialize new epoched delta data. Sets the head to the given value.
    /// This should only be used at genesis.
    pub fn init_at_genesis(
        value: Data,
        current_epoch: impl Into<Epoch>,
    ) -> Self {
        Self::init_at_index(value, current_epoch, 0)
    }

    /// Initialize new data at the data's epoch offset.
    pub fn init(
        value: Data,
        current_epoch: impl Into<Epoch>,
        params: &PosParams,
    ) -> Self {
        let index = Offset::value(params) as usize;
        Self::init_at_index(value, current_epoch, index)
    }

    /// Initialize new data at the given epoch offset (which must not be greater
    /// than the `Offset` type parameter of self).
    pub fn init_at_offset(
        value: Data,
        current_epoch: impl Into<Epoch>,
        offset: DynEpochOffset,
        params: &PosParams,
    ) -> Self {
        let index = offset.value(params) as usize;
        Self::init_at_index(value, current_epoch, index)
    }

    /// Initialize new data at the given index.
    fn init_at_index(
        value: Data,
        current_epoch: impl Into<Epoch>,
        offset: usize,
    ) -> Self {
        let mut data = vec![];
        for _ in 0..offset {
            data.push(None);
        }
        data.push(Some(value));
        Self {
            last_update: current_epoch.into(),
            data,
            offset: PhantomData,
        }
    }

    /// Find the current value for the given epoch as the sum of delta values at
    /// and before the current epoch.
    pub fn get(&self, epoch: impl Into<Epoch>) -> Option<Data> {
        let epoch = epoch.into();
        let index: usize = (epoch.sub_or_default(self.last_update)).into();
        self.get_at_index(index)
    }

    /// Find the value at the offset from the given epoch as the sum of delta
    /// values at and before the epoch offset.
    pub fn get_at_offset(
        &self,
        epoch: impl Into<Epoch>,
        offset: DynEpochOffset,
        params: &PosParams,
    ) -> Option<Data> {
        let offset = offset.value(params);
        let epoch_at_offset = epoch.into() + offset;
        let index: usize =
            (epoch_at_offset.sub_or_default(self.last_update)).into();
        self.get_at_index(index)
    }

    /// Find the value at the given index as the sum of delta values at and
    /// before the index.
    fn get_at_index(&self, offset: usize) -> Option<Data> {
        let index = cmp::min(offset, self.data.len());
        let mut sum: Option<Data> = None;
        for i in 0..index + 1 {
            if let Some(next) = self.data.get(i) {
                // Add current to the sum, if any
                match (&sum, next) {
                    (Some(current_sum), Some(next)) => {
                        sum = Some(current_sum.clone() + next.clone())
                    }
                    (None, Some(next)) => sum = Some(next.clone()),
                    _ => {}
                };
            }
        }
        sum
    }

    /// Update the data associated with epochs, if needed. Any value before the
    /// current epoch is added to the head element before being dropped.
    fn update_data(
        &mut self,
        current_epoch: impl Into<Epoch>,
        params: &PosParams,
    ) {
        let epoch = current_epoch.into();
        debug_assert!(
            epoch >= self.last_update,
            "The current epoch must be greater than or equal to the last \
             update"
        );
        let offset = Offset::value(params) as usize;
        let last_update = self.last_update;
        let shift: usize =
            cmp::min((epoch.sub_or_default(last_update)).into(), offset);

        // Resize the data if needed
        if self.data.len() < offset + 1 {
            self.data.resize_with(offset + 1, Default::default);
        }

        if shift != 0 {
            let mid_point = cmp::min(shift, self.data.len());
            let mut sum: Option<Data> = None;
            // Sum and clear all the elements before the mid-point
            for i in 0..mid_point {
                if let Some(next) = self.data.get(i) {
                    // Add current to the sum, if any
                    match (&sum, next) {
                        (Some(current_sum), Some(next)) => {
                            sum = Some(current_sum.clone() + next.clone())
                        }
                        (Some(current_sum), None) => {
                            sum = Some(current_sum.clone())
                        }
                        (None, Some(next)) => sum = Some(next.clone()),
                        _ => {}
                    };
                    // Clear the field
                    self.data[i] = None;
                }
            }
            // Rotate left on the mid-point
            self.data.rotate_left(mid_point);
            // Add the sum to the head
            let mut current = self.data.get_mut(0).unwrap();
            match (&sum, &mut current) {
                (Some(sum), Some(current)) => {
                    *current = current.clone() + sum.clone()
                }
                (Some(_), None) => *current = sum,
                _ => {}
            }
        }

        self.last_update = epoch;
    }

    /// Update or set the delta value at the data's epoch offset. If there's an
    /// existing value, it will be added to the new value.
    pub fn update(
        &mut self,
        value: Data,
        current_epoch: impl Into<Epoch>,
        params: &PosParams,
    ) {
        self.update_data(current_epoch, params);

        let offset = Offset::value(params) as usize;
        self.data[offset] = self.data[offset].as_ref().map_or_else(
            || Some(value.clone()),
            |last_delta| Some(last_delta.clone() + value.clone()),
        );
    }

    /// Update or set the delta value at the given epoch offset (which must not
    /// be greater than the `Offset` type parameter of self).
    pub fn update_at_offset(
        &mut self,
        value: Data,
        current_epoch: impl Into<Epoch>,
        offset: DynEpochOffset,
        params: &PosParams,
    ) {
        let offset = offset.value(params) as usize;
        debug_assert!(offset <= Offset::value(params) as usize);
        let epoch = current_epoch.into();
        self.update_data(epoch, params);

        self.data[offset] = self.data[offset].as_ref().map_or_else(
            || Some(value.clone()),
            |last_delta| Some(last_delta.clone() + value.clone()),
        );
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::rc::Rc;

    use proptest::prelude::*;
    use proptest::prop_state_machine;
    use proptest::state_machine::{AbstractStateMachine, StateMachineTest};

    use super::*;
    use crate::parameters::BasisPoints;
    use crate::types::tests::arb_epoch;

    prop_state_machine! {
        #[test]
        fn epoched_state_machine_with_pipeline_offset(
            sequential 1..20 => EpochedAbstractStateMachine<OffsetPipelineLen>);

        #[test]
        fn epoched_state_machine_with_unbounding_offset(
            sequential 1..20 => EpochedAbstractStateMachine<OffsetUnboundingLen>);

        #[test]
        fn epoched_delta_state_machine_with_pipeline_offset(
            sequential 1..20 => EpochedDeltaAbstractStateMachine<OffsetPipelineLen>);

        #[test]
        fn epoched_delta_state_machine_with_unbounding_offset(
            sequential 1..20 => EpochedDeltaAbstractStateMachine<OffsetUnboundingLen>);
    }

    /// Abstract representation of [`Epoched`].
    #[derive(Clone, Debug)]
    struct EpochedState<Data> {
        init_at_genesis: bool,
        params: PosParams,
        last_update: Epoch,
        data: HashMap<Epoch, Data>,
    }

    #[derive(Clone, Debug)]
    enum EpochedTransition<Data> {
        Get(Epoch),
        Set { value: Data, epoch: Epoch },
        UpdateFromOffset(UpdateFromOffset<Data>),
    }
    /// These are the arguments of one of the constructors in
    /// [`EpochedTransition`]. It's not inlined because we need to manually
    /// implement `Debug`.
    struct UpdateFromOffset<Data> {
        update_value: Rc<dyn Fn(&mut Data, Epoch)>,
        epoch: Epoch,
        offset: DynEpochOffset,
    }
    impl<Data> Clone for UpdateFromOffset<Data> {
        fn clone(&self) -> Self {
            Self {
                update_value: self.update_value.clone(),
                epoch: self.epoch,
                offset: self.offset.clone(),
            }
        }
    }
    impl<Data> fmt::Debug for UpdateFromOffset<Data> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.debug_struct("UpdateFromOffset")
                .field("update_value", &"Rc<dyn Fn(&mut Data, Epoch)>")
                .field("epoch", &self.epoch)
                .field("offset", &self.offset)
                .finish()
        }
    }

    /// Abstract state machine implementation for [`Epoched`].
    struct EpochedAbstractStateMachine<Offset: EpochOffset> {
        phantom: PhantomData<Offset>,
    }
    impl<Offset> AbstractStateMachine for EpochedAbstractStateMachine<Offset>
    where
        Offset: EpochOffset,
    {
        type State = EpochedState<u64>;
        type Transition = EpochedTransition<u64>;

        fn init_state() -> BoxedStrategy<Self::State> {
            prop_oneof![
                // Initialized at genesis
                (arb_pos_params(), 0_u64..1_000_000, any::<u64>()).prop_map(
                    |(params, epoch, initial)| {
                        let mut data = HashMap::default();
                        data.insert(epoch.into(), initial);
                        EpochedState {
                            init_at_genesis: true,
                            params,
                            last_update: epoch.into(),
                            data,
                        }
                    }
                ),
                // Initialized after genesis
                (arb_pos_params(), 0_u64..1_000_000, any::<u64>()).prop_map(
                    |(params, epoch, initial)| {
                        let offset = Offset::value(&params);
                        let mut data = HashMap::default();
                        data.insert((epoch + offset).into(), initial);
                        EpochedState {
                            init_at_genesis: false,
                            params,
                            last_update: epoch.into(),
                            data,
                        }
                    }
                ),
            ]
            .boxed()
        }

        fn transitions(state: &Self::State) -> BoxedStrategy<Self::Transition> {
            let offset = Offset::value(&state.params);
            let dyn_offset = Offset::dyn_offset();
            let last_update: u64 = state.last_update.into();
            prop_oneof![
                arb_epoch(
                    last_update.checked_sub(4 * offset).unwrap_or_default()
                        ..last_update + 4 * offset
                )
                .prop_map(EpochedTransition::Get),
                (
                    any::<u64>(),
                    // Update's epoch may not be lower than the last_update
                    arb_epoch(last_update..last_update + 10),
                )
                    .prop_map(|(value, epoch)| {
                        EpochedTransition::Set { value, epoch }
                    }),
                (
                    any::<u64>(),
                    arb_epoch(last_update..last_update + 4 * offset),
                    arb_offset(Some(dyn_offset))
                )
                    .prop_map(|(new_value, epoch, offset)| {
                        EpochedTransition::UpdateFromOffset(UpdateFromOffset {
                            update_value: Rc::new(
                                move |current: &mut u64, _epoch| {
                                    let value = new_value;
                                    *current = value
                                },
                            ),
                            epoch,
                            offset,
                        })
                    }),
            ]
            .boxed()
        }

        fn apply_abstract(
            mut state: Self::State,
            transition: &Self::Transition,
        ) -> Self::State {
            match transition {
                EpochedTransition::Get(_epoch) => {
                    // no side effects
                }
                EpochedTransition::Set { value, epoch } => {
                    let offset = Offset::value(&state.params);
                    state.last_update = *epoch;
                    state.data.insert(*epoch + offset, *value);
                }
                EpochedTransition::UpdateFromOffset(UpdateFromOffset {
                    update_value,
                    epoch,
                    offset,
                }) => {
                    state.last_update = *epoch;
                    let offset = *epoch + offset.value(&state.params);
                    if !state.data.contains_key(epoch) {
                        let mut latest_key = Epoch::default();
                        let mut latest = None;
                        for (key, value) in state.data.iter() {
                            if key > &latest_key && key < &offset {
                                latest_key = *key;
                                latest = Some(*value)
                            }
                        }
                        if let Some(latest) = latest {
                            state.data.insert(*epoch, latest);
                        }
                    }
                    for (key, value) in state.data.iter_mut() {
                        if key >= &offset {
                            update_value(value, *key)
                        }
                    }
                }
            }
            state
        }
    }

    impl<Offset> StateMachineTest for EpochedAbstractStateMachine<Offset>
    where
        Offset: EpochOffset,
    {
        type Abstract = Self;
        type ConcreteState = (PosParams, Epoched<u64, Offset>);

        fn init_test(
            initial_state: <Self::Abstract as AbstractStateMachine>::State,
        ) -> Self::ConcreteState {
            assert!(initial_state.data.len() == 1);
            let data = if initial_state.init_at_genesis {
                let genesis_epoch = initial_state.last_update;
                let value = initial_state.data.get(&genesis_epoch).unwrap();
                Epoched::init_at_genesis(*value, genesis_epoch)
            } else {
                let (key, value) = initial_state.data.iter().next().unwrap();
                let data = Epoched::init(
                    *value,
                    initial_state.last_update,
                    &initial_state.params,
                );
                assert_eq!(
                    Some(*value),
                    data.data[usize::from(*key - initial_state.last_update)]
                );
                data
            };
            (initial_state.params, data)
        }

        fn apply_concrete(
            (params, mut data): Self::ConcreteState,
            transition: &<Self::Abstract as AbstractStateMachine>::Transition,
        ) -> Self::ConcreteState {
            let offset = Offset::value(&params) as usize;
            match transition {
                EpochedTransition::Get(epoch) => {
                    let epoch = *epoch;
                    let value = data.get(epoch);
                    // Post-conditions
                    let last_update = data.last_update;
                    match value {
                        Some(val) => {
                            // When a value found, it should be the last value
                            // before or on the upper bound
                            let upper_bound = cmp::min(
                                cmp::min(
                                    (epoch.sub_or_default(last_update)).into(),
                                    offset,
                                ) + 1,
                                data.data.len(),
                            );
                            for i in (0..upper_bound).rev() {
                                match data.data[i] {
                                    Some(ref stored_val) => {
                                        assert_eq!(val, stored_val);
                                        break;
                                    }
                                    None => {
                                        // The value must be found on or after 0
                                        // index
                                        assert_ne!(i, 0);
                                    }
                                }
                            }
                        }
                        None => {
                            // When no value found, there should be no values
                            // before the upper bound
                            let upper_bound = cmp::min(
                                cmp::min(
                                    (epoch.sub_or_default(last_update)).into(),
                                    offset,
                                ) + 1,
                                data.data.len(),
                            );
                            for i in 0..upper_bound {
                                assert_eq!(None, data.data[i]);
                            }
                        }
                    }
                }
                EpochedTransition::Set { value, epoch } => {
                    let current_before_update = data.get(*epoch).copied();
                    let epochs_up_to_offset =
                        (*epoch + 1_u64).iter_range(offset as u64 - 1);
                    // Find the values in epochs up to the offset
                    let range_before_update: Vec<_> = epochs_up_to_offset
                        .clone()
                        .map(|epoch| data.get(epoch).copied())
                        .collect();

                    data.set(*value, *epoch, &params);

                    // Post-conditions
                    assert_eq!(data.last_update, *epoch);
                    assert_eq!(
                        data.data[offset as usize],
                        Some(*value),
                        "The value at offset must be updated"
                    );
                    assert!(
                        data.data.len() > offset as usize,
                        "The length of the data must be greater than the \
                         offset"
                    );
                    assert_eq!(
                        data.get(*epoch),
                        current_before_update.as_ref(),
                        "The current value must not change"
                    );
                    let range_after_update: Vec<_> = epochs_up_to_offset
                        .map(|epoch| data.get(epoch).copied())
                        .collect();
                    assert_eq!(
                        range_before_update, range_after_update,
                        "The values in epochs before the offset must not \
                         change"
                    );
                }
                EpochedTransition::UpdateFromOffset(UpdateFromOffset {
                    update_value,
                    epoch,
                    offset: update_offset,
                }) => {
                    let update_offset_val = update_offset.value(&params);
                    let current_before_update = data.get(*epoch).copied();
                    let next_epoch: u64 = (*epoch + 1_u64).into();
                    let epoch_at_update_offset: u64 =
                        (*epoch + update_offset_val).into();
                    // Find the values in epochs before the offset
                    let range_up_to_offset_before_update: Vec<_> = (next_epoch
                        ..epoch_at_update_offset)
                        .map(|i: u64| data.get(Epoch::from(i)).copied())
                        .collect();
                    let epoch_after_offset: u64 = epoch_at_update_offset + 1;
                    let epoch_at_offset: u64 = (*epoch + offset).into();
                    // Find the values in epochs after the offset
                    let mut range_from_offset_before_update: Vec<_> =
                        (epoch_after_offset..epoch_at_offset)
                            .map(|i: u64| {
                                let epoch = Epoch::from(i);
                                (data.get(epoch).copied(), epoch)
                            })
                            .collect();

                    data.update_from_offset(
                        |val, epoch| update_value(val, epoch),
                        *epoch,
                        *update_offset,
                        &params,
                    );

                    // Post-conditions
                    assert_eq!(data.last_update, *epoch);
                    // Update all the values with the update function
                    let range_from_offset_before_update: Vec<_> =
                        range_from_offset_before_update
                            .iter_mut()
                            .map(|(val, epoch)| {
                                if let Some(val) = val.as_mut() {
                                    update_value(val, *epoch);
                                }
                                *val
                            })
                            .collect();
                    let range_from_offset_after_update: Vec<_> =
                        (epoch_after_offset..epoch_at_offset)
                            .map(|i: u64| data.get(Epoch::from(i)).copied())
                            .collect();
                    assert_eq!(
                        range_from_offset_before_update,
                        range_from_offset_after_update,
                        "The values in epochs from the offset must be updated"
                    );
                    assert_eq!(
                        data.get(*epoch),
                        current_before_update.as_ref(),
                        "The current value must not change"
                    );
                    let range_up_to_offset_after_update: Vec<_> = (next_epoch
                        ..epoch_at_update_offset)
                        .map(|i: u64| data.get(Epoch::from(i)).copied())
                        .collect();
                    assert_eq!(
                        range_up_to_offset_before_update,
                        range_up_to_offset_after_update,
                        "The values in epochs up to the offset must not change"
                    );
                }
            }
            (params, data)
        }

        fn invariants((params, data): &Self::ConcreteState) {
            let offset = Offset::value(&params);
            assert!(data.data.len() <= (offset + 1) as usize);
        }
    }

    #[derive(Clone, Debug)]
    enum EpochedDeltaTransition<Data> {
        Get(Epoch),
        Update { value: Data, epoch: Epoch },
    }

    /// Abstract state machine implementation for [`EpochedDelta`].
    struct EpochedDeltaAbstractStateMachine<Offset: EpochOffset> {
        phantom: PhantomData<Offset>,
    }
    impl<Offset> AbstractStateMachine for EpochedDeltaAbstractStateMachine<Offset>
    where
        Offset: EpochOffset,
    {
        type State = EpochedState<u64>;
        type Transition = EpochedDeltaTransition<u64>;

        fn init_state() -> BoxedStrategy<Self::State> {
            prop_oneof![
                // Initialized at genesis
                (arb_pos_params(), arb_epoch(0..1_000_000), 1..10_000_000_u64)
                    .prop_map(|(params, epoch, initial)| {
                        let mut data = HashMap::default();
                        data.insert(epoch, initial);
                        EpochedState {
                            init_at_genesis: true,
                            params,
                            last_update: epoch,
                            data,
                        }
                    }),
                // Initialized after genesis
                (arb_pos_params(), arb_epoch(0..1_000_000), 1..10_000_000_u64)
                    .prop_map(|(params, epoch, initial)| {
                        let offset = Offset::value(&params);
                        let mut data = HashMap::default();
                        data.insert(epoch + offset, initial);
                        EpochedState {
                            init_at_genesis: false,
                            params,
                            last_update: epoch,
                            data,
                        }
                    }),
            ]
            .boxed()
        }

        fn transitions(state: &Self::State) -> BoxedStrategy<Self::Transition> {
            let offset = Offset::value(&state.params);
            let last_update: u64 = state.last_update.into();
            prop_oneof![
                (last_update.checked_sub(4 * offset).unwrap_or_default()
                    ..last_update + 4 * offset)
                    .prop_map(|epoch| {
                        EpochedDeltaTransition::Get(epoch.into())
                    }),
                (
                    1..10_000_000_u64,
                    // Update's epoch may not be lower than the last_update
                    last_update..last_update + 10,
                )
                    .prop_map(|(value, epoch)| {
                        EpochedDeltaTransition::Update {
                            value,
                            epoch: epoch.into(),
                        }
                    })
            ]
            .boxed()
        }

        fn apply_abstract(
            mut state: Self::State,
            transition: &Self::Transition,
        ) -> Self::State {
            match transition {
                EpochedDeltaTransition::Get(_epoch) => {
                    // no side effects
                }
                EpochedDeltaTransition::Update {
                    value: change,
                    epoch,
                } => {
                    let epoch = *epoch;
                    let offset = Offset::value(&state.params);
                    state.last_update = epoch;
                    let current = state.data.entry(epoch + offset).or_insert(0);
                    *current += *change;
                }
            }
            state
        }
    }

    impl<Offset> StateMachineTest for EpochedDeltaAbstractStateMachine<Offset>
    where
        Offset: EpochOffset,
    {
        type Abstract = Self;
        type ConcreteState = (PosParams, EpochedDelta<u64, Offset>);

        fn init_test(
            initial_state: <Self::Abstract as AbstractStateMachine>::State,
        ) -> Self::ConcreteState {
            assert!(initial_state.data.len() == 1);
            let data = if initial_state.init_at_genesis {
                let genesis_epoch = initial_state.last_update;
                let value = initial_state.data.get(&genesis_epoch).unwrap();
                EpochedDelta::init_at_genesis(*value, genesis_epoch)
            } else {
                let (key, value) = initial_state.data.iter().next().unwrap();
                let data = EpochedDelta::init(
                    *value,
                    initial_state.last_update,
                    &initial_state.params,
                );
                assert_eq!(
                    Some(*value),
                    data.data[usize::from(*key - initial_state.last_update)]
                );
                data
            };
            (initial_state.params, data)
        }

        fn apply_concrete(
            (params, mut data): Self::ConcreteState,
            transition: &<Self::Abstract as AbstractStateMachine>::Transition,
        ) -> Self::ConcreteState {
            let offset = Offset::value(&params) as usize;
            match transition {
                EpochedDeltaTransition::Get(epoch) => {
                    let epoch = *epoch;
                    let value = data.get(epoch);
                    // Post-conditions
                    let last_update = data.last_update;
                    match value {
                        Some(val) => {
                            // When a value found, it should be equal to the sum
                            // of deltas before and on the upper bound
                            let upper_bound = cmp::min(
                                cmp::min(
                                    (epoch.sub_or_default(last_update)).into(),
                                    offset,
                                ) + 1,
                                data.data.len(),
                            );
                            let mut sum = 0;
                            for i in (0..upper_bound).rev() {
                                if let Some(stored_val) = data.data[i] {
                                    sum += stored_val;
                                }
                            }
                            assert_eq!(val, sum);
                        }
                        None => {
                            // When no value found, there should be no values
                            // before the upper bound
                            let upper_bound = cmp::min(
                                cmp::min(
                                    (epoch.sub_or_default(last_update)).into(),
                                    offset,
                                ) + 1,
                                data.data.len(),
                            );
                            for i in 0..upper_bound {
                                assert_eq!(None, data.data[i]);
                            }
                        }
                    }
                }
                EpochedDeltaTransition::Update {
                    value: change,
                    epoch,
                } => {
                    let current_value_before_update = data.get(*epoch);
                    let value_at_offset_before_update =
                        data.get(*epoch + offset);
                    let epochs_up_to_offset =
                        (*epoch + 1_u64).iter_range(offset as u64 - 1);
                    // Find the values in epochs before the offset
                    let range_before_update: Vec<_> = epochs_up_to_offset
                        .clone()
                        .map(|epoch| data.get(epoch))
                        .collect();

                    data.update(*change, *epoch, &params);

                    // Post-conditions
                    assert_eq!(data.last_update, *epoch);
                    let value_at_offset_after_update =
                        data.get(*epoch + offset);
                    assert_eq!(
                        value_at_offset_after_update.unwrap_or_default(),
                        *change
                            + value_at_offset_before_update.unwrap_or_default(),
                        "The value at the offset must have increased by the \
                         change"
                    );
                    assert!(
                        data.data.len() > offset as usize,
                        "The length of the data must be greater than the \
                         offset"
                    );
                    assert_eq!(
                        data.get(*epoch),
                        current_value_before_update,
                        "The current value must not change"
                    );
                    let range_after_update: Vec<_> = epochs_up_to_offset
                        .map(|epoch| data.get(epoch))
                        .collect();
                    assert_eq!(
                        range_before_update, range_after_update,
                        "The values in epochs before the offset must not \
                         change"
                    );
                }
            }
            (params, data)
        }

        fn invariants((params, data): &Self::ConcreteState) {
            let offset = Offset::value(&params);
            assert!(data.data.len() <= (offset + 1) as usize);
        }
    }

    fn arb_pos_params() -> impl Strategy<Value = PosParams> {
        (
            10..500_u64,
            1..10_u64,
            1..10_000_u64,
            1..1_000_u64,
            1..1_000_u64,
        )
            .prop_flat_map(
                |(
                    max_validator_slots,
                    pipeline_len,
                    votes_per_token,
                    block_proposer_reward,
                    block_vote_reward,
                )| {
                    (pipeline_len + 1..pipeline_len + 10).prop_map(
                        move |unbonding_len| PosParams {
                            max_validator_slots,
                            pipeline_len,
                            unbonding_len,
                            votes_per_token: BasisPoints::new(votes_per_token),
                            block_proposer_reward,
                            block_vote_reward,
                        },
                    )
                },
            )
    }

    fn arb_offset(
        min: Option<DynEpochOffset>,
    ) -> impl Strategy<Value = DynEpochOffset> {
        match min {
            Some(DynEpochOffset::PipelineLen) => {
                Just(DynEpochOffset::PipelineLen).boxed()
            }
            Some(DynEpochOffset::UnbondingLen) | None => prop_oneof![
                Just(DynEpochOffset::PipelineLen),
                Just(DynEpochOffset::UnbondingLen),
            ]
            .boxed(),
        }
    }
}