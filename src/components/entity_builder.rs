use std::any::Any;
use std::borrow::Borrow;
use std::ops::DerefMut;

use futures::SinkExt;
use legion::query::{FilterResult, LayoutFilter};
use legion::storage::{
    ArchetypeSource, ArchetypeWriter, Component, ComponentSource, ComponentTypeId, ComponentWriter,
    EntityLayout, IntoComponentSource, PackedStorage, UnknownComponentStorage,
};
use legion::*;

use crate::components::*;

pub struct EntityBuilder<'a> {
    buffer: legion::systems::CommandBuffer,
    entity: Entity,
    world: &'a mut World,
}

impl<'a> EntityBuilder<'a> {
    pub fn new(world: &'a mut World) -> Self {
        let mut buffer = legion::systems::CommandBuffer::new(world);
        let entity = buffer.push(());
        return Self {
            buffer,
            entity,
            world,
        };
    }
    pub fn build(&mut self) { self.buffer.flush(self.world) }
    fn add_component<T: Component>(&mut self, component: T) {
        self.buffer.add_component(self.entity, component);
    }
    pub fn position(&mut self, pos: Vector2<f32>) -> &mut Self {
        self.add_component(Position(pos));
        return self;
    }
    pub fn velocity(&mut self, vel: Vector2<f32>) -> &mut Self {
        self.add_component(Velocity(vel));
        return self;
    }
    pub fn orientation(&mut self, ori: f32) -> &mut Self {
        self.add_component(Orientation(Deg(ori)));
        return self;
    }
    pub fn dynamic_body(&mut self, mass: f32) -> &mut Self {
        self.add_component(DynamicBody { mass });
        return self;
    }
    pub fn agent(&mut self, accel: Acceleration) -> &mut Self {
        self.add_component(accel);
        return self;
    }
}
