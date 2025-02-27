use std::{
    any::type_name,
    mem::MaybeUninit,
    ops::Index,
    panic::{AssertUnwindSafe, catch_unwind, resume_unwind},
};

use bevy_ecs::{
    component::{ComponentId, ComponentIdFor},
    prelude::*,
    query::{QueryItem, ReadOnlyQueryData},
    storage::SparseSet,
    system::{
        ReadOnlySystemParam, SystemParamItem, SystemState,
        lifetimeless::{Read, SQuery},
    },
    world::unsafe_world_cell::UnsafeWorldCell,
};
use bevy_math::prelude::*;

pub trait Measure: Component {
    type Param: ReadOnlySystemParam;
    type Item: ReadOnlyQueryData;

    fn measure(
        &self,
        param: &SystemParamItem<Self::Param>,
        item: QueryItem<Self::Item>,
        known_size: (Option<f32>, Option<f32>),
        available_space: (AvailableSpace, AvailableSpace),
    ) -> Vec2;
}

pub(crate) fn on_measure_inserted<T: Measure>(
    trigger: Trigger<OnInsert, T>,
    mut commands: Commands,
    measurements: Res<Measurements>,
    id: ComponentIdFor<T>,
) {
    let e = trigger.entity();
    commands.entity(e).insert(ContentSize(
        measurements
            .get(id.get())
            .unwrap_or_else(|| panic!("`{}` not registered", type_name::<T>())),
    ));
}

#[derive(Component, Copy, Clone)]
pub struct ContentSize(MeasureId);
impl ContentSize {
    #[inline]
    pub const fn get(self) -> MeasureId {
        self.0
    }
}

impl Default for ContentSize {
    #[inline]
    fn default() -> Self {
        Self(MeasureId::INVALID)
    }
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct MeasureId(usize);
impl MeasureId {
    pub const INVALID: Self = Self(usize::MAX);
}

#[derive(Copy, Clone, PartialEq, PartialOrd)]
pub enum AvailableSpace {
    Definite(f32),
    MinContent,
    MaxContent,
}

impl From<taffy::AvailableSpace> for AvailableSpace {
    #[inline]
    fn from(value: taffy::AvailableSpace) -> Self {
        match value {
            taffy::AvailableSpace::Definite(value) => Self::Definite(value),
            taffy::AvailableSpace::MinContent => Self::MinContent,
            taffy::AvailableSpace::MaxContent => Self::MaxContent,
        }
    }
}

#[derive(Resource, Default)]
pub struct Measurements {
    ids: SparseSet<ComponentId, MeasureId>,
    data: Vec<Box<dyn MeasureDyn>>,
}

impl Measurements {
    #[inline]
    pub fn register<T: Measure>(&mut self, world: &mut World) -> MeasureId {
        *self.ids.get_or_insert_with(world.register_component::<T>(), || {
            self.data.push(Box::new(MeasureImpl::<T> {
                state: SystemState::new(world),
                fetch: MaybeUninit::uninit(),
            }));

            MeasureId(self.data.len() - 1)
        })
    }

    #[inline]
    pub fn get(&self, id: ComponentId) -> Option<MeasureId> {
        self.ids.get(id).copied()
    }

    #[inline]
    pub unsafe fn get_measurers(&mut self, world: UnsafeWorldCell) -> impl Index<MeasureId, Output = dyn Measurer> {
        struct Guard<'a> {
            measurers: &'a mut [Box<dyn MeasureDyn>],
        }

        impl Index<MeasureId> for Guard<'_> {
            type Output = dyn Measurer;

            #[inline]
            fn index(&self, index: MeasureId) -> &Self::Output {
                &*self.measurers[index.0]
            }
        }

        impl Drop for Guard<'_> {
            fn drop(&mut self) {
                for measurer in &mut *self.measurers {
                    unsafe { measurer.finish_fetch() }
                }
            }
        }

        for (i, data) in self.data.iter_mut().enumerate() {
            if let Err(e) = catch_unwind(AssertUnwindSafe(|| unsafe { data.init_fetch(world) })) {
                for i in 0..i {
                    unsafe { self.data[i].finish_fetch() }
                }

                resume_unwind(e)
            }
        }

        Guard {
            measurers: &mut self.data,
        }
    }

    pub fn apply_measurers(&mut self, world: &mut World) {
        for data in &mut self.data {
            data.apply(world)
        }
    }
}

pub trait Measurer: 'static + Send + Sync {
    fn measure(
        &self,
        known_size: (Option<f32>, Option<f32>),
        available_space: (AvailableSpace, AvailableSpace),
        entity: Entity,
    ) -> Vec2;
}

unsafe trait MeasureDyn: Measurer {
    unsafe fn init_fetch<'w>(&'w mut self, world: UnsafeWorldCell<'w>);

    unsafe fn finish_fetch(&mut self);

    fn apply(&mut self, world: &mut World);
}

impl<T: Measure> Measurer for MeasureImpl<T> {
    #[inline]
    fn measure<'w>(
        &'w self,
        known_size: (Option<f32>, Option<f32>),
        available_space: (AvailableSpace, AvailableSpace),
        entity: Entity,
    ) -> Vec2 {
        let (param, queue) = unsafe {
            std::mem::transmute::<
                &'w SystemParamItem<'static, 'static, (T::Param, SQuery<(Read<T>, T::Item)>)>,
                &'w SystemParamItem<'w, 'w, (T::Param, SQuery<(Read<T>, T::Item)>)>,
            >(self.fetch.assume_init_ref())
        };

        let Ok((measure, item)) = queue.get(entity) else {
            return Vec2::ZERO;
        };

        measure.measure(param, item, known_size, available_space)
    }
}

unsafe impl<T: Measure> MeasureDyn for MeasureImpl<T> {
    #[inline]
    unsafe fn init_fetch<'w>(&'w mut self, world: UnsafeWorldCell<'w>) {
        self.state.update_archetypes_unsafe_world_cell(world);
        unsafe {
            self.fetch.as_mut_ptr().write(std::mem::transmute::<
                SystemParamItem<'w, 'w, (T::Param, SQuery<(Read<T>, T::Item)>)>,
                SystemParamItem<'static, 'static, (T::Param, SQuery<(Read<T>, T::Item)>)>,
            >(self.state.get_unchecked_manual(world)))
        }
    }

    #[inline]
    unsafe fn finish_fetch(&mut self) {
        unsafe { self.fetch.assume_init_drop() }
    }

    #[inline]
    fn apply(&mut self, world: &mut World) {
        self.state.apply(world)
    }
}

struct MeasureImpl<T: Measure> {
    state: SystemState<(T::Param, SQuery<(Read<T>, T::Item)>)>,
    fetch: MaybeUninit<SystemParamItem<'static, 'static, (T::Param, SQuery<(Read<T>, T::Item)>)>>,
}

unsafe impl<T: Measure> Send for MeasureImpl<T> {}
unsafe impl<T: Measure> Sync for MeasureImpl<T> {}
